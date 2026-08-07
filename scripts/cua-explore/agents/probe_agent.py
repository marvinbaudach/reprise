#!/usr/bin/env python3
"""Tiny observation recorder for display-backed vocabulary calibration."""

from __future__ import annotations

import json
import os
import pathlib
import sys


def main() -> int:
    root = pathlib.Path(os.environ.get("HOME", ".")) / "observations"
    root.mkdir(parents=True, exist_ok=True)
    for index, line in enumerate(sys.stdin):
        request = json.loads(line)
        observation = request.get("observation", {})
        (root / f"{index:03}.json").write_text(
            json.dumps(observation, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        state_id = str(observation.get("state_id", ""))
        labels = observation.get("actionable_labels", [])
        if index >= 14:
            action = {
                "schema_version": 1,
                "state_id": state_id,
                "kind": "finish",
                "reason": "Vocabulary probe retained 15 observations",
            }
        elif labels:
            action = {
                "schema_version": 1,
                "state_id": state_id,
                "kind": "activate",
                "target": {"label": str(labels[index % len(labels)])},
                "dispatch": "ax",
                "expect_effect": "idempotent",
            }
        else:
            action = {
                "schema_version": 1,
                "state_id": state_id,
                "kind": "wait",
                "duration_ms": 250,
                "expect_status": False,
            }
        print(json.dumps(action, separators=(",", ":"), sort_keys=True), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
