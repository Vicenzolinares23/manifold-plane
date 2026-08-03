"""Emit an Ollama Modelfile template for the classifier."""

from __future__ import annotations

from pathlib import Path

from manifold_agent.config import get_settings

TEMPLATE = """FROM {base}

ADAPTER {adapter}

PARAMETER temperature 0
SYSTEM \"\"\"You classify agent tool calls into ToolKind + taint fields for manifold-plane.\"\"\"
"""


def write_modelfile(adapter_path: str | Path, dest: str | Path | None = None) -> Path:
    settings = get_settings().training
    dest_path = Path(dest or Path(settings.output_dir) / "Modelfile")
    dest_path.parent.mkdir(parents=True, exist_ok=True)
    dest_path.write_text(
        TEMPLATE.format(base=settings.base_model, adapter=adapter_path),
        encoding="utf-8",
    )
    return dest_path
