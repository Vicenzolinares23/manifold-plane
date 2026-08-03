"""manifold-plane agentic layer.

A LangGraph agent whose tool calls are gated by the Rust manifold-plane engine
(`docs/08-agentic-integration.md`). The engine sees only bits; Postgres sees
everything.

Subpackages:
- ``agent``   — the LangGraph graph, nodes, prompts
- ``tools``   — tool registry gated by the engine
- ``memory``  — session and long-term memory
- ``guardrails`` — input/output/tool-call classification and checks
- ``training``   — small-model fine-tuning for the measurement layer
- ``db``      — SQLAlchemy models and session management
"""

__version__ = "0.1.0"
