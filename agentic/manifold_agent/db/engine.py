"""Engine factory for the agentic Postgres layer."""

from __future__ import annotations

from os import getenv
from typing import Any

from sqlalchemy import Engine, create_engine

_DEFAULT_URL = "postgresql+psycopg://manifold:manifold@localhost:5432/manifold_plane"


def create_engine_from_env(url: str | None = None, **kwargs: Any) -> Engine:
    """Build a SQLAlchemy engine from ``MP_DATABASE_URL`` (or *url*).

    Defaults match ``manifold_agent.config`` / docker-compose: local Postgres
    with user/password ``manifold`` and database ``manifold_plane``.
    """
    database_url = url or getenv("MP_DATABASE_URL", _DEFAULT_URL)
    opts: dict[str, Any] = {"pool_pre_ping": True}
    opts.update(kwargs)
    return create_engine(database_url, **opts)
