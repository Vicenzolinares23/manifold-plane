"""Shared fixtures for Postgres db tests.

Live Postgres is preferred (compose stack). Tests skip when unreachable.
"""

from __future__ import annotations

import os
import uuid

import pytest
from sqlalchemy import text
from sqlalchemy.exc import OperationalError
from sqlalchemy.orm import Session

from manifold_agent.db.base import Base
from manifold_agent.db.engine import create_engine_from_env
from manifold_agent.db.models import Asker, SessionRow
from manifold_agent.db.session import SessionLocal, configure_session

import manifold_agent.db.models  # noqa: F401 — register metadata

pytestmark = pytest.mark.db


def _database_url() -> str:
    return os.getenv(
        "MP_DATABASE_URL",
        "postgresql+psycopg://manifold:manifold@localhost:5432/manifold_plane",
    )


def pytest_configure(config: pytest.Config) -> None:
    config.addinivalue_line("markers", "db: tests that require a live Postgres")


@pytest.fixture(scope="session")
def engine():
    eng = create_engine_from_env(_database_url())
    try:
        with eng.connect() as conn:
            conn.execute(text("SELECT 1"))
    except OperationalError as exc:
        pytest.skip(f"Postgres unavailable at {_database_url()}: {exc}")
    configure_session(eng)
    # Prefer Alembic-managed schema; create_all is a no-op when tables exist.
    Base.metadata.create_all(bind=eng)
    yield eng
    eng.dispose()


@pytest.fixture
def db(engine) -> Session:
    """Per-test session wrapped in a rolled-back outer transaction."""
    connection = engine.connect()
    transaction = connection.begin()
    session = SessionLocal(bind=connection)
    nested = connection.begin_nested()

    yield session

    session.close()
    if nested.is_active:
        nested.rollback()
    if transaction.is_active:
        transaction.rollback()
    connection.close()


@pytest.fixture
def asker(db: Session) -> Asker:
    row = Asker(asker_id=f"asker-{uuid.uuid4().hex[:8]}", symmetry_class="default")
    db.add(row)
    db.flush()
    return row


@pytest.fixture
def session_row(db: Session, asker: Asker) -> SessionRow:
    row = SessionRow(asker_id=asker.id, thread_id=f"thread-{uuid.uuid4().hex[:8]}")
    db.add(row)
    db.flush()
    return row
