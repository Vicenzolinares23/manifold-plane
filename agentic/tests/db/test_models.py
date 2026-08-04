"""DB model smoke tests (no live Postgres required)."""

from manifold_agent.db.models import Asker, Base, FinetuneDataset, MemoryEntryRow, ToolEvent


def test_models_are_mapped():
    tables = set(Base.metadata.tables)
    for name in (
        "askers",
        "sessions",
        "messages",
        "tool_events",
        "memory_entries",
        "guardrail_events",
        "calibrations",
        "finetune_datasets",
        "finetune_samples",
        "models",
        "eval_runs",
    ):
        assert name in tables


def test_asker_defaults():
    a = Asker(asker_id="x")
    assert a.asker_id == "x"
