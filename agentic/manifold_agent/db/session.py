"""Session factory and helpers for the agentic Postgres layer."""

from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager

from sqlalchemy import Engine
from sqlalchemy.orm import Session, sessionmaker

from manifold_agent.db.engine import create_engine_from_env

_engine: Engine | None = None

SessionLocal = sessionmaker(autoflush=False, autocommit=False, expire_on_commit=False)


def configure_session(engine: Engine | None = None) -> Engine:
    """Bind ``SessionLocal`` to *engine* (or one from env). Idempotent."""
    global _engine
    if engine is None:
        if _engine is None:
            _engine = create_engine_from_env()
        engine = _engine
    else:
        _engine = engine
    SessionLocal.configure(bind=engine)
    return engine


def get_session() -> Session:
    """Return a new session bound to the env engine (configures on first use)."""
    configure_session()
    return SessionLocal()


@contextmanager
def session_scope() -> Iterator[Session]:
    """Provide a transactional scope around a series of operations."""
    session = get_session()
    try:
        yield session
        session.commit()
    except Exception:
        session.rollback()
        raise
    finally:
        session.close()
