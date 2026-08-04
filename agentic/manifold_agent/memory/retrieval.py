"""Keyword (+ optional embedding) retrieval over memory entries."""

from __future__ import annotations

from manifold_agent.state import MemoryEntry


def keyword_search(entries: list[MemoryEntry], query: str, *, limit: int = 5) -> list[MemoryEntry]:
    q = query.lower().strip()
    if not q:
        return sorted(entries, key=lambda e: e.importance, reverse=True)[:limit]
    tokens = [t for t in q.replace(",", " ").split() if t]
    scored: list[tuple[float, MemoryEntry]] = []
    for e in entries:
        hay = f"{e.key} {e.content} {e.kind.value}".lower()
        score = sum(1.0 for t in tokens if t in hay) + e.importance
        if score > e.importance or any(t in hay for t in tokens):
            scored.append((score, e))
    scored.sort(key=lambda x: x[0], reverse=True)
    return [e for _, e in scored[:limit]]
