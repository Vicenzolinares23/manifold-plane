"""Shared contracts for the agentic layer.

Owned by the orchestrator and immutable during the build
(`docs/08` §8.4). Workstreams import these types; they never edit this module.
If a seam needs a different shape, that is a contract change and goes through
integration.
"""

from __future__ import annotations

from enum import Enum
from typing import Any

from pydantic import BaseModel, Field

# The six capability axes in engine order. Matches `mp_core::axis::Axis`.
AXES = ["authority", "reach", "irreversibility", "opacity", "coupling", "tempo"]


class ToolKind(str, Enum):
    """What a tool does, independent of what it is called.

    Matches the daemon's ``kind`` strings and `mp_adapters::agent::ToolKind`.
    """

    READ_LOCAL = "ReadLocal"
    READ_EXTERNAL = "ReadExternal"
    WRITE_LOCAL = "WriteLocal"
    SEND_EXTERNAL = "SendExternal"
    EXECUTE = "Execute"
    SELF_MODIFY = "SelfModify"
    DELEGATE = "Delegate"


class Decision(str, Enum):
    """The three-way admission outcome from `docs/05` §5.5."""

    ADMIT = "admit"
    HOLD = "hold"
    DENY = "deny"


class ToolCall(BaseModel):
    """A tool invocation reduced to the fields the agent adapter measures.

    These fields are the measurement layer's output: they are what the Rust
    engine turns into a displacement in bits.
    """

    name: str
    kind: ToolKind = ToolKind.READ_LOCAL
    arguments: dict[str, Any] = Field(default_factory=dict)
    payload_bytes: int = 0
    recipients: int = 0
    argument_tainted: bool = False
    off_transcript: bool = False
    source_sensitivity: float = 0.01

    model_config = {"frozen": False, "extra": "forbid"}


class Verdict(BaseModel):
    """Mirror of the daemon's `/v1/decide` response."""

    decision: Decision
    admissible_fraction: float
    coalitions_checked: int
    blocked_by_coalition: int | None = None
    margin_before: float
    margin_after: float
    required: float
    alpha_effective: float
    orbit_residual: float
    budget_fraction: float
    state_after: list[float]
    denied: int
    held: int
    admitted: int


class AskerSpec(BaseModel):
    """Engine state for one asker, seedable via ``PUT /v1/askers/{id}``."""

    asker_id: str
    symmetry_class: str = "default"
    z: list[float] = Field(default_factory=lambda: [0.0] * 6)
    last_seen: float = 0.0
    admitted: int = 0
    denied: int = 0
    held: int = 0


class ToolResult(BaseModel):
    """Outcome of a gated tool execution."""

    call: ToolCall
    result: str = ""
    ok: bool = True
    error: str | None = None


class MemoryKind(str, Enum):
    FACT = "fact"
    WORK = "work"
    PREFERENCE = "preference"
    EPISODE = "episode"


class MemoryEntry(BaseModel):
    """A long-term memory row persisted to ``memory_entries``."""

    key: str
    kind: MemoryKind = MemoryKind.FACT
    content: str
    importance: float = 0.5
    scope: str = "global"
    ttl_secs: float | None = None
    metadata: dict[str, Any] = Field(default_factory=dict)


class GuardrailStage(str, Enum):
    INPUT = "input"
    CLASSIFY = "classify"
    OUTPUT = "output"
    ENGINE = "engine"


class GuardrailReport(BaseModel):
    """A single guardrail's verdict."""

    allowed: bool = True
    reason: str = "ok"
    risk: float = 0.0
    details: dict[str, Any] = Field(default_factory=dict)
    model: str | None = None


class GateReport(BaseModel):
    """The composed result of input → classify → engine → output."""

    input_check: GuardrailReport
    tool_call: ToolCall
    verdict: Verdict | None = None
    output_check: GuardrailReport | None = None
    admissible: bool = True
    reason: str = "ok"
