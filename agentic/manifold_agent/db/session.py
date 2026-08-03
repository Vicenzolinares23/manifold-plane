"""Session factory for the agentic Postgres store."""

from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager

from sqlalchemy.orm import Session, sessionmaker

from manifold_agent.db.engine import create_engine_from_env

_session_factory: sessionmaker[Session] | None = None


def configure_session(url: str | None = None) -> sessionmaker[Session]:
    """Bind a process-wide session factory to an engine."""
    global _session_factory
    eng = create_engine_from_env(url)
    _session_factory = sessionmaker(bind=eng, autoflush=False, autocommit=False, expire_on_commit=False, future=True)
    return _session_factory


def SessionLocal() -> Session:
    """Create a new Session (configures from env on first use)."""
    global _session_factory
    if _session_factory is None:
        configure_session()
    assert _session_factory is not None
    return _session_factory()


@contextmanager
def get_session() -> Iterator[Session]:
    """Yield a short-lived session; commits on success, rolls back on error."""
    session = SessionLocal()
    try:
        yield session
        session.commit()
    except Exception:
        session.rollback()
        raise
    finally:
        session.close()
