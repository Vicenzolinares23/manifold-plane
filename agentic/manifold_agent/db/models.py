"""SQLAlchemy 2.0 models for Stage 8 Postgres schema (`docs/08` §8.5)."""

from __future__ import annotations

import uuid
from datetime import datetime
from typing import Any, Optional

from sqlalchemy import (
    Boolean,
    CheckConstraint,
    DateTime,
    Float,
    ForeignKey,
    Index,
    Integer,
    Numeric,
    String,
    Text,
    UniqueConstraint,
    func,
    text,
)
from sqlalchemy.dialects.postgresql import ARRAY, JSONB, UUID
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship


class Base(DeclarativeBase):
    pass


def _uuid() -> uuid.UUID:
    return uuid.uuid4()


class Asker(Base):
    __tablename__ = "askers"
    __table_args__ = (UniqueConstraint("asker_id", name="uq_askers_asker_id"),)

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    asker_id: Mapped[str] = mapped_column(Text, nullable=False)
    symmetry_class: Mapped[str] = mapped_column(Text, nullable=False, default="default")
    z: Mapped[list[float]] = mapped_column(ARRAY(Numeric), nullable=False, server_default=text("ARRAY[0,0,0,0,0,0]::numeric[]"))
    last_seen: Mapped[float] = mapped_column(Float, nullable=False, default=0.0)
    admitted: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    denied: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    held: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    baseline: Mapped[list[float]] = mapped_column(
        ARRAY(Numeric), nullable=False, server_default=text("ARRAY[0,0,0,0,0,0]::numeric[]")
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())

    sessions: Mapped[list[SessionRow]] = relationship(back_populates="asker")


class SessionRow(Base):
    __tablename__ = "sessions"
    __table_args__ = (UniqueConstraint("thread_id", name="uq_sessions_thread_id"),)

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    asker_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("askers.id", ondelete="CASCADE"), nullable=False)
    thread_id: Mapped[str] = mapped_column(Text, nullable=False)
    started_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    ended_at: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)

    asker: Mapped[Asker] = relationship(back_populates="sessions")
    messages: Mapped[list[Message]] = relationship(back_populates="session")


class Message(Base):
    __tablename__ = "messages"
    __table_args__ = (
        CheckConstraint(
            "role IN ('system','user','assistant','tool')",
            name="ck_messages_role",
        ),
    )

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    session_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("sessions.id", ondelete="CASCADE"), nullable=False)
    role: Mapped[str] = mapped_column(Text, nullable=False)
    content: Mapped[str] = mapped_column(Text, nullable=False)
    message_id: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())

    session: Mapped[SessionRow] = relationship(back_populates="messages")


class ToolEvent(Base):
    __tablename__ = "tool_events"
    __table_args__ = (
        CheckConstraint(
            "decision IN ('admit','hold','deny')",
            name="ck_tool_events_decision",
        ),
        Index("ix_tool_events_session_created", "session_id", "created_at"),
    )

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    session_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("sessions.id", ondelete="CASCADE"), nullable=False)
    asker_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("askers.id", ondelete="CASCADE"), nullable=False)
    tool_name: Mapped[str] = mapped_column(Text, nullable=False)
    kind: Mapped[str] = mapped_column(Text, nullable=False)
    arguments: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False, server_default=text("'{}'::jsonb"))
    payload_bytes: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    recipients: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    argument_tainted: Mapped[bool] = mapped_column(Boolean, nullable=False, default=False)
    off_transcript: Mapped[bool] = mapped_column(Boolean, nullable=False, default=False)
    source_sensitivity: Mapped[float] = mapped_column(Float, nullable=False, default=0.01)
    decision: Mapped[str] = mapped_column(Text, nullable=False)
    margin_before: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    margin_after: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    required: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    alpha_effective: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    orbit_residual: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    budget_fraction: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    admissible_fraction: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    blocked_by_coalition: Mapped[Optional[int]] = mapped_column(Integer, nullable=True)
    z_before: Mapped[Optional[list[float]]] = mapped_column(ARRAY(Numeric), nullable=True)
    z_after: Mapped[Optional[list[float]]] = mapped_column(ARRAY(Numeric), nullable=True)
    result: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())


