from manifold_agent.memory.longterm import LongTermMemory
from manifold_agent.state import MemoryEntry, MemoryKind


def test_store_recall_forget():
    mem = LongTermMemory()
    mem.store("a1", MemoryEntry(key="tz", kind=MemoryKind.FACT, content="UTC", importance=0.9))
    hits = mem.recall("a1", "timezone UTC")
    assert hits and hits[0].key == "tz"
    assert mem.forget("a1", "tz")
    assert mem.recall("a1", "UTC") == []
