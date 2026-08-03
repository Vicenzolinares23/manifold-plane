"""Compose input → classify → engine → output into a GateReport."""

from __future__ import annotations

from typing import Any

from manifold_agent.gateway import DaemonError, decide, heuristic_verdict
from manifold_agent.guardrails.classify import classify_tool_call
from manifold_agent.guardrails.input import check_input
from manifold_agent.guardrails.output import check_output
from manifold_agent.state import Decision, GateReport, GuardrailReport, ToolCall


def run_gate(
    *,
    user_message: str,
    tool_name: str,
    arguments: dict[str, Any],
    asker_id: str,
    symmetry_class: str = "default",
    output_preview: str | None = None,
    context: dict[str, Any] | None = None,
) -> GateReport:
    context = context or {}
    input_check = check_input(user_message, context)
    if not input_check.allowed:
        return GateReport(
            input_check=input_check,
            tool_call=ToolCall(name=tool_name, arguments=arguments),
            admissible=False,
            reason=input_check.reason,
        )

    tool_call = classify_tool_call(tool_name, arguments, context)
    try:
        verdict = decide(asker_id, tool_call, symmetry_class=symmetry_class)
    except DaemonError:
        verdict = heuristic_verdict(tool_call)

    if verdict.decision != Decision.ADMIT:
        return GateReport(
            input_check=input_check,
            tool_call=tool_call,
            verdict=verdict,
            admissible=False,
            reason=f"engine_{verdict.decision.value}",
        )

    output_check: GuardrailReport | None = None
    if output_preview is not None:
        output_check = check_output(output_preview, context)
        if not output_check.allowed:
            return GateReport(
                input_check=input_check,
                tool_call=tool_call,
                verdict=verdict,
                output_check=output_check,
                admissible=False,
                reason=output_check.reason,
            )

    return GateReport(
        input_check=input_check,
        tool_call=tool_call,
        verdict=verdict,
        output_check=output_check,
        admissible=True,
        reason="ok",
    )
