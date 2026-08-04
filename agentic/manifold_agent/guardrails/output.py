"""Output guardrail: PII / secret exfiltration and length heuristics."""

from __future__ import annotations

import re
from typing import Any

from manifold_agent.config import get_settings
from manifold_agent.state import GuardrailReport

# Deliberately broad patterns for training/demo secrets — not production scanners.
_SECRET_PATTERNS: list[tuple[str, re.Pattern[str], float]] = [
    (
        "aws_access_key",
        re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
        0.95,
    ),
    (
        "openai_sk",
        re.compile(r"\bsk-[A-Za-z0-9]{20,}\b"),
        0.95,
    ),
    (
        "generic_api_key",
        re.compile(
            r"(?i)\b(?:api[_-]?key|secret[_-]?key|access[_-]?token)\s*[:=]\s*['\"]?[A-Za-z0-9_\-]{16,}",
        ),
        0.9,
    ),
    (
        "bearer_token",
        re.compile(r"(?i)\bbearer\s+[A-Za-z0-9\-_\.]{20,}\b"),
        0.9,
    ),
    (
        "private_key_block",
        re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
        0.98,
    ),
    (
        "slack_token",
        re.compile(r"\bxox[baprs]-[0-9A-Za-z-]{10,}\b"),
        0.95,
    ),
    (
        "github_pat",
        re.compile(r"\bghp_[A-Za-z0-9]{20,}\b"),
        0.95,
    ),
]

_PII_PATTERNS: list[tuple[str, re.Pattern[str], float]] = [
    (
        "ssn",
        re.compile(r"\b\d{3}-\d{2}-\d{4}\b"),
        0.85,
    ),
    (
        "email",
        re.compile(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"),
        0.4,
    ),
    (
        "phone_us",
        re.compile(r"\b(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b"),
        0.45,
    ),
    (
        "credit_card",
        re.compile(r"\b(?:\d[ -]*?){13,19}\b"),
        0.7,
    ),
]

_DEFAULT_MAX_CHARS = 50_000


def check_output(
    content: str,
    context: dict[str, Any] | None = None,
) -> GuardrailReport:
    """Flag model/tool output that looks like secrets or sensitive PII."""
    ctx = dict(context or {})
    settings = get_settings().guardrails
    text = content or ""
    details: dict[str, Any] = {"matches": []}
    risk = 0.0
    reasons: list[str] = []

    max_chars = int(ctx.get("max_chars", _DEFAULT_MAX_CHARS))
    if len(text) > max_chars:
        details["matches"].append("too_long")
        details["length"] = len(text)
        reasons.append("too_long")
        risk = max(risk, 0.7)

    for name, pattern, weight in _SECRET_PATTERNS:
        if pattern.search(text):
            details["matches"].append(name)
            reasons.append(name)
            risk = max(risk, weight)

    # Email alone is weak signal; only escalate when combined or forced.
    email_hits = 0
    for name, pattern, weight in _PII_PATTERNS:
        hits = pattern.findall(text)
        if not hits:
            continue
        if name == "email":
            email_hits = len(hits)
            details["email_count"] = email_hits
            if email_hits >= 5 or ctx.get("strict_pii"):
                details["matches"].append(name)
                reasons.append(name)
                risk = max(risk, weight)
            continue
        if name == "credit_card":
            # Require digit length sanity to reduce false positives on hashes.
            for hit in hits:
                digits = re.sub(r"\D", "", hit)
                if 13 <= len(digits) <= 19:
                    details["matches"].append(name)
                    reasons.append(name)
                    risk = max(risk, weight)
                    break
            continue
        details["matches"].append(name)
        reasons.append(name)
        risk = max(risk, weight)

    threshold = float(ctx.get("risk_threshold", settings.output_risk_threshold))
    allowed = risk < threshold
    reason = "ok" if allowed else ("blocked:" + ",".join(reasons) if reasons else "blocked")
    return GuardrailReport(
        allowed=allowed,
        reason=reason,
        risk=risk,
        details=details,
        model=None,
    )
