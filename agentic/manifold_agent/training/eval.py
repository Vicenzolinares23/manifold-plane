"""Held-out eval for classifier labels."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from manifold_agent.guardrails.classify import classify_tool_call
from manifold_agent.scripts.seed_corpus import seed_rows
from manifold_agent.training.dataset import samples_from_seed


def evaluate(samples_path: Path | None = None) -> dict[str, float]:
    if samples_path and samples_path.exists():
        rows = [json.loads(line) for line in samples_path.read_text(encoding="utf-8").splitlines() if line.strip()]
        samples = [
            type("S", (), {"input_text": r["input_text"], "label": r["label"], "split": r.get("split", "test")})()
            for r in rows
        ]
    else:
        samples = samples_from_seed(seed_rows())

    test = [s for s in samples if getattr(s, "split", "test") == "test"] or list(samples)
    correct = 0
    for s in test:
        # input_text format: "tool_name :: json_args"
        if " :: " in s.input_text:
            name, args_s = s.input_text.split(" :: ", 1)
            try:
                args = json.loads(args_s)
            except json.JSONDecodeError:
                args = {}
        else:
            name, args = s.input_text, {}
        pred = classify_tool_call(name, args if isinstance(args, dict) else {})
        want = s.label.get("kind")
        if pred.kind.value == want:
            correct += 1
    acc = correct / max(len(test), 1)
    return {"accuracy": acc, "n": float(len(test))}


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--dataset", type=Path, default=None)
    args = p.parse_args(argv)
    metrics = evaluate(args.dataset)
    print(json.dumps(metrics))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
