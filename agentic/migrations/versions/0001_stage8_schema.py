"""Initial Stage 8 schema — all agentic tables."""

from __future__ import annotations

from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects import postgresql

revision: str = "0001_stage8_schema"
down_revision: Union[str, None] = None
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.execute('CREATE EXTENSION IF NOT EXISTS "uuid-ossp"')

    op.create_table(
        "askers",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("asker_id", sa.Text(), nullable=False),
        sa.Column("symmetry_class", sa.Text(), nullable=False, server_default="default"),
        sa.Column("z", postgresql.ARRAY(sa.Numeric()), nullable=False),
        sa.Column("last_seen", sa.Float(), nullable=False, server_default="0"),
        sa.Column("admitted", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("denied", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("held", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("baseline", postgresql.ARRAY(sa.Numeric()), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
        sa.UniqueConstraint("asker_id", name="uq_askers_asker_id"),
    )
    op.create_table(
        "sessions",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("asker_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("askers.id", ondelete="CASCADE"), nullable=False),
        sa.Column("thread_id", sa.Text(), nullable=False),
        sa.Column("started_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
        sa.Column("ended_at", sa.DateTime(timezone=True), nullable=True),
        sa.UniqueConstraint("thread_id", name="uq_sessions_thread_id"),
    )
    op.create_table(
        "messages",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("session_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("sessions.id", ondelete="CASCADE"), nullable=False),
        sa.Column("role", sa.Text(), nullable=False),
        sa.Column("content", sa.Text(), nullable=False),
        sa.Column("message_id", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
        sa.CheckConstraint("role IN ('system','user','assistant','tool')", name="ck_messages_role"),
    )
    op.create_table(
        "tool_events",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("session_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("sessions.id", ondelete="CASCADE"), nullable=False),
        sa.Column("asker_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("askers.id", ondelete="CASCADE"), nullable=False),
        sa.Column("tool_name", sa.Text(), nullable=False),
        sa.Column("kind", sa.Text(), nullable=False),
        sa.Column("arguments", postgresql.JSONB(), nullable=False, server_default=sa.text("'{}'::jsonb")),
        sa.Column("payload_bytes", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("recipients", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("argument_tainted", sa.Boolean(), nullable=False, server_default=sa.text("false")),
        sa.Column("off_transcript", sa.Boolean(), nullable=False, server_default=sa.text("false")),
        sa.Column("source_sensitivity", sa.Float(), nullable=False, server_default="0.01"),
        sa.Column("decision", sa.Text(), nullable=False),
        sa.Column("margin_before", sa.Float(), nullable=True),
        sa.Column("margin_after", sa.Float(), nullable=True),
        sa.Column("required", sa.Float(), nullable=True),
        sa.Column("alpha_effective", sa.Float(), nullable=True),
        sa.Column("orbit_residual", sa.Float(), nullable=True),
        sa.Column("budget_fraction", sa.Float(), nullable=True),
        sa.Column("admissible_fraction", sa.Float(), nullable=True),
        sa.Column("blocked_by_coalition", sa.Integer(), nullable=True),
        sa.Column("z_before", postgresql.ARRAY(sa.Numeric()), nullable=True),
        sa.Column("z_after", postgresql.ARRAY(sa.Numeric()), nullable=True),
        sa.Column("result", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
        sa.CheckConstraint("decision IN ('admit','hold','deny')", name="ck_tool_events_decision"),
    )
    op.create_index("ix_tool_events_session_created", "tool_events", ["session_id", "created_at"])

    op.create_table(
        "memory_entries",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("asker_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("askers.id", ondelete="CASCADE"), nullable=False),
        sa.Column("session_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("sessions.id", ondelete="SET NULL"), nullable=True),
        sa.Column("kind", sa.Text(), nullable=False),
        sa.Column("content", sa.Text(), nullable=False),
        sa.Column("importance", sa.Float(), nullable=False, server_default="0.5"),
        sa.Column("scope", sa.Text(), nullable=False, server_default="global"),
        sa.Column("ttl_secs", sa.Float(), nullable=True),
        sa.Column("expires_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("embedding", postgresql.ARRAY(sa.Float()), nullable=True),
        sa.Column("metadata", postgresql.JSONB(), nullable=False, server_default=sa.text("'{}'::jsonb")),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
        sa.Column("last_accessed_at", sa.DateTime(timezone=True), nullable=True),
        sa.CheckConstraint("kind IN ('fact','work','preference','episode')", name="ck_memory_kind"),
    )
    op.create_index("ix_memory_asker_kind", "memory_entries", ["asker_id", "kind"])

    op.create_table(
        "guardrail_events",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("session_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("sessions.id", ondelete="CASCADE"), nullable=False),
        sa.Column("asker_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("askers.id", ondelete="CASCADE"), nullable=False),
        sa.Column("stage", sa.Text(), nullable=False),
        sa.Column("allowed", sa.Boolean(), nullable=False),
        sa.Column("risk", sa.Float(), nullable=False, server_default="0"),
        sa.Column("reason", sa.Text(), nullable=False, server_default="ok"),
        sa.Column("model", sa.Text(), nullable=True),
        sa.Column("details", postgresql.JSONB(), nullable=False, server_default=sa.text("'{}'::jsonb")),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
        sa.CheckConstraint("stage IN ('input','classify','output','engine')", name="ck_guardrail_stage"),
    )
    op.create_index("ix_guardrail_session_created", "guardrail_events", ["session_id", "created_at"])

    op.create_table(
        "calibrations",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("metric", postgresql.ARRAY(sa.Numeric(), dimensions=2), nullable=False),
        sa.Column("budget", sa.Float(), nullable=False),
        sa.Column("alpha", sa.Float(), nullable=False),
        sa.Column("review_band", sa.Float(), nullable=False),
        sa.Column("projection_distance", sa.Float(), nullable=False, server_default="0"),
        sa.Column("quantile", sa.Float(), nullable=False, server_default="0.999"),
        sa.Column("sample_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
    )
    op.create_table(
        "finetune_datasets",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("name", sa.Text(), nullable=False),
        sa.Column("source", sa.Text(), nullable=False, server_default="seed"),
        sa.Column("sample_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
        sa.UniqueConstraint("name", name="uq_finetune_datasets_name"),
    )
    op.create_table(
        "finetune_samples",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("dataset_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("finetune_datasets.id", ondelete="CASCADE"), nullable=False),
        sa.Column("input_text", sa.Text(), nullable=False),
        sa.Column("label", postgresql.JSONB(), nullable=False),
        sa.Column("split", sa.Text(), nullable=False, server_default="train"),
        sa.Column("weight", sa.Float(), nullable=False, server_default="1.0"),
        sa.Column("source_event_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("tool_events.id", ondelete="SET NULL"), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
        sa.CheckConstraint("split IN ('train','val','test')", name="ck_finetune_split"),
    )
    op.create_index("ix_finetune_samples_dataset_split", "finetune_samples", ["dataset_id", "split"])

    op.create_table(
        "models",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("name", sa.Text(), nullable=False),
        sa.Column("family", sa.Text(), nullable=False, server_default="classifier"),
        sa.Column("base_model", sa.Text(), nullable=False),
        sa.Column("path", sa.Text(), nullable=False),
        sa.Column("params", postgresql.JSONB(), nullable=False, server_default=sa.text("'{}'::jsonb")),
        sa.Column("metrics", postgresql.JSONB(), nullable=False, server_default=sa.text("'{}'::jsonb")),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
    )
    op.create_table(
        "eval_runs",
        sa.Column("id", postgresql.UUID(as_uuid=True), primary_key=True),
        sa.Column("model_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("models.id", ondelete="CASCADE"), nullable=False),
        sa.Column("dataset_id", postgresql.UUID(as_uuid=True), sa.ForeignKey("finetune_datasets.id", ondelete="CASCADE"), nullable=False),
        sa.Column("metrics", postgresql.JSONB(), nullable=False, server_default=sa.text("'{}'::jsonb")),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()")),
    )


def downgrade() -> None:
    op.drop_table("eval_runs")
    op.drop_table("models")
    op.drop_index("ix_finetune_samples_dataset_split", table_name="finetune_samples")
    op.drop_table("finetune_samples")
    op.drop_table("finetune_datasets")
    op.drop_table("calibrations")
    op.drop_index("ix_guardrail_session_created", table_name="guardrail_events")
    op.drop_table("guardrail_events")
    op.drop_index("ix_memory_asker_kind", table_name="memory_entries")
    op.drop_table("memory_entries")
    op.drop_index("ix_tool_events_session_created", table_name="tool_events")
    op.drop_table("tool_events")
    op.drop_table("messages")
    op.drop_table("sessions")
    op.drop_table("askers")
