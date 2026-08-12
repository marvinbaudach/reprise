"""Declarative, observation-late action steps."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Mapping

from agents.vocabulary import LabelMatcher
from ui_vocabulary import invocable_actions


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


def step_is_satisfied(step: Step, observation: Mapping[str, Any]) -> bool:
    return step.skip_when is not None and step.skip_when.resolve(observation) is not None


def _activation_dispatch(
    observation: Mapping[str, Any], label: str, default: str
) -> tuple[str, Mapping[str, Any] | None]:
    """The route plus, when the route is unproven, the evidence for saying so."""
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
        return default, None
    if any(invocable_actions(item.get("actions") or ()) for item in matches):
        return "ax", None
    frame = matches[0].get("frame", {})
    width = frame.get("width", frame.get("w", 0)) if isinstance(frame, dict) else 0
    height = frame.get("height", frame.get("h", 0)) if isinstance(frame, dict) else 0
    if matches[0].get("geometry_trusted") is not False and width > 0 and height > 0:
        return "px", None
    # Three measurements on two profiles: `ax` dispatched without any observable
    # effect while `px` always worked. The target offers no invocable action, so
    # the pointer route is the indicated one - but its geometry is unmeasured and
    # guessing a coordinate would be worse. Staying is right; staying silently is
    # not, because the run then acts on a route measured to do nothing.
    return default, {
        "target": label,
        "role": str(matches[0].get("role", "")),
        "actions": list(matches[0].get("actions") or ()),
        "dispatch": default,
        "geometry_trusted": matches[0].get("geometry_trusted"),
        "frame": dict(frame) if isinstance(frame, dict) else {},
    }


def step_to_action(
    step: Step,
    observation: Mapping[str, Any],
    *,
    force_dispatch: str | None = None,
) -> tuple[dict[str, Any] | None, bool, Mapping[str, Any] | None]:
    """The action, whether a role fallback was needed, and an unproven route."""
    action = {
        "schema_version": 1,
        "state_id": str(observation.get("state_id", "")),
        "kind": step.kind,
        **dict(step.fields),
    }
    mismatch = False
    dispatch_note = None
    if step.matcher is not None:
        candidates, mismatch = step.matcher.candidates_with_role_fallback(observation)
        if not candidates:
            return None, mismatch, None
        action["target"] = {"label": candidates[0]}
        if step.kind == "activate":
            declared = str(action.get("dispatch", "auto"))
            if force_dispatch:
                action["dispatch"] = force_dispatch
            else:
                action["dispatch"], dispatch_note = _activation_dispatch(
                    observation,
                    candidates[0],
                    "ax" if declared == "auto" else declared,
                )
    return action, mismatch, dispatch_note
