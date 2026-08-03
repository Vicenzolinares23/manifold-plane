"""Long-term memory store with in-memory backend (Postgres-ready)."""

from __future__ import annotations

import time
from dataclasses import dataclass, field

from manifold_agent.memory.retrieval import keyword_search
from manifold_agent.state import MemoryEntry


@dataclass
class LongTermMemory:
    """Asker-scoped memory. Falls back to dict when SQLAlchemy is unavailable."""

    _entries: dict[str, dict[str, MemoryEntry]] = field(default_factory=dict)
    _accessed: dict[str, dict[str, float]] = field(default_factory=dict)

    def store(self, asker_id: str, entry: MemoryEntry) -> MemoryEntry:
        bucket = self._entries.setdefault(asker_id, {})
        bucket[entry.key] = entry
        self._accessed.setdefault(asker_id, {})[entry.key] = time.time()
        return entry

    def recall(self, asker_id: str, query: str, *, limit: int = 5) -> list[MemoryEntry]:
        bucket = self._entries.get(asker_id, {})
        hits = keyword_search(list(bucket.values()), query, limit=limit)
        now = time.time()
        for h in hits:
            self._accessed.setdefault(asker_id, {})[h.key] = now
        return hits

    def forget(self, asker_id: str, key: str) -> bool:
        bucket = self._entries.get(asker_id, {})
        if key in bucket:
            del bucket[key]
            self._accessed.get(asker_id, {}).pop(key, None)
            return True
        return False

    def fade(self, asker_id: str, *, max_age_secs: float, now: float | None = None) -> int:
        """Drop entries whose last access is older than max_age_secs. Returns count removed."""
        now = now if now is not None else time.time()
        accessed = self._accessed.get(asker_id, {})
        bucket = self._entries.get(asker_id, {})
        stale = [k for k, t in accessed.items() if now - t > max_age_secs]
        for k in stale:
            bucket.pop(k, None)
            accessed.pop(k, None)
        return len(stale)
