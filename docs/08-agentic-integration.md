# Stage 8 — Agentic integration contract

The engine (`mp-core`, `mp-barrier`, `mp-adapters`) is Rust and stays Rust. This
document is the contract between the Rust engine, the Python agentic layer, and
Postgres. It is the coordinate system for the four workstreams that build it:

- **A** — Postgres schema, SQLAlchemy models, Alembic migrations
- **B** — `mp-daemon`: HTTP server exposing the engine
- **C** — LangChain/LangGraph agent: graph, tools, memory, daemon gateway
- **D** — Guardrails + small-model fine-tuning pipeline

Every boundary below is a promise. Workstreams build against the contract, not
against each other. Where the two sides of a seam land at slightly different
times, the owning workstream ships a local stub for its own tests and the
integration is verified after merge.

## 8.1 Why this split

`docs/05` §5.11 says the kernel's guarantee is conditional on `g` — the
displacement function that turns a request into bits. The engine is trustworthy
exactly to the extent that the *measurement* feeding it is. That measurement is
the weak part, and it is where an LLM earns its place: a small fine-tuned model
that classifies a tool call into the `ToolKind` + taint + sensitivity fields the
agent adapter consumes (`crates/mp-adapters/src/agent.rs`). Postgres is the
audit trail and the memory. LangGraph is the loop that holds the engine between
the model and the tools.

The pieces are deliberately separable. The agent runs with the daemon down by
using the heuristic fallback classifier; the daemon runs with the agent down
and is exercised directly by integration probes.

## 8.2 System diagram

```text
┌──────────────────────────────  LangGraph agent (C)  ─────────────────────────────┐
│                                                                                  │
│  model (Ollama phi4 / any) ──► agent node ──► GATE node ──► ToolNode ──► memory  │
│                                     ▲              │            ▲                 │
│                                     │        classify (D)      │                 │
│                                     │        input guard (D)    │                 │
│                                     └──────── output guard (D)   │                 │
│                                                  │              │                 │
│                          POST /v1/decide (C's gateway.py)       │                 │
└──────────────────────────────────────┬───────────────────────────┘                 │
                                       │                                             │
                       ┌───────────────▼──────────────┐                  ┌──────────▼──────────┐
                       │   mp-daemon  (B, Rust/axum)  │                  │  Postgres (A)       │
                       │  engine.decide → ADMIT/HOLD/ │                  │  askers, sessions,  │
                       │  DENY + full verdict         │                  │  tool_events,       │
                       └───────────────┬──────────────┘                  │  memory, guardrails,│
                                       │  calibration / state snapshots  │  finetune, evals    │
                                       └────────────────────────────────►┘                     │
```

Data always flows model → classify → engine → decision → tool or replan. The
engine sees only bits. Postgres sees everything, including the bits.

## 8.3 The daemon HTTP API (owned by B, consumed by C and D)

Base URL `http://localhost:8787`. All payloads JSON. Times are Unix epoch
seconds as float. Vectors are length-6 float arrays in axis order
`[authority, reach, irreversibility, opacity, coupling, tempo]`.

### `POST /v1/decide`
Body:
```json
{
  "asker_id": "agent-session-7f3a",
  "symmetry_class": "default",
  "tool_call": {
    "kind": "SendExternal",
    "payload_bytes": 5120,
    "recipients": 1,
    "argument_tainted": true,
    "off_transcript": false,
    "source_sensitivity": 0.8
  },
  "at": 1754246800.5
}
```
`kind` ∈ `ReadLocal | ReadExternal | WriteLocal | SendExternal | Execute |
SelfModify | Delegate`.

Response `200`:
```json
{
  "decision": "admit",
  "admissible_fraction": 1.0,
  "coalitions_checked": 0,
  "blocked_by_coalition": null,
  "margin_before": 98.4,
  "margin_after": 91.2,
  "required": 93.5,
  "alpha_effective": 0.05,
  "orbit_residual": 0.0,
  "budget_fraction": 0.081,
  "state_after": [0.2, 1.1, 2.4, 0.0, 0.0, 0.0],
  "denied": 0,
  "held": 0,
  "admitted": 12
}
```
`decision` ∈ `admit | hold | deny`. The state snapshot lets the Python layer
persist the engine position to Postgres after every decision.

