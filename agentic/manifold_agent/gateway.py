"""HTTP gateway to manifold-planed (`POST /admit` agent domain)."""

from __future__ import annotations

from typing import Any

import httpx

from manifold_agent.config import get_settings
from manifold_agent.state import Decision, ToolCall, Verdict


class DaemonError(RuntimeError):
    """Raised when the engine daemon cannot be reached or returns an error."""


def decide(
    asker_id: str,
    tool_call: ToolCall,
    *,
    symmetry_class: str | None = None,
    at: float | None = None,
    engine_url: str | None = None,
    timeout: float = 5.0,
) -> Verdict:
    """POST ``/admit`` with an agent tool-call payload and return a typed Verdict.

    ``at`` is accepted for Stage 8 call-site compatibility; the daemon stamps
    decision time itself.
    """
    _ = at
    settings = get_settings()
    base = (engine_url or settings.engine_url).rstrip("/")
    payload: dict[str, Any] = {
        "asker": asker_id,
        "class": symmetry_class or settings.symmetry_class,
        "label": tool_call.name,
        "agent": {
            "kind": tool_call.kind.value,
            "payload_bytes": tool_call.payload_bytes,
            "recipients": tool_call.recipients,
            "argument_tainted": tool_call.argument_tainted,
            "off_transcript": tool_call.off_transcript,
            "source_sensitivity": tool_call.source_sensitivity,
        },
    }
    try:
        with httpx.Client(timeout=timeout) as client:
            resp = client.post(f"{base}/admit", json=payload)
            resp.raise_for_status()
            data = resp.json()
    except httpx.HTTPError as exc:
        raise DaemonError(f"engine admit failed: {exc}") from exc

    return Verdict(
        decision=Decision(data["decision"]),
        admissible_fraction=float(data.get("admissible_fraction", 0.0)),
        coalitions_checked=int(data.get("coalitions_checked", 0)),
        blocked_by_coalition=data.get("blocked_by_coalition"),
        margin_before=float(data["margin_before"]),
        margin_after=float(data["margin_after"]),
        required=float(data["required"]),
        alpha_effective=float(data["alpha_effective"]),
        orbit_residual=float(data["orbit_residual"]),
        budget_fraction=float(data["budget_fraction"]),
        state_after=list(data.get("state_after") or [0.0] * 6),
        denied=int(data.get("denied", 0)),
        held=int(data.get("held", 0)),
        admitted=int(data.get("admitted", 0)),
    )


def heuristic_verdict(tool_call: ToolCall) -> Verdict:
    """Offline fallback when the daemon is down — conservative stubs for demos/tests."""
    risky = tool_call.kind.value in {"SendExternal", "Execute", "SelfModify"} and (
        tool_call.argument_tainted or tool_call.payload_bytes > 4096
    )
    decision = Decision.DENY if risky else Decision.ADMIT
    return Verdict(
        decision=decision,
        admissible_fraction=0.0 if risky else 1.0,
        coalitions_checked=0,
        blocked_by_coalition=None,
        margin_before=100.0,
        margin_after=90.0 if not risky else -1.0,
        required=95.0,
        alpha_effective=0.05,
        orbit_residual=0.0,
        budget_fraction=0.1,
        state_after=[0.0] * 6,
        denied=1 if risky else 0,
        held=0,
        admitted=0 if risky else 1,
    )
