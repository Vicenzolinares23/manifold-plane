#!/usr/bin/env python3
"""Run every validation script. Exit non-zero if any check fails."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

SCRIPTS = ["units.py", "calibration.py", "adversarial.py"]


def main() -> int:
    here = Path(__file__).parent
    failed = []
    for s in SCRIPTS:
        print("=" * 72)
        r = subprocess.run([sys.executable, str(here / s)], cwd=here)
        if r.returncode != 0:
            failed.append(s)
        print()

    print("=" * 72)
    if failed:
        print(f"FAILED: {', '.join(failed)}")
        return 1
    print(f"all {len(SCRIPTS)} validation suites passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
