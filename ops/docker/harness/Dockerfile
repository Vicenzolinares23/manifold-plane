# Agent harness. Standard library only — urllib and json are enough to talk to
# both Ollama and the daemon, and a dependency-free harness is one less thing
# between a measurement and believing it.
FROM python:3.12-slim

WORKDIR /app
COPY examples/agent_harness.py /app/agent_harness.py

RUN useradd --create-home --uid 10001 harness
USER harness

ENTRYPOINT ["python", "/app/agent_harness.py"]
CMD ["--wait", "120"]
