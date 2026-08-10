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
    skip_when: LabelMatcher | None = None
    missing_code: str | None = None


_STRUCTURAL_ACTION_PREFIXES_LOCAL = (
    "listitem.",
    "list.",
    "win.",
    "window.",
    "default.",
)


def _invocable_actions_local(names: object) -> tuple[str, ...]:
    """Temporary local copy until STROM I's shared vocabulary is integrated."""
    if not isinstance(names, (list, tuple)):
        return ()
    return tuple(
        str(name)
        for name in names
        if isinstance(name, str)
        and not name.startswith(_STRUCTURAL_ACTION_PREFIXES_LOCAL)
    )


def step_is_satisfied(step: Step, observation: Mapping[str, Any]) -> bool:
    return step.skip_when is not None and step.skip_when.resolve(observation) is not None


def _activation_dispatch(
    observation: Mapping[str, Any], label: str, default: str
) -> str:
    matches = [
        item
        for item in observation.get("elements", [])
        if isinstance(item, dict) and item.get("label") == label
    ]
    matches.sort(
        key=lambda item: (
            float(item.get("frame", {}).get("y", 0)),
            float(item.get("frame", {}).get("x", 0)),
        )
    )
    if not matches or not all("actions" in item for item in matches):
        return default
    if any(_invocable_actions_local(item.get("actions")) for item in matches):
        return "ax"
    frame = matches[0].get("frame", {})
    width = frame.get("width", frame.get("w", 0)) if isinstance(frame, dict) else 0
    height = frame.get("height", frame.get("h", 0)) if isinstance(frame, dict) else 0
    if matches[0].get("geometry_trusted") is not False and width > 0 and height > 0:
        return "px"
    return default


def step_to_action(
    step: Step,
    observation: Mapping[str, Any],
    *,
    force_dispatch: str | None = None,
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
        if step.kind == "activate":
            declared = str(action.get("dispatch", "auto"))
            action["dispatch"] = force_dispatch or _activation_dispatch(
                observation,
                candidates[0],
                "ax" if declared == "auto" else declared,
            )
    return action, mismatch
