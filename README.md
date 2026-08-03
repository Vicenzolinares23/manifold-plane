# manifold-plane

Trajectory-based admission control.

The mathematics was derived first (`docs/`), then implemented. The engine is a
discrete-time exponential control barrier function over a six-dimensional
capability state space (bits), proven to keep any asker inside its safe set no
matter what sequence of individually-acceptable requests it makes.

## Architecture

- **Rust engine** — `mp-core` (state space, metric), `mp-barrier` (admission
  kernel, coalitions, orbit residual), `mp-adapters` (k8s / ICS / LLM-agent
  request adapters), `mp-daemon` (HTTP server exposing the engine).
- **Agentic layer** (`agentic/`, Python) — a LangGraph agent whose tool calls
  are gated by the engine before they execute: classify → engine → tool or
  replan. Long-term memory and every decision persist to Postgres. Guardrails
  (input, tool-call measurement, output) plus a small fine-tuned classifier for
  the measurement layer.

See `docs/08-agentic-integration.md` for the integration contract.

## Quick start

```sh
make db-up          # Postgres on :5432
make engine         # mp-daemon on :8787
make db-migrate     # Alembic schema
make agent-demo     # LangGraph agent with a gated tool chain
```

## Verify

```sh
make rust-test      # cargo test --workspace
make py-test        # pytest under agentic/
```

## Status

The engine (docs 0–7, `mp-core`/`mp-barrier`/`mp-adapters`) is proven and
tested. The agentic layer (docs 8, `agentic/`) implements LangGraph, tools, memory, guardrails, fine-tune hooks, and Postgres schema.
