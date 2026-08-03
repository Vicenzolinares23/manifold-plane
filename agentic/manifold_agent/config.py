"""Runtime configuration for the agentic layer.

Orchestrator-owned and immutable during the build (`docs/08` §8.8). Workstreams
read from this module; they never edit it. Anything workstream-specific belongs
in that workstream's own module.

All settings are env-driven with local-first defaults.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from functools import lru_cache
from os import getenv

_ENGINE_URL = "http://localhost:8787"
_DB_URL = "postgresql+psycopg://manifold:manifold@localhost:5432/manifold_plane"


@dataclass(frozen=True)
class LlmSettings:
    provider: str = field(default_factory=lambda: getenv("MP_LLM_PROVIDER", "ollama"))
    model: str = field(default_factory=lambda: getenv("MP_LLM_MODEL", "phi4:14b"))
    base_url: str = field(default_factory=lambda: getenv("MP_LLM_BASE_URL", "http://localhost:11434/v1"))
    temperature: float = float(getenv("MP_LLM_TEMPERATURE", "0.2"))


@dataclass(frozen=True)
class GuardrailSettings:
    judge_model: str = field(default_factory=lambda: getenv("MP_JUDGE_MODEL", "phi4:14b"))
    input_risk_threshold: float = float(getenv("MP_INPUT_RISK_THRESHOLD", "0.8"))
    output_risk_threshold: float = float(getenv("MP_OUTPUT_RISK_THRESHOLD", "0.8"))
    use_judge: bool = getenv("MP_USE_JUDGE", "0") == "1"


@dataclass(frozen=True)
class TrainingSettings:
    base_model: str = field(default_factory=lambda: getenv("MP_BASE_MODEL", "Qwen/Qwen2.5-0.5B-Instruct"))
    output_dir: str = field(default_factory=lambda: getenv("MP_TRAIN_OUTPUT", "models/classifier"))
    max_seq_length: int = int(getenv("MP_MAX_SEQ_LENGTH", "1024"))
    lora_r: int = int(getenv("MP_LORA_R", "16"))


@dataclass(frozen=True)
class Settings:
    engine_url: str = field(default_factory=lambda: getenv("MP_ENGINE_URL", _ENGINE_URL))
    database_url: str = field(default_factory=lambda: getenv("MP_DATABASE_URL", _DB_URL))
    symmetry_class: str = field(default_factory=lambda: getenv("MP_SYMMETRY_CLASS", "default"))
    llm: LlmSettings = field(default_factory=LlmSettings)
    guardrails: GuardrailSettings = field(default_factory=GuardrailSettings)
    training: TrainingSettings = field(default_factory=TrainingSettings)


@lru_cache(maxsize=1)
def get_settings() -> Settings:
    return Settings()
