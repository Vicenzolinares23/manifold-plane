"""LangGraph nodes: agent, gate, tools, memory, replan."""

from __future__ import annotations

from typing import Any, Literal, TypedDict

from manifold_agent.gateway import DaemonError, decide, heuristic_verdict
from manifold_agent.state import Decision, ToolCall, ToolKind, Verdict
from manifold_agent.tools.registry import get_tool, load_all_tools


class AgentState(TypedDict, total=False):
    asker_id: str
    symmetry_class: str
    messages: list[dict[str, Any]]
    pending_tools: list[dict[str, Any]]
    last_verdict: dict[str, Any] | None
    last_results: list[str]
    route: str
    deny_reason: str


def heuristic_classify(name: str, arguments: dict[str, Any]) -> ToolCall:
    """Fallback classifier when guardrails.classify is unavailable."""
    mapping = {
        "read_local": ToolKind.READ_LOCAL,
        "read_external": ToolKind.READ_EXTERNAL,
        "write_local": ToolKind.WRITE_LOCAL,
        "send_external": ToolKind.SEND_EXTERNAL,
        "execute": ToolKind.EXECUTE,
        "self_modify": ToolKind.SELF_MODIFY,
        "delegate": ToolKind.DELEGATE,
        "remember": ToolKind.SELF_MODIFY,
        "recall": ToolKind.READ_LOCAL,
        "forget": ToolKind.SELF_MODIFY,
    }
    kind = mapping.get(name, ToolKind.EXECUTE)
    payload = len(str(arguments).encode("utf-8"))
    tainted = bool(arguments.get("tainted") or arguments.get("argument_tainted"))
    recipients = int(arguments.get("recipients") or (1 if kind == ToolKind.SEND_EXTERNAL else 0))
    return ToolCall(
        name=name,
        kind=kind,
        arguments=arguments,
        payload_bytes=int(arguments.get("payload_bytes") or payload),
        recipients=recipients,
        argument_tainted=tainted,
        off_transcript=bool(arguments.get("off_transcript")),
        source_sensitivity=float(arguments.get("source_sensitivity") or 0.01),
    )


def classify_tool_call(name: str, arguments: dict[str, Any], context: dict[str, Any] | None = None) -> ToolCall:
    try:
        from manifold_agent.guardrails.classify import classify_tool_call as _classify

        return _classify(name, arguments, context or {})
    except Exception:
        return heuristic_classify(name, arguments)


def agent_node(state: AgentState) -> AgentState:
    """In dry-run / scripted mode, pending_tools are already set by the caller."""
    msgs = list(state.get("messages") or [])
    if not state.get("pending_tools"):
        msgs.append({"role": "assistant", "content": "No tool calls pending."})
    return {**state, "messages": msgs, "route": "gate" if state.get("pending_tools") else "end"}


def gate_node(state: AgentState) -> AgentState:
    load_all_tools()
    pending = list(state.get("pending_tools") or [])
    if not pending:
        return {**state, "route": "end"}

    call_spec = pending[0]
    name = call_spec["name"]
    args = dict(call_spec.get("arguments") or {})
    tool_call = classify_tool_call(name, args, {"asker_id": state.get("asker_id")})

    try:
        verdict = decide(
            state.get("asker_id") or "agent",
            tool_call,
            symmetry_class=state.get("symmetry_class"),
        )
    except DaemonError:
        verdict = heuristic_verdict(tool_call)

    try:
        from manifold_agent.db.persist import record_tool_event

        record_tool_event(
            asker_key=state.get("asker_id") or "agent",
            thread_id=state.get("thread_id") or f"thread-{state.get('asker_id') or 'agent'}",
            tool_call=tool_call,
            verdict=verdict,
            symmetry_class=state.get("symmetry_class") or "default",
        )
    except Exception:
        pass

    route: Literal["tools", "replan", "hold", "end"]
    if verdict.decision == Decision.ADMIT:
        route = "tools"
    elif verdict.decision == Decision.HOLD:
        route = "hold"
    else:
        route = "replan"

    return {
        **state,
        "last_verdict": verdict.model_dump(),
        "deny_reason": "" if route == "tools" else f"{verdict.decision.value}: margin_after={verdict.margin_after}",
        "route": route,
        "_admitted_call": tool_call.model_dump(),  # type: ignore[typeddict-unknown-key]
    }


def tools_node(state: AgentState) -> AgentState:
    """Execute only the first pending tool after an admit."""
    load_all_tools()
    pending = list(state.get("pending_tools") or [])
    if not pending:
        return {**state, "route": "memory"}
    call_spec = pending[0]
    tool = get_tool(call_spec["name"])
    if tool is None:
        result = f"unknown tool: {call_spec['name']}"
        ok = False
    else:
        try:
            result = str(tool.invoke(call_spec.get("arguments") or {}))
            ok = True
        except Exception as exc:  # noqa: BLE001
            result = f"tool error: {exc}"
            ok = False
    results = list(state.get("last_results") or [])
    results.append(result)
    msgs = list(state.get("messages") or [])
    msgs.append({"role": "tool", "content": result, "ok": ok})
    return {
        **state,
        "pending_tools": pending[1:],
        "last_results": results,
        "messages": msgs,
        "route": "memory",
    }


def memory_node(state: AgentState) -> AgentState:
    """Hook for persisting episode crumbs; long-term writes go through tools."""
    return {**state, "route": "agent" if state.get("pending_tools") else "end"}


def replan_node(state: AgentState) -> AgentState:
    msgs = list(state.get("messages") or [])
    msgs.append(
        {
            "role": "system",
            "content": f"Tool denied by admission engine: {state.get('deny_reason')}. Choose a safer step.",
        }
    )
    # Drop the denied call so we do not loop forever in dry-run.
    pending = list(state.get("pending_tools") or [])
    return {**state, "messages": msgs, "pending_tools": pending[1:], "route": "agent"}


def hold_node(state: AgentState) -> AgentState:
    msgs = list(state.get("messages") or [])
    msgs.append({"role": "system", "content": "HOLD: human review required before tool execution."})
    pending = list(state.get("pending_tools") or [])
    return {**state, "messages": msgs, "pending_tools": pending[1:], "route": "end"}


def route_from_gate(state: AgentState) -> str:
    return state.get("route") or "end"


def route_after_agent(state: AgentState) -> str:
    return state.get("route") or "end"
