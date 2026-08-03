# manifold-plane development tasks.

.PHONY: test rust-test py-test db-up db-down db-migrate engine build-engine
.PHONY: agent-demo train eval clean

# Rust engine
rust-test:
	cargo test --workspace

engine:
	cargo run -p mp-daemon --release

build-engine:
	cargo build -p mp-daemon --release

# Postgres
db-up:
	docker compose up -d postgres

db-down:
	docker compose down

db-migrate:
	cd agentic && alembic upgrade head

# Python agentic layer
py-test:
	cd agentic && python -m pytest

agent-demo:
	cd agentic && python -m manifold_agent.examples.demo_agent

train:
	cd agentic && python -m manifold_agent.training.train

eval:
	cd agentic && python -m manifold_agent.training.eval

clean:
	cd agentic && rm -rf .venv .pytest_cache models/classifier || true
	cargo clean
