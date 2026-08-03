# Ollama with an 8B model baked in at build time.
#
# Pre-pulling during the build eliminates the multi-second first-request stall
# that model loading otherwise costs, and — more usefully here — it makes the
# demo reproducible: the agent harness in `examples/` gets the same model
# weights every run, so the tool-call traces it produces are comparable across
# machines rather than depending on what happened to be pulled.
FROM ollama/ollama:latest

# Space-separated. Defaults to an 8B-parameter model.
ARG MODELS="llama3:8b"
ARG PRE_PULL_MODELS="true"

ENV OLLAMA_HOST=0.0.0.0

# Start the server, pull, stop. Done in one layer so the weights land in the
# image rather than in a volume that a fresh `docker compose up` would discard.
RUN if [ "$PRE_PULL_MODELS" = "true" ]; then \
      echo "pre-pulling: $MODELS" && \
      (ollama serve &) && \
      for i in $(seq 1 60); do \
        if ollama list >/dev/null 2>&1; then break; fi; \
        sleep 1; \
      done && \
      for m in $MODELS; do \
        echo "==> pulling $m" && ollama pull "$m" || exit 1; \
      done && \
      pkill -f "ollama serve" || true; \
    else \
      echo "pre-pull disabled; models fetch on first request"; \
    fi

HEALTHCHECK --interval=10s --timeout=5s --retries=12 --start-period=30s \
  CMD ollama list >/dev/null 2>&1 || exit 1
