"""Optional fine-tuned classifier seam for classify_tool_call."""

from __future__ import annotations

from typing import Any

from manifold_agent.state import ToolCall


def try_model_classify(
    name: str,
    arguments: dict[str, Any],
    context: dict[str, Any] | None = None,
) -> ToolCall | None:
    """Return a ToolCall if a trained adapter is configured and confident; else None.

    Default implementation always returns None so heuristics remain authoritative
    until a model is registered via MP_CLASSIFIER_PATH.
    """
    import os

    path = os.getenv("MP_CLASSIFIER_PATH")
    if not path:
        return None
    # Placeholder: loading LoRA adapters is environment-specific.
    _ = (name, arguments, context, path)
    return None
