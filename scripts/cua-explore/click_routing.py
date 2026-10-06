"""How a click on a target with no accessibility action reaches the app."""

from __future__ import annotations

import dataclasses
from typing import Any, Mapping

from oracles import ActionEvidence, Finding
from ui_vocabulary import invocable_actions

# A refusal to deliver says something about the harness, not the product, so
# the note about it stays well below the confidence of a measured finding.
UNDELIVERED_NOTE_CONFIDENCE = 0.3
UNDELIVERED_ACTION_CONFIDENCE = 0.9


def label_carries_action(raw: Mapping[str, Any], label: str | None) -> bool | None:
    """True or False once the walk carries actions at all; None without one."""

    structured = raw.get("structuredContent")
    container = structured if isinstance(structured, dict) else raw
    elements = container.get("elements", [])
    matches = [
        item for item in elements if isinstance(item, dict) and item.get("label") == label
    ]
    if not any("actions" in item for item in matches):
        return None
    return any(invocable_actions(item.get("actions", ())) for item in matches)


def route_actionless_click(
    evidence: ActionEvidence,
    target: Mapping[str, Any] | None,
    target_has_action: bool | None,
    *,
    can_aim_pixels: bool,
) -> ActionEvidence:
    """Aim a click at the pointer when its target has nothing to invoke.

    A list row or a column header offers assistive technology no click, and
    cua-driver 0.33 refuses to aim at one by element. A user clicks it anyway,
    so the click goes by pixel and its effect is observed like any other. The
    returned evidence says so, which keeps the oracles from judging a pointer
    click by the accessibility rules. Without a frame to aim at, or without a
    window origin, the evidence is returned unchanged.
    """

    if (
        evidence.kind != "activate"
        or evidence.dispatch != "ax"
        or target_has_action is not False
        or not can_aim_pixels
        or not isinstance(target, Mapping)
        or not isinstance(target.get("frame"), Mapping)
    ):
        return evidence
    return dataclasses.replace(evidence, dispatch="px")


def undelivered_finding(
    evidence: ActionEvidence,
    response: Mapping[str, Any],
    target_has_action: bool | None,
) -> Finding:
    """Report an action whose delivery the driver's answer does not prove."""

    # A click the driver refused because nothing offers an action is a limit of
    # how the harness addressed the target: a person could click it.
    actionless = (
        evidence.kind == "activate"
        and evidence.dispatch == "ax"
        and target_has_action is False
    )
    detail: dict[str, Any] = {
        "kind": evidence.kind,
        "target": evidence.target_label,
        "dispatch": evidence.dispatch,
        "response": dict(response),
    }
    if actionless:
        detail["target_has_action"] = False
    return Finding(
        "driver-action-undelivered",
        "warning",
        UNDELIVERED_NOTE_CONFIDENCE if actionless else UNDELIVERED_ACTION_CONFIDENCE,
        "The driver accepted the action but its answer does not prove "
        "the input reached the app; no product verdict was drawn.",
        detail,
        blocks_gate=False,
    )
