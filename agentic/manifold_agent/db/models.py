"""SQLAlchemy 2.0 models for the Stage 8 Postgres schema (§8.5)."""

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
    Text,
    UniqueConstraint,
    func,
    text,
)
from sqlalchemy.dialects.postgresql import ARRAY, JSONB, UUID
from sqlalchemy.orm import Mapped, mapped_column, relationship

from manifold_agent.db.base import Base

_TOOL_KINDS = (
    "ReadLocal",
    "ReadExternal",
    "WriteLocal",
    "SendExternal",
    "Execute",
    "SelfModify",
    "Delegate",
)
_DECISIONS = ("admit", "hold", "deny")
_MESSAGE_ROLES = ("user", "assistant", "system", "tool")
_MEMORY_KINDS = ("fact", "work", "preference", "episode")
_GUARDRAIL_STAGES = ("input", "classify", "output", "engine")
_FINETUNE_SPLITS = ("train", "val", "test")
_EMBEDDING_DIM = 384


def _uuid_pk() -> Mapped[uuid.UUID]:
    return mapped_column(UUID(as_uuid=True), primary_key=True, default=uuid.uuid4)


def _created_at() -> Mapped[datetime]:
    return mapped_column(
        DateTime(timezone=True),
        nullable=False,
        server_default=func.now(),
    )


def _enum_in(column: str, values: tuple[str, ...]) -> str:
    quoted = ", ".join(f"'{v}'" for v in values)
    return f"{column} IN ({quoted})"


class Asker(Base):
    """Agent identity plus carried engine state."""

    __tablename__ = "askers"
    __table_args__ = (
        CheckConstraint("cardinality(z) = 6", name="ck_askers_z_len"),
        CheckConstraint("cardinality(baseline) = 6", name="ck_askers_baseline_len"),
        CheckConstraint("admitted >= 0 AND denied >= 0 AND held >= 0", name="ck_askers_counts"),
    )

    id: Mapped[uuid.UUID] = _uuid_pk()
    asker_id: Mapped[str] = mapped_column(Text, nullable=False, unique=True)
    symmetry_class: Mapped[str] = mapped_column(Text, nullable=False, server_default=text("'default'"))
    z: Mapped[list[Any]] = mapped_column(
        ARRAY(Numeric),
        nullable=False,
        server_default=text("ARRAY[0,0,0,0,0,0]::numeric[]"),
    )
    last_seen: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)
    admitted: Mapped[int] = mapped_column(Integer, nullable=False, server_default=text("0"))
    denied: Mapped[int] = mapped_column(Integer, nullable=False, server_default=text("0"))
    held: Mapped[int] = mapped_column(Integer, nullable=False, server_default=text("0"))
    baseline: Mapped[list[Any]] = mapped_column(
        ARRAY(Numeric),
        nullable=False,
        server_default=text("ARRAY[0,0,0,0,0,0]::numeric[]"),
    )
    created_at: Mapped[datetime] = _created_at()

    sessions: Mapped[list[SessionRow]] = relationship(back_populates="asker")
    tool_events: Mapped[list[ToolEvent]] = relationship(back_populates="asker")
    memory_entries: Mapped[list[MemoryEntryRow]] = relationship(back_populates="asker")
    guardrail_events: Mapped[list[GuardrailEvent]] = relationship(back_populates="asker")


class SessionRow(Base):
    """LangGraph thread / conversation session."""

    __tablename__ = "sessions"
    __table_args__ = (UniqueConstraint("thread_id", name="uq_sessions_thread_id"),)

    id: Mapped[uuid.UUID] = _uuid_pk()
    asker_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("askers.id", ondelete="CASCADE"),
        nullable=False,
    )
    thread_id: Mapped[str] = mapped_column(Text, nullable=False)
    started_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True),
        nullable=False,
        server_default=func.now(),
    )
    ended_at: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)

    asker: Mapped[Asker] = relationship(back_populates="sessions")
    messages: Mapped[list[Message]] = relationship(back_populates="session")
    tool_events: Mapped[list[ToolEvent]] = relationship(back_populates="session")
    memory_entries: Mapped[list[MemoryEntryRow]] = relationship(back_populates="session")
    guardrail_events: Mapped[list[GuardrailEvent]] = relationship(back_populates="session")


