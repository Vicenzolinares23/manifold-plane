"""Local read tools."""

from __future__ import annotations

from manifold_agent.tools import tool_registry


@tool_registry
def read_local(path: str = ".") -> str:
    """Read a local file or list a directory inside the trust boundary."""
    return f"[read_local] stub content for {path}"