### `GET /v1/askers`
`{"askers": [{"asker_id": "...", "symmetry_class": "...", "z": [...], "last_seen": 1.0, "admitted": 0, "denied": 0, "held": 0, "relaxed_z": [...]}]}`

### `GET /v1/askers/{id}`
Same shape, single object, `404` if unknown.

### `PUT /v1/askers/{id}`
Seeding: reconstruct engine state from Postgres after a restart.
```json
{ "symmetry_class": "default", "z": [0,0,0,0,0,0], "last_seen": 1754246800.5 }
```
Returns the stored asker.

### `PUT /v1/coupling`
```json
{ "a": "asker-1", "b": "asker-2", "kappa_bits": 1.5 }
```

### `POST /v1/calibrate`
Fit `M`, budget `c`, and report the projection distance.
```json
{ "samples": [[0.1,0,0,0,0,0], [0,0.2,0,0,0,0]], "quantile": 0.999 }
```
Response: `{"metric": [[...6x6...]], "budget": 12.4, "projection_distance": 0.003,
"alpha": 0.05, "review_band": 0.02}`. Persist this in Postgres `calibrations`.

### `GET /v1/config`
The active `BarrierConfig` + `EngineConfig` as JSON.

## 8.4 Shared Python contracts (in `agentic/manifold_agent/state.py`)

Frozen Pydantic v2 models. **Owned by the orchestrator, immutable during the
build.** Workstreams import, never edit.

- `Axis` — str enum of the six axes.
- `ToolKind` — str enum matching the daemon's `kind` strings.
- `ToolCall` — `name: str`, `kind: ToolKind`, `arguments: dict`,
  `payload_bytes: int`, `recipients: int`, `argument_tainted: bool`,
  `off_transcript: bool`, `source_sensitivity: float`.
- `Decision` — str enum `admit | hold | deny`.
- `Verdict` — mirror of the decide response.
- `AskerSpec` — `asker_id`, `symmetry_class`, `z: list[float]`, `last_seen`.
- `ToolResult` — `call: ToolCall`, `result: str`, `ok: bool`, `error: str|None`.
- `MemoryEntry` — `key`, `kind` (`fact | work | preference | episode`),
  `content`, `importance: float`, `scope: str`, `ttl_secs: float|None`,
  `metadata: dict`.
- `GuardrailReport` — `allowed: bool`, `reason: str`, `risk: float`,
  `details: dict`, `model: str|None`.

## 8.5 Postgres schema (owned by A, consumed by C and D)

Database `manifold_plane`, user/password `manifold/manifold`, port `5432`.

| table | purpose | key columns |
|---|---|---|
| `askers` | agent identity + carried engine state | `id` uuid pk, `asker_id` text uniq, `symmetry_class`, `z` numeric[6], `last_seen`, `admitted/denied/held`, `baseline` numeric[6], `created_at` |
| `sessions` | LangGraph threads | `id` uuid pk, `asker_id` fk, `thread_id` text uniq, `started_at`, `ended_at` |
| `messages` | transcript, every role | `id` uuid pk, `session_id` fk, `role`, `content`, `created_at`, `message_id` |
| `tool_events` | every gated tool call + decision | `id` uuid pk, `session_id` fk, `asker_id` fk, `tool_name`, `kind`, `arguments` jsonb, `payload_bytes`, `recipients`, `argument_tainted`, `off_transcript`, `source_sensitivity`, `decision`, `margin_before/after`, `required`, `alpha_effective`, `orbit_residual`, `budget_fraction`, `admissible_fraction`, `blocked_by_coalition`, `z_before` numeric[6], `z_after` numeric[6], `result`, `created_at` |
| `memory_entries` | long-term memory | `id` uuid pk, `asker_id` fk, `session_id` fk null, `kind`, `content`, `importance`, `scope`, `ttl_secs`, `expires_at`, `embedding` vector(384) null, `metadata` jsonb, `created_at`, `last_accessed_at` |
| `guardrail_events` | every guardrail decision | `id` uuid pk, `session_id` fk, `asker_id` fk, `stage` (`input|classify|output|engine`), `allowed`, `risk`, `reason`, `model`, `details` jsonb, `created_at` |
| `calibrations` | engine fit snapshots | `id` uuid pk, `metric` numeric[6][6], `budget`, `alpha`, `review_band`, `projection_distance`, `quantile`, `sample_count`, `created_at` |
| `finetune_datasets` | named training sets | `id` uuid pk, `name` uniq, `source` text, `sample_count`, `created_at` |
| `finetune_samples` | labeled training rows | `id` uuid pk, `dataset_id` fk, `input_text`, `label` jsonb, `split` (`train|val|test`), `weight`, `source_event_id` fk null, `created_at` |
| `models` | model registry | `id` uuid pk, `name`, `family`, `base_model`, `path`, `params`, `metrics` jsonb, `created_at` |
| `eval_runs` | fine-tune eval results | `id` uuid pk, `model_id` fk, `dataset_id` fk, `metrics` jsonb, `created_at` |

