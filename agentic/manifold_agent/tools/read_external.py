"""External read tools."""

from __future__ import annotations

from manifold_agent.tools import tool_registry


@tool_registry
def read_external(url: str) -> str:
    """Fetch content from outside the trust boundary (web/API)."""
    return f"[read_external] stub body from {url}"
