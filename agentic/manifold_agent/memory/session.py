"""LangGraph session checkpointing."""

from __future__ import annotations

from typing import Any


def build_checkpointer(database_url: str | None = None) -> Any:
    """Prefer PostgresSaver when a DB URL is available; else MemorySaver."""
    from langgraph.checkpoint.memory import MemorySaver

    if not database_url:
        return MemorySaver()
    try:
        from langgraph.checkpoint.postgres import PostgresSaver  # type: ignore

        # Connection setup is env-specific; fall back if the optional path fails.
        return PostgresSaver.from_conn_string(database_url)  # type: ignore[attr-defined]
    except Exception:
        return MemorySaver()