class MemoryEntryRow(Base):
    __tablename__ = "memory_entries"
    __table_args__ = (
        CheckConstraint(
            "kind IN ('fact','work','preference','episode')",
            name="ck_memory_kind",
        ),
        Index("ix_memory_asker_kind", "asker_id", "kind"),
    )

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    asker_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("askers.id", ondelete="CASCADE"), nullable=False)
    session_id: Mapped[Optional[uuid.UUID]] = mapped_column(
        ForeignKey("sessions.id", ondelete="SET NULL"), nullable=True
    )
    kind: Mapped[str] = mapped_column(Text, nullable=False)
    content: Mapped[str] = mapped_column(Text, nullable=False)
    importance: Mapped[float] = mapped_column(Float, nullable=False, default=0.5)
    scope: Mapped[str] = mapped_column(Text, nullable=False, default="global")
    ttl_secs: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    expires_at: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)
    # FLOAT8[384] — portable without requiring the pgvector extension at bootstrap.
    embedding: Mapped[Optional[list[float]]] = mapped_column(ARRAY(Float), nullable=True)
    metadata_: Mapped[dict[str, Any]] = mapped_column(
        "metadata", JSONB, nullable=False, server_default=text("'{}'::jsonb")
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    last_accessed_at: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)


class GuardrailEvent(Base):
    __tablename__ = "guardrail_events"
    __table_args__ = (
        CheckConstraint(
            "stage IN ('input','classify','output','engine')",
            name="ck_guardrail_stage",
        ),
        Index("ix_guardrail_session_created", "session_id", "created_at"),
    )

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    session_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("sessions.id", ondelete="CASCADE"), nullable=False)
    asker_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("askers.id", ondelete="CASCADE"), nullable=False)
    stage: Mapped[str] = mapped_column(Text, nullable=False)
    allowed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    risk: Mapped[float] = mapped_column(Float, nullable=False, default=0.0)
    reason: Mapped[str] = mapped_column(Text, nullable=False, default="ok")
    model: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    details: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False, server_default=text("'{}'::jsonb"))
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())


class Calibration(Base):
    __tablename__ = "calibrations"

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    metric: Mapped[list[list[float]]] = mapped_column(ARRAY(Numeric, dimensions=2), nullable=False)
    budget: Mapped[float] = mapped_column(Float, nullable=False)
    alpha: Mapped[float] = mapped_column(Float, nullable=False)
    review_band: Mapped[float] = mapped_column(Float, nullable=False)
    projection_distance: Mapped[float] = mapped_column(Float, nullable=False, default=0.0)
    quantile: Mapped[float] = mapped_column(Float, nullable=False, default=0.999)
    sample_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())


class FinetuneDataset(Base):
    __tablename__ = "finetune_datasets"
    __table_args__ = (UniqueConstraint("name", name="uq_finetune_datasets_name"),)

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    name: Mapped[str] = mapped_column(Text, nullable=False)
    source: Mapped[str] = mapped_column(Text, nullable=False, default="seed")
    sample_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())

    samples: Mapped[list[FinetuneSample]] = relationship(back_populates="dataset")


class FinetuneSample(Base):
    __tablename__ = "finetune_samples"
    __table_args__ = (
        CheckConstraint("split IN ('train','val','test')", name="ck_finetune_split"),
        Index("ix_finetune_samples_dataset_split", "dataset_id", "split"),
    )

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    dataset_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("finetune_datasets.id", ondelete="CASCADE"), nullable=False
    )
    input_text: Mapped[str] = mapped_column(Text, nullable=False)
    label: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    split: Mapped[str] = mapped_column(Text, nullable=False, default="train")
    weight: Mapped[float] = mapped_column(Float, nullable=False, default=1.0)
    source_event_id: Mapped[Optional[uuid.UUID]] = mapped_column(
        ForeignKey("tool_events.id", ondelete="SET NULL"), nullable=True
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())

    dataset: Mapped[FinetuneDataset] = relationship(back_populates="samples")


class ModelRow(Base):
    __tablename__ = "models"

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    name: Mapped[str] = mapped_column(Text, nullable=False)
    family: Mapped[str] = mapped_column(Text, nullable=False, default="classifier")
    base_model: Mapped[str] = mapped_column(Text, nullable=False)
    path: Mapped[str] = mapped_column(Text, nullable=False)
    params: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False, server_default=text("'{}'::jsonb"))
    metrics: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False, server_default=text("'{}'::jsonb"))
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())


class EvalRun(Base):
    __tablename__ = "eval_runs"

    id: Mapped[uuid.UUID] = mapped_column(UUID(as_uuid=True), primary_key=True, default=_uuid)
    model_id: Mapped[uuid.UUID] = mapped_column(ForeignKey("models.id", ondelete="CASCADE"), nullable=False)
    dataset_id: Mapped[uuid.UUID] = mapped_column(
        ForeignKey("finetune_datasets.id", ondelete="CASCADE"), nullable=False
    )
    metrics: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False, server_default=text("'{}'::jsonb"))
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
