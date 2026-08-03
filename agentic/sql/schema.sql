-- manifold-plane Stage 8 schema (`docs/08` §8.5)
-- Bootstrap / reference DDL. Alembic is the migration authority.

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE IF NOT EXISTS askers (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    asker_id TEXT NOT NULL,
    symmetry_class TEXT NOT NULL DEFAULT 'default',
    z NUMERIC[] NOT NULL DEFAULT ARRAY[0,0,0,0,0,0]::numeric[],
    last_seen DOUBLE PRECISION NOT NULL DEFAULT 0,
    admitted INTEGER NOT NULL DEFAULT 0,
    denied INTEGER NOT NULL DEFAULT 0,
    held INTEGER NOT NULL DEFAULT 0,
    baseline NUMERIC[] NOT NULL DEFAULT ARRAY[0,0,0,0,0,0]::numeric[],
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_askers_asker_id UNIQUE (asker_id)
);

CREATE TABLE IF NOT EXISTS sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    asker_id UUID NOT NULL REFERENCES askers(id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at TIMESTAMPTZ,
    CONSTRAINT uq_sessions_thread_id UNIQUE (thread_id)
);

CREATE TABLE IF NOT EXISTS messages (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    message_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_messages_role CHECK (role IN ('system','user','assistant','tool'))
);

CREATE TABLE IF NOT EXISTS tool_events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    asker_id UUID NOT NULL REFERENCES askers(id) ON DELETE CASCADE,
    tool_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    arguments JSONB NOT NULL DEFAULT '{}'::jsonb,
    payload_bytes INTEGER NOT NULL DEFAULT 0,
    recipients INTEGER NOT NULL DEFAULT 0,
    argument_tainted BOOLEAN NOT NULL DEFAULT false,
    off_transcript BOOLEAN NOT NULL DEFAULT false,
    source_sensitivity DOUBLE PRECISION NOT NULL DEFAULT 0.01,
    decision TEXT NOT NULL,
    margin_before DOUBLE PRECISION,
    margin_after DOUBLE PRECISION,
    required DOUBLE PRECISION,
    alpha_effective DOUBLE PRECISION,
    orbit_residual DOUBLE PRECISION,
    budget_fraction DOUBLE PRECISION,
    admissible_fraction DOUBLE PRECISION,
    blocked_by_coalition INTEGER,
    z_before NUMERIC[],
    z_after NUMERIC[],
    result TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_tool_events_decision CHECK (decision IN ('admit','hold','deny'))
);
CREATE INDEX IF NOT EXISTS ix_tool_events_session_created ON tool_events (session_id, created_at);

CREATE TABLE IF NOT EXISTS memory_entries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    asker_id UUID NOT NULL REFERENCES askers(id) ON DELETE CASCADE,
    session_id UUID REFERENCES sessions(id) ON DELETE SET NULL,
    kind TEXT NOT NULL,
    content TEXT NOT NULL,
    importance DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    scope TEXT NOT NULL DEFAULT 'global',
    ttl_secs DOUBLE PRECISION,
    expires_at TIMESTAMPTZ,
    embedding DOUBLE PRECISION[],
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_accessed_at TIMESTAMPTZ,
    CONSTRAINT ck_memory_kind CHECK (kind IN ('fact','work','preference','episode'))
);
CREATE INDEX IF NOT EXISTS ix_memory_asker_kind ON memory_entries (asker_id, kind);

CREATE TABLE IF NOT EXISTS guardrail_events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    asker_id UUID NOT NULL REFERENCES askers(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    allowed BOOLEAN NOT NULL,
    risk DOUBLE PRECISION NOT NULL DEFAULT 0,
    reason TEXT NOT NULL DEFAULT 'ok',
    model TEXT,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_guardrail_stage CHECK (stage IN ('input','classify','output','engine'))
);
CREATE INDEX IF NOT EXISTS ix_guardrail_session_created ON guardrail_events (session_id, created_at);

CREATE TABLE IF NOT EXISTS calibrations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    metric NUMERIC[][] NOT NULL,
    budget DOUBLE PRECISION NOT NULL,
    alpha DOUBLE PRECISION NOT NULL,
    review_band DOUBLE PRECISION NOT NULL,
    projection_distance DOUBLE PRECISION NOT NULL DEFAULT 0,
    quantile DOUBLE PRECISION NOT NULL DEFAULT 0.999,
    sample_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS finetune_datasets (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'seed',
    sample_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_finetune_datasets_name UNIQUE (name)
);

CREATE TABLE IF NOT EXISTS finetune_samples (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    dataset_id UUID NOT NULL REFERENCES finetune_datasets(id) ON DELETE CASCADE,
    input_text TEXT NOT NULL,
    label JSONB NOT NULL,
    split TEXT NOT NULL DEFAULT 'train',
    weight DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    source_event_id UUID REFERENCES tool_events(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_finetune_split CHECK (split IN ('train','val','test'))
);
CREATE INDEX IF NOT EXISTS ix_finetune_samples_dataset_split ON finetune_samples (dataset_id, split);

CREATE TABLE IF NOT EXISTS models (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL,
    family TEXT NOT NULL DEFAULT 'classifier',
    base_model TEXT NOT NULL,
    path TEXT NOT NULL,
    params JSONB NOT NULL DEFAULT '{}'::jsonb,
    metrics JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS eval_runs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    model_id UUID NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    dataset_id UUID NOT NULL REFERENCES finetune_datasets(id) ON DELETE CASCADE,
    metrics JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
