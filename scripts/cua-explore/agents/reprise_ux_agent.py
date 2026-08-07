#!/usr/bin/env python3
"""JSONL entrypoint for the bundled deterministic Reprise UX agent."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import sys
from typing import Any, Mapping, Sequence


EXPLORE_ROOT = pathlib.Path(__file__).resolve().parent.parent
if str(EXPLORE_ROOT) not in sys.path:
    sys.path.insert(0, str(EXPLORE_ROOT))

from agents.agent_core import AgentSession  # noqa: E402


MAX_REASON_CHARS = 400


def _internal_error(observation: Mapping[str, Any], error: Exception) -> dict[str, Any]:
    reason = f"agent-internal-error: {type(error).__name__}: {str(error)[:180]}"
    return {
        "schema_version": 1,
        "state_id": str(observation.get("state_id", "")),
        "kind": "finish",
        "reason": reason[:MAX_REASON_CHARS],
    }


def run_loop(session: AgentSession) -> int:
    for line in sys.stdin:
        observation: Mapping[str, Any] = {}
        try:
            request = json.loads(line)
            if not isinstance(request, dict):
                raise ValueError("request must be an object")
            mission = request.get("mission")
            observation = request.get("observation")
            history = request.get("recent_history", [])
            if not isinstance(mission, dict) or not isinstance(observation, dict):
                raise ValueError("request mission and observation must be objects")
            if not isinstance(history, list):
                raise ValueError("recent_history must be a list")
            action = session.next_action(mission, observation, history)
        except Exception as error:
            action = _internal_error(observation, error)
            print(json.dumps(action, separators=(",", ":"), sort_keys=True), flush=True)
            break
        print(json.dumps(action, separators=(",", ":"), sort_keys=True), flush=True)
    return 0


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument(
        "--notes-dir",
        type=pathlib.Path,
        default=pathlib.Path(os.environ.get("HOME", ".")),
    )
    parser.add_argument("--probe-ratio", type=float, default=1.0)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    session = AgentSession(
        seed=args.seed,
        notes_dir=args.notes_dir,
        probe_ratio=max(0.0, args.probe_ratio),
    )
    return run_loop(session)


if __name__ == "__main__":
    raise SystemExit(main())
