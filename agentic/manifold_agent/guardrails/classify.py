"""Heuristic (and optional fine-tuned) tool-call classifier.

Maps a tool name + arguments into the measurement fields the agent adapter
needs: kind, payload_bytes, recipients, taint, sensitivity, off_transcript.
"""

from __future__ import annotations

import json
import re
from typing import Any

from manifold_agent.state import ToolCall, ToolKind

# Ordered from most specific / highest-risk outward effect to generic reads.
_KIND_RULES: list[tuple[ToolKind, tuple[str, ...]]] = [
    (
        ToolKind.SEND_EXTERNAL,
        (
            "send_external",
            "send_email",
            "email",
            "webhook",
            "http_post",
            "http_put",
            "http_patch",
            "upload",
            "post_message",
            "notify",
            "slack_send",
            "telegram_send",
            "exfil",
        ),
    ),
    (
        ToolKind.READ_EXTERNAL,
        (
            "read_external",
            "web_fetch",
            "web_search",
            "http_get",
            "browse",
            "fetch_url",
            "scrape",
            "download",
        ),
    ),
    (
        ToolKind.EXECUTE,
        (
            "execute",
            "shell",
            "bash",
            "run_command",
            "run_code",
            "python_exec",
            "subprocess",
            "os_system",
        ),
    ),
    (
        ToolKind.SELF_MODIFY,
        (
            "self_modify",
            "update_config",
            "set_permission",
            "grant_permission",
            "change_policy",
            "disable_logging",
            "mute_audit",
        ),
    ),
    (
        ToolKind.DELEGATE,
        (
            "delegate",
            "spawn_agent",
            "handoff",
            "subagent",
            "fork_agent",
        ),
    ),
    (
        ToolKind.WRITE_LOCAL,
        (
            "write_local",
            "write_file",
            "save_file",
            "create_file",
            "append_file",
            "delete_file",
            "mkdir",
        ),
    ),
    (
        ToolKind.READ_LOCAL,
        (
            "read_local",
            "read_file",
            "cat",
            "open_file",
            "list_dir",
            "list_files",
            "stat",
            "grep_local",
            "search_files",
        ),
    ),
]

_SENSITIVE_KEYS = frozenset(
    {
        "password",
        "secret",
        "token",
        "api_key",
        "apikey",
        "authorization",
        "private_key",
        "credential",
        "ssn",
        "credit_card",
    }
)

_SENSITIVE_PATH_RE = re.compile(
    r"(?:^|/)(?:\.env|id_rsa|credentials|secrets?(?:\.|/)|passwd|shadow)(?:$|/|\.)",
    re.IGNORECASE,
)

_CONTENT_KEYS = (
    "content",
    "body",
    "text",
    "data",
    "payload",
    "message",
    "query",
    "code",
    "html",
)


def _normalize_name(name: str) -> str:
    return re.sub(r"[\s\-]+", "_", name.strip().lower())


def infer_kind(name: str) -> ToolKind:
    """Map a tool name to ToolKind via substring heuristics."""
    n = _normalize_name(name)
    for kind, needles in _KIND_RULES:
        for needle in needles:
            if needle in n:
                return kind
    # Verb-ish fallbacks when the name is free-form.
    if any(v in n for v in ("send", "post", "mail", "upload", "publish")):
        return ToolKind.SEND_EXTERNAL
    if any(v in n for v in ("fetch", "browse", "http", "url", "web")):
        return ToolKind.READ_EXTERNAL
    if any(v in n for v in ("exec", "shell", "bash", "run")):
        return ToolKind.EXECUTE
    if any(v in n for v in ("write", "save", "create", "delete", "put")):
        return ToolKind.WRITE_LOCAL
    if any(v in n for v in ("read", "open", "list", "get", "load")):
        return ToolKind.READ_LOCAL
    return ToolKind.READ_LOCAL


def _estimate_payload_bytes(arguments: dict[str, Any]) -> int:
    if "payload_bytes" in arguments:
        try:
            return max(0, int(arguments["payload_bytes"]))
        except (TypeError, ValueError):
            pass
    for key in ("size", "nbytes", "byte_count", "length"):
        if key in arguments:
            try:
                return max(0, int(arguments[key]))
            except (TypeError, ValueError):
                pass
    total = 0
    for key in _CONTENT_KEYS:
        if key in arguments and arguments[key] is not None:
            val = arguments[key]
            if isinstance(val, (bytes, bytearray)):
                total += len(val)
            elif isinstance(val, str):
                total += len(val.encode("utf-8"))
            else:
                total += len(json.dumps(val, default=str).encode("utf-8"))
    if total:
        return total
    # Fall back to serialized argument size so empty-arg tools stay at 0.
    try:
        encoded = json.dumps(arguments, default=str).encode("utf-8")
        # Ignore trivial empty-object encoding `{}`.
        return 0 if encoded in (b"{}", b"null") else len(encoded)
    except (TypeError, ValueError):
        return 0


