"""Local write tools."""

from __future__ import annotations

from manifold_agent.tools import tool_registry


@tool_registry
def write_local(path: str, content: str = "") -> str:
    """Write data inside the trust boundary."""
    return f"[write_local] wrote {len(content)} bytes to {path}"
