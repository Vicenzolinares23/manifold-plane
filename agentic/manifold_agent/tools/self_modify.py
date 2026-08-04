"""Self-modification tools."""

from __future__ import annotations

from manifold_agent.tools import tool_registry


@tool_registry
def self_modify(setting: str, value: str = "") -> str:
    """Change the agent's own configuration, permissions, or memory policy."""
    return f"[self_modify] would set {setting}={value}"