def _estimate_recipients(arguments: dict[str, Any], kind: ToolKind) -> int:
    if "recipients" in arguments:
        try:
            return max(0, int(arguments["recipients"]))
        except (TypeError, ValueError):
            pass
    for key in ("to", "recipient", "recipients_list", "cc", "bcc", "channels"):
        if key not in arguments:
            continue
        val = arguments[key]
        if isinstance(val, list):
            return max(1, len(val)) if kind == ToolKind.SEND_EXTERNAL else len(val)
        if isinstance(val, str) and val.strip():
            parts = [p for p in re.split(r"[,;]", val) if p.strip()]
            return max(1, len(parts))
    if kind == ToolKind.SEND_EXTERNAL:
        for key in ("url", "endpoint", "webhook_url", "channel"):
            if arguments.get(key):
                return 1
        return 1
    return 0


def _path_from_args(arguments: dict[str, Any]) -> str:
    for key in ("path", "file", "filepath", "filename", "uri"):
        val = arguments.get(key)
        if isinstance(val, str):
            return val
    return ""


def _source_sensitivity(arguments: dict[str, Any], kind: ToolKind, context: dict[str, Any]) -> float:
    if "source_sensitivity" in arguments:
        try:
            return float(arguments["source_sensitivity"])
        except (TypeError, ValueError):
            pass
    if "source_sensitivity" in context:
        try:
            return float(context["source_sensitivity"])
        except (TypeError, ValueError):
            pass

    sensitivity = 0.01
    path = _path_from_args(arguments)
    if path and _SENSITIVE_PATH_RE.search(path):
        sensitivity = max(sensitivity, 0.8)
    lowered_keys = {str(k).lower() for k in arguments}
    if lowered_keys & _SENSITIVE_KEYS:
        sensitivity = max(sensitivity, 0.7)
    blob = json.dumps(arguments, default=str).lower()
    if any(k in blob for k in _SENSITIVE_KEYS):
        sensitivity = max(sensitivity, 0.5)
    if kind == ToolKind.SEND_EXTERNAL and context.get("session_tainted"):
        sensitivity = max(sensitivity, 0.6)
    if kind in (ToolKind.READ_LOCAL, ToolKind.WRITE_LOCAL) and "secret" in path.lower():
        sensitivity = max(sensitivity, 0.9)
    return min(1.0, sensitivity)


_LEAKY_KINDS = frozenset(
    {
        ToolKind.SEND_EXTERNAL,
        ToolKind.EXECUTE,
        ToolKind.SELF_MODIFY,
        ToolKind.DELEGATE,
        ToolKind.WRITE_LOCAL,
    }
)


def _argument_tainted(
    arguments: dict[str, Any],
    context: dict[str, Any],
    kind: ToolKind,
) -> bool:
    if "argument_tainted" in arguments:
        return bool(arguments["argument_tainted"])
    if "argument_tainted" in context:
        return bool(context["argument_tainted"])

    tainted_keys = context.get("tainted_keys") or context.get("tainted_fields") or ()
    if tainted_keys and any(key in arguments for key in tainted_keys):
        return True

    # Classic prompt-injection signature: outbound/escalating call after an
    # external read in this session (docs/00 / agent adapter).
    session_dirty = bool(
        context.get("session_tainted")
        or context.get("had_external_read")
        or context.get("external_reads")
    )
    if session_dirty and kind in _LEAKY_KINDS:
        return True
    return False


def _off_transcript(arguments: dict[str, Any], context: dict[str, Any]) -> bool:
    if "off_transcript" in arguments:
        return bool(arguments["off_transcript"])
    if context.get("off_transcript"):
        return True
    return bool(arguments.get("silent") or arguments.get("no_log") or arguments.get("hidden"))


def classify_heuristic(
    name: str,
    arguments: dict[str, Any] | None = None,
    context: dict[str, Any] | None = None,
) -> ToolCall:
    """Pure-heuristic classification used as the default measurement layer."""
    args = dict(arguments or {})
    ctx = dict(context or {})
    kind = infer_kind(name)

    return ToolCall(
        name=name,
        kind=kind,
        arguments=args,
        payload_bytes=_estimate_payload_bytes(args),
        recipients=_estimate_recipients(args, kind),
        argument_tainted=_argument_tainted(args, ctx, kind),
        off_transcript=_off_transcript(args, ctx),
        source_sensitivity=_source_sensitivity(args, kind, ctx),
    )


def classify_tool_call(
    name: str,
    arguments: dict[str, Any] | None = None,
    context: dict[str, Any] | None = None,
) -> ToolCall:
    """Classify a tool call into engine-facing measurement fields.

    Tries a fine-tuned classifier when one is configured, then falls back to
    heuristics on low confidence or load failure.
    """
    args = dict(arguments or {})
    ctx = dict(context or {})

    try:
        from manifold_agent.training.classify_integration import try_model_classify

        model_call = try_model_classify(name, args, ctx)
        if model_call is not None:
            return model_call
    except Exception:
        # Seam must never break the gate: heuristics are the floor.
        pass

    return classify_heuristic(name, args, ctx)
