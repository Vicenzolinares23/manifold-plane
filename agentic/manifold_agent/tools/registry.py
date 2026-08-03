"""Tool registry — every tool is gated by the engine before execution."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from langchain_core.tools import BaseTool, StructuredTool

ALL_TOOLS: list[BaseTool] = []
_BY_NAME: dict[str, BaseTool] = {}


def tool_registry(fn: Callable[..., Any] | None = None, *, name: str | None = None):
    """Decorator that registers a callable as a LangChain tool."""

    def _wrap(f: Callable[..., Any]) -> BaseTool:
        tool_name = name or f.__name__
        tool = StructuredTool.from_function(func=f, name=tool_name, description=f.__doc__ or tool_name)
        ALL_TOOLS.append(tool)
        _BY_NAME[tool_name] = tool
        return tool

    if fn is not None:
        return _wrap(fn)
    return _wrap


def get_tool(name: str) -> BaseTool | None:
    return _BY_NAME.get(name)


def tool_names() -> list[str]:
    return list(_BY_NAME.keys())


def load_all_tools() -> list[BaseTool]:
    """Import tool modules so the registry is populated."""
    from manifold_agent.tools import (  # noqa: F401
        delegate,
        execute,
        memory_tools,
        read_external,
        read_local,
        self_modify,
        send_external,
        write_local,
    )

    return list(ALL_TOOLS)
