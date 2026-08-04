"""Asker / session model round-trips."""

from __future__ import annotations

import uuid

import pytest
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from manifold_agent.db.models import Asker, Message, SessionRow

pytestmark = pytest.mark.db


def test_asker_session_round_trip(db: Session) -> None:
    asker = Asker(
        asker_id=f"rt-{uuid.uuid4().hex[:8]}",
        symmetry_class="default",
        z=[0.1, 0.2, 0.3, 0.4, 0.5, 0.6],
        baseline=[0, 0, 0, 0, 0, 0],
        admitted=1,
        denied=0,
        held=0,
    )
    db.add(asker)
    db.flush()

    session = SessionRow(asker_id=asker.id, thread_id=f"t-{uuid.uuid4().hex[:8]}")
    db.add(session)
    db.flush()

    loaded = db.scalar(select(Asker).where(Asker.id == asker.id))
    assert loaded is not None
    assert loaded.asker_id == asker.asker_id
    assert len(loaded.z) == 6
    assert float(loaded.z[0]) == pytest.approx(0.1)

    sess = db.scalar(select(SessionRow).where(SessionRow.id == session.id))
    assert sess is not None
    assert sess.asker_id == asker.id
    assert sess.asker.asker_id == asker.asker_id


def test_asker_id_unique(db: Session) -> None:
    key = f"uniq-{uuid.uuid4().hex[:8]}"
    db.add(Asker(asker_id=key))
    db.flush()
    db.add(Asker(asker_id=key))
    with pytest.raises(IntegrityError):
        db.flush()


def test_message_role_check(db: Session, session_row: SessionRow) -> None:
    db.add(Message(session_id=session_row.id, role="user", content="hi", message_id="m1"))
    db.flush()
    bad = Message(session_id=session_row.id, role="narrator", content="nope")
    db.add(bad)
    with pytest.raises(IntegrityError):
        db.flush()