Indexes: unique indexes on `askers.asker_id`, `sessions.thread_id`,
`finetune_datasets.name`; index on `tool_events (session_id, created_at)`,
`memory_entries (asker_id, kind)`, `guardrail_events (session_id, created_at)`,
`finetune_samples (dataset_id, split)`.

**A owns everything under `agentic/migrations/` and
`agentic/manifold_agent/db/`.** Models are SQLAlchemy 2.0 (`DeclarativeBase`,
`Mapped`), module `manifold_agent.db.models`, session helper
`manifold_agent.db.session.SessionLocal`, engine factory
`manifold_agent.db.engine.create_engine_from_env`. Migrations are Alembic,
revision history under `agentic/migrations/versions/`.

## 8.6 The agent graph (owned by C)

Modules under `agentic/manifold_agent/agent/`:
`graph.py` (build + compile), `nodes.py` (agent, gate, tools, memory, replan),
`prompts.py`. State is LangGraph `StateGraph` over `agentic/manifold_agent/state.py`
extended with runtime fields (keep those in `agent/graph.py`).

Edges: `START → agent → gate → tools → memory → agent` with a `replan`
conditional after `gate` when the decision is `deny` (the model is told why and
asked to choose a different, admitted step) and a `hold` path that escalates to
a human-in-the-loop interrupt. The gate node calls
`manifold_agent.guardrails.classify.classify_tool_call` (owned by D) and
`manifold_agent.gateway.decide` (owned by C). **If the classifier module is not
present, the gate falls back to a pure-heuristic classifier defined in the gate
node** so the graph runs stand-alone. Never let a tool execute on `deny`; a
`hold` must interrupt before execution.

Tools live in `agentic/manifold_agent/tools/`: `registry.py` (a `@tool_registry`
decorator + `ALL_TOOLS`), one module per kind mirroring the adapter's `ToolKind`
semantics (`read_local`, `read_external`, `write_local`, `send_external`,
`execute`, `self_modify`, `delegate`). Every registered tool call is routed
through the gate — the tools never execute before the engine says admit.

Memory lives in `agentic/manifold_agent/memory/`: `session.py`
(LangGraph `PostgresSaver` checkpointing through the A session factory),
`longterm.py` (store/recall/fade over `memory_entries`), `retrieval.py`
(keyword + optional embedding search). Memory tools (`remember`, `recall`,
`forget`) are registered in the registry like any tool and therefore gated.

## 8.7 Guardrails and fine-tuning (owned by D)

Modules under `agentic/manifold_agent/guardrails/`:
- `classify.py` — `classify_tool_call(name, arguments, context) -> ToolCall`.
  The measurement layer: fills `kind`, `argument_tainted`,
  `source_sensitivity`, `payload_bytes`, `recipients`, `off_transcript`.
  Default: heuristic rules. If a fine-tuned classifier model is configured,
  route through it first, fall back to heuristics on low confidence.
