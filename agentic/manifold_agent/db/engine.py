"""Engine factory — env-driven Postgres connection."""

from __future__ import annotations

from sqlalchemy import create_engine
from sqlalchemy.engine import Engine

from manifold_agent.config import get_settings


def create_engine_from_env(url: str | None = None, *, echo: bool = False) -> Engine:
    """Build a SQLAlchemy engine from ``MP_DATABASE_URL`` (or an override)."""
    database_url = url or get_settings().database_url
    return create_engine(database_url, echo=echo, pool_pre_ping=True, future=True)