class Message(Base):
    """Transcript row for every chat role."""

    __tablename__ = "messages"
    __table_args__ = (
        CheckConstraint(_enum_in("role", _MESSAGE_ROLES), name="ck_messages_role"),
        Index("ix_messages_session_created", "session_id", "created_at"),
    )

    id: Mapped[uuid.UUID] = _uuid_pk()
    session_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("sessions.id", ondelete="CASCADE"),
        nullable=False,
    )
    role: Mapped[str] = mapped_column(Text, nullable=False)
    content: Mapped[str] = mapped_column(Text, nullable=False)
    message_id: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = _created_at()

    session: Mapped[SessionRow] = relationship(back_populates="messages")


class ToolEvent(Base):
    """Every gated tool call plus the engine decision and margins."""

    __tablename__ = "tool_events"
    __table_args__ = (
        CheckConstraint(_enum_in("kind", _TOOL_KINDS), name="ck_tool_events_kind"),
        CheckConstraint(_enum_in("decision", _DECISIONS), name="ck_tool_events_decision"),
        CheckConstraint(
            "(z_before IS NULL OR cardinality(z_before) = 6)",
            name="ck_tool_events_z_before_len",
        ),
        CheckConstraint(
            "(z_after IS NULL OR cardinality(z_after) = 6)",
            name="ck_tool_events_z_after_len",
        ),
        Index("ix_tool_events_session_created", "session_id", "created_at"),
    )

    id: Mapped[uuid.UUID] = _uuid_pk()
    session_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("sessions.id", ondelete="CASCADE"),
        nullable=False,
    )
    asker_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("askers.id", ondelete="CASCADE"),
        nullable=False,
    )
    tool_name: Mapped[str] = mapped_column(Text, nullable=False)
    kind: Mapped[str] = mapped_column(Text, nullable=False)
    arguments: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False, server_default=text("'{}'::jsonb"))
    payload_bytes: Mapped[int] = mapped_column(Integer, nullable=False, server_default=text("0"))
    recipients: Mapped[int] = mapped_column(Integer, nullable=False, server_default=text("0"))
    argument_tainted: Mapped[bool] = mapped_column(Boolean, nullable=False, server_default=text("false"))
    off_transcript: Mapped[bool] = mapped_column(Boolean, nullable=False, server_default=text("false"))
    source_sensitivity: Mapped[float] = mapped_column(Float, nullable=False, server_default=text("0.01"))
    decision: Mapped[str] = mapped_column(Text, nullable=False)
    margin_before: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    margin_after: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    required: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    alpha_effective: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    orbit_residual: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    budget_fraction: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    admissible_fraction: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    blocked_by_coalition: Mapped[Optional[int]] = mapped_column(Integer, nullable=True)
    z_before: Mapped[Optional[list[Any]]] = mapped_column(ARRAY(Numeric), nullable=True)
    z_after: Mapped[Optional[list[Any]]] = mapped_column(ARRAY(Numeric), nullable=True)
    result: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = _created_at()

    session: Mapped[SessionRow] = relationship(back_populates="tool_events")
    asker: Mapped[Asker] = relationship(back_populates="tool_events")


