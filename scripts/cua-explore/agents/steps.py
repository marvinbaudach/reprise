"""Declarative, observation-late action steps."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Mapping

from agents.vocabulary import LabelMatcher


@dataclass(frozen=True)
class Step:
    name: str
    kind: str
    matcher: LabelMatcher | None = None
    fields: Mapping[str, Any] = field(default_factory=dict)
    alternates: tuple["Step", ...] = ()
    required: bool = True
    atomic_with_next: bool = False
    token_hint: str | None = None


def step_to_action(
    step: Step, observation: Mapping[str, Any]
) -> tuple[dict[str, Any] | None, bool]:
    action = {
        "schema_version": 1,
        "state_id": str(observation.get("state_id", "")),
        "kind": step.kind,
        **dict(step.fields),
    }
    mismatch = False
    if step.matcher is not None:
        candidates, mismatch = step.matcher.candidates_with_role_fallback(observation)
        if not candidates:
            return None, mismatch
        action["target"] = {"label": candidates[0]}
    return action, mismatch
