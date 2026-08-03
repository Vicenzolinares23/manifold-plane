"""Outbound send tools."""

from __future__ import annotations

from manifold_agent.tools import tool_registry


@tool_registry
def send_external(url: str, payload: str = "", recipients: int = 1) -> str:
    """Send data outward (HTTP POST, webhook, email). Irreversible."""
    return f"[send_external] sent {len(payload)} bytes to {url} ({recipients} recipients)"
