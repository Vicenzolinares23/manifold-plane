"""Delegation tools."""

from __future__ import annotations

from manifold_agent.tools import tool_registry


@tool_registry
def delegate(agent_id: str, task: str = "") -> str:
    """Spawn or hand work to another agent (coupling)."""
    return f"[delegate] would hand '{task}' to {agent_id}"