- `input.py` — `check_input(user_message, context) -> GuardrailReport`
  (prompt-injection, off-topic, jailbreak heuristics + optional model judge).
- `output.py` — `check_output(content, context) -> GuardrailReport`
  (PII/secret exfiltration patterns, length, formatting).
- `gateway.py` — composes input → classify → engine → output into one
  `GateReport`; the graph's gate node may call it directly.

Fine-tuning lives in `agentic/manifold_agent/training/`:
- `dataset.py` — build `finetune_samples` from `tool_events` +
  `guardrail_events` (+ `scripts/seed_corpus.py`); split, dedupe, count.
- `train.py` — LoRA (PEFT) + TRL SFTTrainer on a small instruct model
  (default `Qwen/Qwen2.5-0.5B-Instruct`); CLI entry `agentic/scripts/train_guardrail.py`;
  records the run in `models`.
- `eval.py` — held-out accuracy on `finetune_samples` split=test; writes
  `eval_runs`.
- `ollama_export.py` — write a Modelfile (GGUF via llama.cpp optional; if the
  toolchain is absent, emit the Modelfile template and document) so the
  classifier can serve from Ollama.
- `classify_integration.py` — load the trained adapter and satisfy
  `classify_tool_call` with it (the seam with the guardrails module).

The seed corpus in `scripts/seed_corpus.py` must include the four moves from
`docs/00`: benign reads, the classic exfiltration chain (read external → read
local → send external), and tainted-vs-clean pairs, so the classifier has
something to learn from before real traffic exists.

## 8.8 Configuration

`agentic/manifold_agent/config.py` (orchestrator-owned, immutable during build):
env-driven dataclass `Settings` with `engine_url`, `database_url`, `llm` section
(provider, model, base_url), `guardrails` section (judge model, thresholds),
`training` section (base model, output dir). Defaults target local Ollama at
`http://localhost:11434` and the daemon at `http://localhost:8787`.

## 8.9 Verification gates

- Rust: `cargo test` green in `mp-daemon` (B). Integration probes in B hit the
  running server and assert the three outcomes.
- Python: `python -m pytest` green for each workstream's own tests, run against
  the Postgres container (A, C, D). C and D tests that need the daemon use a
  `gateway` HTTPX mock or the heuristic fallback when the daemon is not running.
- End-to-end (orchestrator after merge): start Postgres + daemon, run
  `agentic/examples/demo_agent.py`, observe a tool call admitted, then a
  deliberately dangerous chain throttled, with rows in `tool_events`,
  `guardrail_events`, `memory_entries`.

## 8.10 File ownership matrix

| path | owner |
|---|---|
| `docs/08-agentic-integration.md`, `agentic/pyproject.toml`, `agentic/manifold_agent/{__init__,state,config}.py`, `docker-compose.yml`, `Makefile`, root `README.md` | orchestrator |
| `crates/mp-daemon/**`, workspace `Cargo.toml` (deps only) | B |
| `agentic/migrations/**`, `agentic/manifold_agent/db/**`, `agentic/sql/schema.sql`, `agentic/tests/db/**` | A |
| `agentic/manifold_agent/{gateway.py,agent,tools,memory}/**`, `agentic/examples/**`, `agentic/tests/{agent,tools,memory}/**` | C |
| `agentic/manifold_agent/{guardrails,training}/**`, `agentic/scripts/**`, `agentic/tests/{guardrails,training}/**` | D |

No workstream edits another's path. If a seam needs a different shape, that is a
contract change: leave it, note it in the commit message, and the orchestrator
resolves it at integration time.

## 8.11 Commit discipline

Each workstream commits its own subtree in logical units with conventional
messages matching the repo (`feat(scope): ...`, `fix(...): ...`, `docs: ...`,
`test(...): ...`). Rust tests must pass before B's final commit. Python tests
must pass before each workstream's final commit. The orchestrator verifies
`cargo test` at workspace root and `pytest` under `agentic/` after merge, then
pushes.
