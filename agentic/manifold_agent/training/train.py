"""LoRA fine-tune entrypoint (optional heavy deps)."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from manifold_agent.config import get_settings
from manifold_agent.scripts.seed_corpus import seed_rows
from manifold_agent.training.dataset import samples_from_seed


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Fine-tune the tool-call classifier")
    parser.add_argument("--dry-run", action="store_true", help="Build dataset only; do not train")
    parser.add_argument("--output-dir", default=None)
    args = parser.parse_args(argv)

    settings = get_settings().training
    out = Path(args.output_dir or settings.output_dir)
    out.mkdir(parents=True, exist_ok=True)

    samples = samples_from_seed(seed_rows())
    (out / "dataset.jsonl").write_text(
        "\n".join(json.dumps({"input_text": s.input_text, "label": s.label, "split": s.split}) for s in samples)
        + "\n",
        encoding="utf-8",
    )
    if args.dry_run:
        print(f"wrote {len(samples)} samples to {out / 'dataset.jsonl'}")
        return 0

    try:
        import torch  # noqa: F401
        from peft import LoraConfig, get_peft_model  # noqa: F401
        from transformers import AutoModelForCausalLM, AutoTokenizer  # noqa: F401
        from trl import SFTTrainer  # noqa: F401
    except ImportError as exc:
        raise SystemExit(
            "Training extras not installed. pip install -e '.[training]' or pass --dry-run"
        ) from exc

    # Real training path is intentionally minimal here — dry-run is the default CI path.
    print("torch/peft available; wire full SFTTrainer run for offline GPU jobs.")
    (out / "READY").write_text(settings.base_model, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
