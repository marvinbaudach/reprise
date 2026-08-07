"""Small deterministic probes inserted only from surplus action budget."""

from __future__ import annotations

import random
from typing import Any, Mapping


def initial_probe(
    mission: Mapping[str, Any], observation: Mapping[str, Any], rng: random.Random
) -> dict[str, Any] | None:
    if "wait" not in mission.get("capabilities", []):
        return None
    return {
        "schema_version": 1,
        "state_id": str(observation.get("state_id", "")),
        "kind": "wait",
        "duration_ms": rng.choice((250, 500, 750)),
        "expect_status": False,
    }
