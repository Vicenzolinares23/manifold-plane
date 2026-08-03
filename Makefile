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
