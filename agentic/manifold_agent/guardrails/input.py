"""Input guardrail: prompt-injection, jailbreak, and off-topic heuristics."""

from __future__ import annotations

import re
from typing import Any

from manifold_agent.config import get_settings
from manifold_agent.state import GuardrailReport

_INJECTION_PATTERNS: list[tuple[str, re.Pattern[str], float]] = [
    (
        "ignore_instructions",
        re.compile(
            r"ignore\s+(all\s+)?(previous|prior|above)\s+(instructions|rules|prompts)",
            re.IGNORECASE,
        ),
        0.95,
    ),
    (
        "jailbreak",
        re.compile(
            r"\b(dan\s+mode|developer\s+mode|jailbreak|do\s+anything\s+now)\b",
            re.IGNORECASE,
        ),
        0.95,
    ),
    (
        "role_hijack",
        re.compile(
            r"(?:you\s+are\s+now|act\s+as|pretend\s+to\s+be)\s+(?:an?\s+)?(?:unrestricted|evil|root|system)",
            re.IGNORECASE,
        ),
        0.9,
    ),
    (
        "system_override",
        re.compile(
            r"(?:system\s*prompt|override\s+safety|disable\s+(?:safety|guardrails|filters))",
            re.IGNORECASE,
        ),
        0.9,
    ),
    (
        "exfil_coercion",
        re.compile(
            r"(?:exfiltrat|send\s+(?:all|the)\s+(?:secrets?|keys?|credentials?)\s+(?:to|via))",
            re.IGNORECASE,
        ),
        0.85,
    ),
    (
        "hidden_instruction",
        re.compile(
            r"(?:<\s*(?:system|instructions?)\s*>|\[(?:INST|SYSTEM)\])",
            re.IGNORECASE,
        ),
        0.8,
    ),
]

_OFF_TOPIC = re.compile(
    r"^\s*(?:write\s+(?:me\s+)?(?:a\s+)?(?:poem|song|joke)|tell\s+me\s+a\s+story)\b",
    re.IGNORECASE,
)


def check_input(
    user_message: str,
    context: dict[str, Any] | None = None,
) -> GuardrailReport:
    """Score a user message for injection / jailbreak / off-topic risk."""
    ctx = dict(context or {})
    settings = get_settings().guardrails
    text = user_message or ""
    details: dict[str, Any] = {"matches": []}
    risk = 0.0
    reasons: list[str] = []

    if not text.strip():
        return GuardrailReport(
            allowed=False,
            reason="empty_input",
            risk=0.5,
            details={"matches": ["empty"]},
        )

    for name, pattern, weight in _INJECTION_PATTERNS:
        if pattern.search(text):
            details["matches"].append(name)
            reasons.append(name)
            risk = max(risk, weight)

    if ctx.get("strict_topic") and _OFF_TOPIC.search(text):
        details["matches"].append("off_topic")
        reasons.append("off_topic")
        risk = max(risk, 0.6)

    # Optional model judge is a seam; heuristics remain the floor.
    if settings.use_judge:
        details["judge_requested"] = True
        details["judge_model"] = settings.judge_model

    threshold = float(ctx.get("risk_threshold", settings.input_risk_threshold))
    allowed = risk < threshold
    reason = "ok" if allowed else ("blocked:" + ",".join(reasons) if reasons else "blocked")
    return GuardrailReport(
        allowed=allowed,
        reason=reason,
        risk=risk,
        details=details,
        model=settings.judge_model if settings.use_judge else None,
    )
