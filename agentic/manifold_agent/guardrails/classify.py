"""Heuristic (+ optional fine-tuned) tool-call measurement layer."""

from __future__ import annotations

from typing import Any

from manifold_agent.state import ToolCall, ToolKind

_NAME_TO_KIND: dict[str, ToolKind] = {
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


def _heuristic(name: str, arguments: dict[str, Any]) -> ToolCall:
    kind = _NAME_TO_KIND.get(name)
    if kind is None:
        lowered = name.lower()
        if "http" in lowered or "fetch" in lowered or "web" in lowered:
            kind = ToolKind.READ_EXTERNAL
        elif "send" in lowered or "post" in lowered or "email" in lowered:
            kind = ToolKind.SEND_EXTERNAL
        elif "shell" in lowered or "exec" in lowered:
            kind = ToolKind.EXECUTE
        else:
            kind = ToolKind.READ_LOCAL

    payload = arguments.get("payload_bytes")
    if payload is None:
        blob = arguments.get("payload") or arguments.get("content") or arguments.get("command") or ""
        payload = len(str(blob).encode("utf-8")) if blob else len(str(arguments).encode("utf-8"))

    recipients = int(arguments.get("recipients") or 0)
    if kind == ToolKind.SEND_EXTERNAL and recipients <= 0:
        recipients = 1

    tainted = bool(
        arguments.get("argument_tainted")
        or arguments.get("tainted")
        or arguments.get("from_external")
    )
    sensitivity = float(arguments.get("source_sensitivity") or (0.8 if tainted else 0.01))

    return ToolCall(
        name=name,
        kind=kind,
        arguments=dict(arguments),
        payload_bytes=int(payload),
        recipients=recipients,
        argument_tainted=tainted,
        off_transcript=bool(arguments.get("off_transcript")),
        source_sensitivity=sensitivity,
    )


def classify_tool_call(
    name: str,
    arguments: dict[str, Any],
    context: dict[str, Any] | None = None,
) -> ToolCall:
    """Fill ToolCall measurement fields. Fine-tuned model first when configured."""
    context = context or {}
    try:
        from manifold_agent.training.classify_integration import try_model_classify

        modeled = try_model_classify(name, arguments, context)
        if modeled is not None:
            return modeled
    except Exception:
        pass
    return _heuristic(name, arguments)
