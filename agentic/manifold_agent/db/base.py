"""SQLAlchemy declarative base for the agentic Postgres layer."""

from __future__ import annotations

from sqlalchemy.orm import DeclarativeBase


class Base(DeclarativeBase):
    """Shared declarative base for all ``manifold_agent.db`` models."""
