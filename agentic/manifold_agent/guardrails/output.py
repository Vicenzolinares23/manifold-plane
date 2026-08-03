"""Output guardrails — PII / secret exfiltration patterns."""

from __future__ import annotations

import re
from typing import Any

from manifold_agent.config import get_settings
from manifold_agent.state import GuardrailReport

_SECRET = re.compile(
    r"(AKIA[0-9A-Z]{16}|sk-[A-Za-z0-9]{20,}|-----BEGIN (RSA |OPENSSH )?PRIVATE KEY-----|"
    r"xox[baprs]-[A-Za-z0-9-]{10,})",
    re.I,
)
_EMAIL = re.compile(r"[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}", re.I)
_SSN = re.compile(r"\b\d{3}-\d{2}-\d{4}\b")


def check_output(content: str, context: dict[str, Any] | None = None) -> GuardrailReport:
    settings = get_settings().guardrails
    text = content or ""
    if _SECRET.search(text):
        return GuardrailReport(
            allowed=False,
            reason="secret_pattern",
            risk=0.99,
            details={"match": "secret"},
        )
    pii_hits = len(_EMAIL.findall(text)) + len(_SSN.findall(text))
    risk = min(0.2 + 0.2 * pii_hits, 0.95)
    if len(text) > 50_000:
        risk = max(risk, 0.7)
    allowed = risk < settings.output_risk_threshold
    return GuardrailReport(
        allowed=allowed,
        reason="ok" if allowed else "output_risk",
        risk=risk,
        details={"pii_hits": pii_hits},
    )
