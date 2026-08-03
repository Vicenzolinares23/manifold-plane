"""Dataset builder for the measurement-layer classifier."""

from __future__ import annotations

import hashlib
import json
from dataclasses import asdict, dataclass
from typing import Any, Iterable


@dataclass
class Sample:
    input_text: str
    label: dict[str, Any]
    split: str = "train"
    weight: float = 1.0


def _split_for(key: str) -> str:
    h = int(hashlib.sha256(key.encode()).hexdigest(), 16) % 100
    if h < 70:
        return "train"
    if h < 85:
        return "val"
    return "test"


def samples_from_seed(rows: Iterable[dict[str, Any]]) -> list[Sample]:
    out: list[Sample] = []
    seen: set[str] = set()
    for row in rows:
        text = row["input_text"]
        if text in seen:
            continue
        seen.add(text)
        label = row["label"]
        out.append(
            Sample(
                input_text=text,
                label=label,
                split=_split_for(text),
                weight=float(row.get("weight", 1.0)),
            )
        )
    return out


def counts_by_split(samples: list[Sample]) -> dict[str, int]:
    counts = {"train": 0, "val": 0, "test": 0}
    for s in samples:
        counts[s.split] = counts.get(s.split, 0) + 1
    return counts


def to_jsonl(samples: list[Sample]) -> str:
    return "\n".join(json.dumps(asdict(s), sort_keys=True) for s in samples)