class MemoryEntryRow(Base):
    """Long-term memory row; embedding stored as FLOAT8[384] (pgvector optional).

    The compose image is ``postgres:16-alpine`` without pgvector. Prefer
    ``vector(384)`` when the extension is available; otherwise FLOAT8[] of
    length 384 (see ``agentic/sql/schema.sql``).
    """

    __tablename__ = "memory_entries"
    __table_args__ = (
        CheckConstraint(_enum_in("kind", _MEMORY_KINDS), name="ck_memory_entries_kind"),
        CheckConstraint(
            f"(embedding IS NULL OR cardinality(embedding) = {_EMBEDDING_DIM})",
            name="ck_memory_entries_embedding_len",
        ),
        Index("ix_memory_entries_asker_kind", "asker_id", "kind"),
    )

    id: Mapped[uuid.UUID] = _uuid_pk()
    asker_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("askers.id", ondelete="CASCADE"),
        nullable=False,
    )
    session_id: Mapped[Optional[uuid.UUID]] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("sessions.id", ondelete="SET NULL"),
        nullable=True,
    )
    kind: Mapped[str] = mapped_column(Text, nullable=False)
    content: Mapped[str] = mapped_column(Text, nullable=False)
    importance: Mapped[float] = mapped_column(Float, nullable=False, server_default=text("0.5"))
    scope: Mapped[str] = mapped_column(Text, nullable=False, server_default=text("'global'"))
    ttl_secs: Mapped[Optional[float]] = mapped_column(Float, nullable=True)
    expires_at: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)
    # FLOAT8[] length 384 — documented fallback when pgvector is unavailable.
    embedding: Mapped[Optional[list[float]]] = mapped_column(ARRAY(Float), nullable=True)
    metadata_: Mapped[dict[str, Any]] = mapped_column(
        "metadata",
        JSONB,
        nullable=False,
        server_default=text("'{}'::jsonb"),
    )
    created_at: Mapped[datetime] = _created_at()
    last_accessed_at: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)

    asker: Mapped[Asker] = relationship(back_populates="memory_entries")
    session: Mapped[Optional[SessionRow]] = relationship(back_populates="memory_entries")


class GuardrailEvent(Base):
    """Every guardrail decision (input / classify / output / engine)."""

    __tablename__ = "guardrail_events"
    __table_args__ = (
        CheckConstraint(_enum_in("stage", _GUARDRAIL_STAGES), name="ck_guardrail_events_stage"),
        Index("ix_guardrail_events_session_created", "session_id", "created_at"),
    )

    id: Mapped[uuid.UUID] = _uuid_pk()
    session_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("sessions.id", ondelete="CASCADE"),
        nullable=False,
    )
    asker_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("askers.id", ondelete="CASCADE"),
        nullable=False,
    )
    stage: Mapped[str] = mapped_column(Text, nullable=False)
    allowed: Mapped[bool] = mapped_column(Boolean, nullable=False)
    risk: Mapped[float] = mapped_column(Float, nullable=False, server_default=text("0"))
    reason: Mapped[str] = mapped_column(Text, nullable=False, server_default=text("'ok'"))
    model: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    details: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False, server_default=text("'{}'::jsonb"))
    created_at: Mapped[datetime] = _created_at()

    session: Mapped[SessionRow] = relationship(back_populates="guardrail_events")
    asker: Mapped[Asker] = relationship(back_populates="guardrail_events")


class Calibration(Base):
    """Engine fit snapshot (metric tensor + budget / alpha knobs)."""

    __tablename__ = "calibrations"
    __table_args__ = (
        CheckConstraint("cardinality(metric) = 36", name="ck_calibrations_metric_len"),
    )

    id: Mapped[uuid.UUID] = _uuid_pk()
    # Flattened 6×6 metric (row-major). Spec lists numeric[6][6]; FLOAT8[36]
    # is portable and easy to round-trip from SQLAlchemy ARRAY.
    metric: Mapped[list[Any]] = mapped_column(ARRAY(Numeric), nullable=False)
    budget: Mapped[float] = mapped_column(Float, nullable=False)
    alpha: Mapped[float] = mapped_column(Float, nullable=False)
    review_band: Mapped[float] = mapped_column(Float, nullable=False)
    projection_distance: Mapped[float] = mapped_column(Float, nullable=False)
    quantile: Mapped[float] = mapped_column(Float, nullable=False)
    sample_count: Mapped[int] = mapped_column(Integer, nullable=False, server_default=text("0"))
    created_at: Mapped[datetime] = _created_at()


