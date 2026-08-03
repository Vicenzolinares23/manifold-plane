"""Input guardrails — prompt injection / jailbreak / off-topic heuristics."""

from __future__ import annotations

import re
from typing import Any

from manifold_agent.config import get_settings
from manifold_agent.state import GuardrailReport

_INJECTION = re.compile(
    r"(ignore (all|any|previous|prior) instructions|system prompt|jailbreak|"
    r"dan mode|developer mode|bypass (safety|guard)|do anything now)",
    re.I,
)
_OFF_TOPIC = re.compile(r"(write me malware|how to make a bomb)", re.I)


def check_input(user_message: str, context: dict[str, Any] | None = None) -> GuardrailReport:
    settings = get_settings().guardrails
    text = user_message or ""
    if _INJECTION.search(text):
        return GuardrailReport(
            allowed=False,
            reason="prompt_injection_heuristic",
            risk=0.95,
            details={"match": "injection"},
        )
    if _OFF_TOPIC.search(text):
        return GuardrailReport(
            allowed=False,
            reason="off_topic_disallowed",
            risk=0.9,
            details={"match": "off_topic"},
        )
    risk = 0.1 if len(text) < 4000 else 0.4
    allowed = risk < settings.input_risk_threshold
    return GuardrailReport(allowed=allowed, reason="ok" if allowed else "input_risk", risk=risk)
