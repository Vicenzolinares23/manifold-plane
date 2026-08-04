"""Calibrations, finetune, models registry, and eval_runs."""

from __future__ import annotations

import uuid

import pytest
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from manifold_agent.db.models import (
    Asker,
    Calibration,
    EvalRun,
    FinetuneDataset,
    FinetuneSample,
    ModelRecord,
    SessionRow,
    ToolEvent,
)

pytestmark = pytest.mark.db


def test_calibration_round_trip(db: Session) -> None:
    metric = [float(i % 6) for i in range(36)]
    cal = Calibration(
        metric=metric,
        budget=100.0,
        alpha=0.05,
        review_band=2.0,
        projection_distance=0.1,
        quantile=0.95,
        sample_count=42,
    )
    db.add(cal)
    db.flush()
    loaded = db.scalar(select(Calibration).where(Calibration.id == cal.id))
    assert loaded is not None
    assert len(loaded.metric) == 36
    assert loaded.sample_count == 42


def test_finetune_sample_links_tool_event(
    db: Session, asker: Asker, session_row: SessionRow
) -> None:
    event = ToolEvent(
        session_id=session_row.id,
        asker_id=asker.id,
        tool_name="read_local",
        kind="ReadLocal",
        decision="admit",
    )
    db.add(event)
    ds = FinetuneDataset(name=f"ds-{uuid.uuid4().hex[:8]}", source="seed", sample_count=1)
    db.add(ds)
    db.flush()

    sample = FinetuneSample(
        dataset_id=ds.id,
        input_text="read /etc/passwd",
        label={"kind": "ReadLocal", "decision": "admit"},
        split="train",
        weight=1.0,
        source_event_id=event.id,
    )
    db.add(sample)
    db.flush()

    loaded = db.scalar(select(FinetuneSample).where(FinetuneSample.id == sample.id))
    assert loaded is not None
    assert loaded.split == "train"
    assert loaded.source_event_id == event.id
    assert loaded.dataset.name == ds.name


def test_finetune_split_check(db: Session) -> None:
    ds = FinetuneDataset(name=f"ds-{uuid.uuid4().hex[:8]}")
    db.add(ds)
    db.flush()
    db.add(
        FinetuneSample(
            dataset_id=ds.id,
            input_text="x",
            label={},
            split="holdout",
        )
    )
    with pytest.raises(IntegrityError):
        db.flush()


def test_model_and_eval_run(db: Session) -> None:
    ds = FinetuneDataset(name=f"ds-{uuid.uuid4().hex[:8]}")
    model = ModelRecord(
        name="classifier-v1",
        family="qwen",
        base_model="Qwen/Qwen2.5-0.5B-Instruct",
        path="models/classifier",
        params=500_000_000,
        metrics={"loss": 0.12},
    )
    db.add_all([ds, model])
    db.flush()

    run = EvalRun(
        model_id=model.id,
        dataset_id=ds.id,
        metrics={"accuracy": 0.91, "f1": 0.88},
    )
    db.add(run)
    db.flush()

    loaded = db.scalar(select(EvalRun).where(EvalRun.id == run.id))
    assert loaded is not None
    assert loaded.metrics["accuracy"] == pytest.approx(0.91)
    assert loaded.model.name == "classifier-v1"
    assert loaded.dataset.id == ds.id