class FinetuneDataset(Base):
    """Named training set for the measurement-layer classifier."""

    __tablename__ = "finetune_datasets"
    __table_args__ = (UniqueConstraint("name", name="uq_finetune_datasets_name"),)

    id: Mapped[uuid.UUID] = _uuid_pk()
    name: Mapped[str] = mapped_column(Text, nullable=False)
    source: Mapped[str] = mapped_column(Text, nullable=False, server_default=text("''"))
    sample_count: Mapped[int] = mapped_column(Integer, nullable=False, server_default=text("0"))
    created_at: Mapped[datetime] = _created_at()

    samples: Mapped[list[FinetuneSample]] = relationship(back_populates="dataset")
    eval_runs: Mapped[list[EvalRun]] = relationship(back_populates="dataset")


class FinetuneSample(Base):
    """Labeled training row, optionally linked back to a tool event."""

    __tablename__ = "finetune_samples"
    __table_args__ = (
        CheckConstraint(_enum_in("split", _FINETUNE_SPLITS), name="ck_finetune_samples_split"),
        Index("ix_finetune_samples_dataset_split", "dataset_id", "split"),
    )

    id: Mapped[uuid.UUID] = _uuid_pk()
    dataset_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("finetune_datasets.id", ondelete="CASCADE"),
        nullable=False,
    )
    input_text: Mapped[str] = mapped_column(Text, nullable=False)
    label: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False)
    split: Mapped[str] = mapped_column(Text, nullable=False)
    weight: Mapped[float] = mapped_column(Float, nullable=False, server_default=text("1.0"))
    source_event_id: Mapped[Optional[uuid.UUID]] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("tool_events.id", ondelete="SET NULL"),
        nullable=True,
    )
    created_at: Mapped[datetime] = _created_at()

    dataset: Mapped[FinetuneDataset] = relationship(back_populates="samples")
    source_event: Mapped[Optional[ToolEvent]] = relationship()


class ModelRecord(Base):
    """Registry entry for a trained or base model artifact."""

    __tablename__ = "models"

    id: Mapped[uuid.UUID] = _uuid_pk()
    name: Mapped[str] = mapped_column(Text, nullable=False)
    family: Mapped[str] = mapped_column(Text, nullable=False, server_default=text("''"))
    base_model: Mapped[str] = mapped_column(Text, nullable=False, server_default=text("''"))
    path: Mapped[str] = mapped_column(Text, nullable=False, server_default=text("''"))
    params: Mapped[Optional[int]] = mapped_column(Integer, nullable=True)
    metrics: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False, server_default=text("'{}'::jsonb"))
    created_at: Mapped[datetime] = _created_at()

    eval_runs: Mapped[list[EvalRun]] = relationship(back_populates="model")


class EvalRun(Base):
    """Held-out eval results for a model against a finetune dataset."""

    __tablename__ = "eval_runs"

    id: Mapped[uuid.UUID] = _uuid_pk()
    model_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("models.id", ondelete="CASCADE"),
        nullable=False,
    )
    dataset_id: Mapped[uuid.UUID] = mapped_column(
        UUID(as_uuid=True),
        ForeignKey("finetune_datasets.id", ondelete="CASCADE"),
        nullable=False,
    )
    metrics: Mapped[dict[str, Any]] = mapped_column(JSONB, nullable=False, server_default=text("'{}'::jsonb"))
    created_at: Mapped[datetime] = _created_at()

    model: Mapped[ModelRecord] = relationship(back_populates="eval_runs")
    dataset: Mapped[FinetuneDataset] = relationship(back_populates="eval_runs")
