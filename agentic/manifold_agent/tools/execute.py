"""Code/shell execution tools."""

from __future__ import annotations

from manifold_agent.tools import tool_registry


@tool_registry
def execute(command: str) -> str:
    """Execute a shell command or code snippet (sandboxed stub)."""
    return f"[execute] refused to run unsandboxed: {command[:80]}"
