.PHONY: help test check validate bench fmt lint docker demo clean all

help:
	@echo "make test      cargo test --all"
	@echo "make validate  independent validation in sim/ (no dependencies)"
	@echo "make check     fmt + clippy + test + validate"
	@echo "make bench     decision latency"
	@echo "make docker    build the dev stack"
	@echo "make demo      run the agent harness against a local model"

test:
	cargo test --all

# The independent reimplementation. If this disagrees with cargo test,
# at least one of them is wrong and finding out which is the work.
validate:
	python3 sim/run_all.py

fmt:
	cargo fmt --all

lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

check: lint test validate

bench:
	cargo bench -p mp-barrier

docker:
	DOCKER_BUILDKIT=1 docker compose -f docker-compose.dev.yml build

demo: docker
	docker compose -f docker-compose.dev.yml up -d
	docker compose -f docker-compose.dev.yml --profile demo run --rm agent-harness

clean:
	cargo clean
	rm -rf sim/__pycache__

all: check

# Agentic layer (Python / Stage 8)
.PHONY: db-up db-down db-migrate py-test agent-demo train eval

db-up:
	docker compose up -d postgres

db-down:
	docker compose down

db-migrate:
	cd agentic && alembic upgrade head

py-test:
	cd agentic && python -m pytest

agent-demo:
	cd agentic && python -m manifold_agent.examples.demo_agent

train:
	cd agentic && python -m manifold_agent.training.train --dry-run

eval:
	cd agentic && python -m manifold_agent.training.eval
