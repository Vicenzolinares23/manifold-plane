"""Memory tools — remember / recall / forget (gated like any tool)."""

from __future__ import annotations

from manifold_agent.memory.longterm import LongTermMemory
from manifold_agent.state import MemoryEntry, MemoryKind
from manifold_agent.tools import tool_registry

_STORE = LongTermMemory()


@tool_registry
def remember(key: str, content: str, kind: str = "fact", importance: float = 0.5) -> str:
    """Store a long-term memory entry for this asker."""
    try:
        mk = MemoryKind(kind)
    except ValueError:
        mk = MemoryKind.FACT
    entry = MemoryEntry(key=key, kind=mk, content=content, importance=importance)
    _STORE.store("default", entry)
    return f"[remember] stored {key}"


@tool_registry
def recall(query: str, limit: int = 5) -> str:
    """Recall long-term memories matching a query."""
    hits = _STORE.recall("default", query, limit=limit)
    if not hits:
        return "[recall] no matches"
    return "\n".join(f"- {h.key}: {h.content}" for h in hits)


@tool_registry
def forget(key: str) -> str:
    """Forget a long-term memory by key."""
    ok = _STORE.forget("default", key)
    return f"[forget] {'removed' if ok else 'missing'} {key}"


def memory_store() -> LongTermMemory:
    return _STORE
