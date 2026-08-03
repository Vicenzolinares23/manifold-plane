"""Composed gate: input → classify → optional engine → output → GateReport."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from manifold_agent.gateway import DaemonError, decide, heuristic_verdict
from manifold_agent.guardrails.classify import classify_tool_call
from manifold_agent.guardrails.input import check_input
from manifold_agent.guardrails.output import check_output
from manifold_agent.state import Decision, GateReport, GuardrailReport, ToolCall, Verdict

EngineDecide = Callable[[ToolCall, dict[str, Any]], Verdict | None]


def gate(
    user_message: str | None = None,
    tool_name: str | None = None,
    arguments: dict[str, Any] | None = None,
    context: dict[str, Any] | None = None,
    *,
    output_content: str | None = None,
    engine_decide: EngineDecide | None = None,
) -> GateReport:
    """Run the full guardrail pipeline into one ``GateReport``."""
    ctx = dict(context or {})
    args = dict(arguments or {})

    input_check = (
        check_input(user_message, ctx)
        if user_message is not None
        else GuardrailReport(allowed=True, reason="skipped", risk=0.0)
    )
    if not input_check.allowed:
        empty = ToolCall(name=tool_name or "", arguments=args)
        return GateReport(
            input_check=input_check,
            tool_call=empty,
            verdict=None,
            output_check=None,
            admissible=False,
            reason=input_check.reason,
        )

    name = tool_name or str(ctx.get("tool_name") or "")
    tool_call = classify_tool_call(name, args, ctx) if name else ToolCall(name="", arguments=args)

    verdict: Verdict | None = None
    if engine_decide is not None and name:
        try:
            verdict = engine_decide(tool_call, ctx)
        except Exception as exc:  # noqa: BLE001
            return GateReport(
                input_check=input_check,
                tool_call=tool_call,
                verdict=None,
                output_check=None,
                admissible=False,
                reason=f"engine_error:{exc}",
            )

    if verdict is not None and verdict.decision == Decision.DENY:
        return GateReport(
            input_check=input_check,
            tool_call=tool_call,
            verdict=verdict,
            output_check=None,
            admissible=False,
            reason=f"engine:{verdict.decision.value}",
        )

    output_check: GuardrailReport | None = None
    if output_content is not None:
        output_check = check_output(output_content, ctx)
        if not output_check.allowed:
            return GateReport(
                input_check=input_check,
                tool_call=tool_call,
                verdict=verdict,
                output_check=output_check,
                admissible=False,
                reason=output_check.reason,
            )

    reason = "ok"
    if verdict is not None and verdict.decision == Decision.HOLD:
        reason = "engine:hold"
    return GateReport(
        input_check=input_check,
        tool_call=tool_call,
        verdict=verdict,
        output_check=output_check,
        admissible=True,
        reason=reason,
    )


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
    """Convenience wrapper that calls the live daemon (or heuristic fallback)."""

    def _engine(tool_call: ToolCall, _ctx: dict[str, Any]) -> Verdict:
        try:
            return decide(asker_id, tool_call, symmetry_class=symmetry_class)
        except DaemonError:
            return heuristic_verdict(tool_call)

    return gate(
        user_message=user_message,
        tool_name=tool_name,
        arguments=arguments,
        context=context,
        output_content=output_preview,
        engine_decide=_engine,
    )
