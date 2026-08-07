#!/usr/bin/env python3
"""Pure UX anomaly oracles over CUA before/action/after evidence."""

from __future__ import annotations

import hashlib
import json
import statistics
from dataclasses import dataclass, field
from typing import Any, Iterable, Mapping, Sequence

from ui_vocabulary import (
    ACTIONABLE_ROLES,
    BUSY_ROLES,
    BUSY_WORDS,
    CANONICAL_ROW_ROLE,
    OFFLINE_WORDS,
    WINDOW_ROLES,
    canonical_role,
)


GEOMETRY_EPSILON_PX = 6.0

# All three thresholds are stated on app-attributable time - wall time minus
# the harness's own sleeps and get_window_state round-trips. 250 ms is where a
# pause stops reading as instantaneous and starts reading as a hiccup, so it
# serves both for late feedback and for a sampling gap that overshoots the
# step's own baseline. 750 ms is the point at which an operation owes the user
# a visible waiting state rather than silence.
SLOW_FEEDBACK_MS = 250
STALL_EXCESS_MS = 250
SILENT_WAIT_MS = 750


def element_flag(element: Mapping[str, Any], key: str, default: bool = False) -> bool:
    """Read a boolean element state from whatever shape the driver reports.

    A direct boolean wins. Otherwise a non-empty states list decides by
    membership. If the driver sends neither - the current cua-driver sends no
    'visible' key and no 'states' list at all - the declared default applies.
    Silently ignoring the default here made every element count as invisible.
    """
    direct = element.get(key)
    if isinstance(direct, bool):
        return direct
    states = element.get("states")
    if isinstance(states, list) and states:
        return key in states
    return default


def _number(value: Any, default: float = 0.0) -> float:
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else default


@dataclass(frozen=True)
class Frame:
    x: float
    y: float
    width: float
    height: float

    @property
    def center(self) -> tuple[float, float]:
        return self.x + self.width / 2, self.y + self.height / 2

    @property
    def has_area(self) -> bool:
        return self.width > 0 and self.height > 0

    def intersects_rect(self, other: "Frame") -> bool:
        """Both rectangles must be expressed in the same coordinate system."""
        return (
            self.has_area
            and other.has_area
            and self.x < other.x + other.width
            and self.y < other.y + other.height
            and self.x + self.width > other.x
            and self.y + self.height > other.y
        )


@dataclass(frozen=True)
class Element:
    stable_key: str
    label: str
    role: str
    frame: Frame
    actions: tuple[str, ...]
    enabled: bool
    visible: bool
    focused: bool
    selected: bool
    value: str

    @property
    def actionable(self) -> bool:
        return bool(self.actions) or self.role in ACTIONABLE_ROLES


@dataclass(frozen=True)
class Snapshot:
    state_id: str
    captured_ms: int
    width: float
    height: float
    elements: tuple[Element, ...]
    degraded: bool
    raw_signature: str
    window_frame: Frame | None = None

    @property
    def viewport(self) -> Frame | None:
        """The window rectangle element frames are measured against.

        Element frames are screen coordinates, so the visible region is the
        window's own frame - not the screenshot size, which is the window crop
        anchored at (0, 0) and is missing entirely from the settling probes.
        Without it there is nothing to judge against, and the oracle stays quiet.
        """
        if self.window_frame is not None and self.window_frame.has_area:
            return self.window_frame
        return None

    @property
    def by_key(self) -> Mapping[str, Element]:
        return {element.stable_key: element for element in self.elements}

    @property
    def actionable_labels(self) -> tuple[str, ...]:
        return tuple(
            sorted({element.label for element in self.elements if element.actionable and element.label})
        )

    @property
    def state_signature(self) -> tuple[tuple[Any, ...], ...]:
        return tuple(
            sorted(
                (
                    element.stable_key,
                    element.value,
                    element.enabled,
                    element.visible,
                    element.focused,
                    element.selected,
                )
                for element in self.elements
            )
        )


@dataclass(frozen=True)
class ActionEvidence:
    kind: str
    target_label: str | None = None
    dispatch: str = "ax"
    effect: str | None = None
    expect_effect: str = "required"
    elapsed_ms: int = 0
    observation_ms: int = 0
    first_change_ms: int | None = 0
    expect_status: bool = False
    ax_probe_changed: bool = False
    direction: str | None = None
    amount: int = 1
    by: str = "page"
    connectivity_state: str | None = None
    sample_gaps_ms: tuple[int, ...] = ()
    # Harness cost, so the timing oracles can subtract themselves out.
    settle_delay_ms: int = 0
    snapshot_ms: tuple[int, ...] = ()
    snapshot_ms_before_first_change: int = 0

    @classmethod
    def activate(cls, target_label: str, **kwargs: Any) -> "ActionEvidence":
        return cls(kind="activate", target_label=target_label, **kwargs)

    @classmethod
    def scroll(cls, direction: str, **kwargs: Any) -> "ActionEvidence":
        return cls(kind="scroll", direction=direction, **kwargs)

    @classmethod
    def connectivity(cls, connectivity: str, **kwargs: Any) -> "ActionEvidence":
        return cls(kind="set-connectivity", connectivity_state=connectivity, **kwargs)


@dataclass(frozen=True)
class Finding:
    code: str
    severity: str
    confidence: float
    summary: str
    evidence: Mapping[str, Any] = field(default_factory=dict)
    blocks_gate: bool = False


def _root_window_frame(
    candidates: Sequence[tuple[Mapping[str, Any], str, Frame]]
) -> Frame | None:
    """Pick the depth-0 root, the frame every other element is measured against."""
    usable = [item for item in candidates if item[2].has_area]
    if not usable:
        return None
    depths = [
        (raw.get("depth"), frame)
        for raw, _role, frame in usable
        if isinstance(raw.get("depth"), int) and not isinstance(raw.get("depth"), bool)
    ]
    if depths:
        return min(depths, key=lambda item: item[0])[1]
    parentless = [
        frame
        for raw, _role, frame in usable
        if "parent_index" in raw and raw.get("parent_index") is None
    ]
    if parentless:
        return parentless[0]
    windows = [frame for _raw, role, frame in usable if role in WINDOW_ROLES]
    if windows:
        return max(windows, key=lambda frame: frame.width * frame.height)
    return None


def normalize_snapshot(
    raw: Mapping[str, Any], *, state_id: str, captured_ms: int
) -> Snapshot:
    structured = raw.get("structuredContent")
    if not isinstance(structured, dict):
        structured = {}
    raw_elements = structured.get("elements", raw.get("elements", []))
    if not isinstance(raw_elements, list):
        raw_elements = []
    sortable = []
    root_candidates: list[tuple[Mapping[str, Any], str, Frame]] = []
    for raw_element in raw_elements:
        if not isinstance(raw_element, dict):
            continue
        frame_raw = raw_element.get("frame", {})
        if not isinstance(frame_raw, dict):
            frame_raw = {}
        label = str(raw_element.get("label") or "")
        role = canonical_role(str(raw_element.get("role") or "unknown"))
        frame = Frame(
            _number(frame_raw.get("x")),
            _number(frame_raw.get("y")),
            _number(frame_raw.get("w", frame_raw.get("width"))),
            _number(frame_raw.get("h", frame_raw.get("height"))),
        )
        sortable.append((label, role, frame.x, frame.y, raw_element, frame))
        root_candidates.append((raw_element, role, frame))
    sortable.sort(key=lambda item: item[:4])
    occurrences: dict[tuple[str, str], int] = {}
    elements = []
    for label, role, _x, _y, raw_element, frame in sortable:
        identity = label, role
        occurrence = occurrences.get(identity, 0)
        occurrences[identity] = occurrence + 1
        actions_raw = raw_element.get("actions", [])
        actions = tuple(str(action).lower() for action in actions_raw) if isinstance(actions_raw, list) else ()
        value_raw = raw_element.get("value")
        elements.append(
            Element(
                stable_key=f"{role}|{label}|{occurrence}",
                label=label,
                role=role,
                frame=frame,
                actions=actions,
                enabled=element_flag(raw_element, "enabled", True),
                visible=element_flag(raw_element, "visible", True),
                focused=element_flag(raw_element, "focused"),
                selected=element_flag(raw_element, "selected"),
                value="" if value_raw is None else str(value_raw),
            )
        )
    signature_payload = json.dumps(raw, sort_keys=True, default=str).encode("utf-8")
    return Snapshot(
        state_id=state_id,
        captured_ms=captured_ms,
        width=_number(raw.get("screenshot_width")),
        height=_number(raw.get("screenshot_height")),
        elements=tuple(elements),
        degraded=raw.get("degraded") is True,
        raw_signature=hashlib.sha256(signature_payload).hexdigest(),
        window_frame=_root_window_frame(root_candidates),
    )


class OracleEngine:
    """Classifies observable anomalies without turning heuristics into CI gates."""

    def inspect_snapshot(self, snapshot: Snapshot) -> list[Finding]:
        findings = []
        if snapshot.degraded:
            findings.append(
                Finding(
                    "degraded-accessibility",
                    "error",
                    1.0,
                    "The accessibility tree is degraded.",
                    blocks_gate=True,
                )
            )
        viewport = snapshot.viewport
        if viewport is None:
            # No window rectangle, no verdict. Judging visibility without one is
            # how a run of 79 snapshots produced 981 invented errors.
            return findings
        for element in snapshot.elements:
            if not element.enabled or not element.actionable:
                continue
            if not element.visible or not element.frame.intersects_rect(viewport):
                findings.append(
                    Finding(
                        "invisible-actionable",
                        "error",
                        0.95,
                        f"Enabled action '{element.label or element.role}' is outside the visible window.",
                        {"element": element.stable_key},
                        blocks_gate=True,
                    )
                )
        return findings

    def analyze(
        self,
        action: ActionEvidence,
        before: Snapshot,
        after: Snapshot,
        *,
        settled: Sequence[Snapshot] = (),
    ) -> list[Finding]:
        findings = self.inspect_snapshot(after)
        changed = before.state_signature != after.state_signature
        if action.kind == "activate":
            findings.extend(self._click_findings(action, before, after, changed))
        if action.kind == "scroll":
            findings.extend(self._scroll_findings(action, before, after))
        projected = settled[-1] if settled else after
        if action.kind == "set-connectivity" and action.connectivity_state == "offline":
            if "Music" in before.actionable_labels and "Music" not in projected.actionable_labels:
                findings.append(
                    Finding(
                        "offline-broke-local-music",
                        "error",
                        0.98,
                        "Going offline made the local Music surface unreachable.",
                        blocks_gate=True,
                    )
                )
            cached_before = {
                element.label
                for element in before.elements
                if element.role == CANONICAL_ROW_ROLE and element.label
            }
            cached_after = {
                element.label
                for element in projected.elements
                if element.role == CANONICAL_ROW_ROLE and element.label
            }
            lost_cached = sorted(cached_before - cached_after)
            if lost_cached:
                findings.append(
                    Finding(
                        "offline-lost-cached-content",
                        "error",
                        0.9,
                        "Going offline removed cached rows from the active source.",
                        {"lost_rows": lost_cached},
                        blocks_gate=True,
                    )
                )
        if action.kind == "set-connectivity" and action.connectivity_state == "online":
            if self._has_offline_status(projected):
                findings.append(
                    Finding(
                        "reconnect-kept-offline-status",
                        "warning",
                        0.8,
                        "The settled view still presents an offline-authored status after reconnect.",
                    )
                )
        findings.extend(self._timing_findings(action, (after, *settled)))
        if action.kind == "wait" and settled:
            findings.extend(self._layout_findings((before, *settled)))
        return self._deduplicate(findings)

    def _click_findings(
        self, action: ActionEvidence, before: Snapshot, after: Snapshot, changed: bool
    ) -> list[Finding]:
        if changed or action.expect_effect in {"idempotent", "none"}:
            return self._misroute_findings(action, before, after)
        if action.dispatch == "px" and action.ax_probe_changed:
            return [
                Finding(
                    "suspected-occlusion",
                    "error",
                    0.9,
                    f"'{action.target_label}' worked through accessibility but not at its visible pointer target.",
                    {"target": action.target_label},
                    blocks_gate=True,
                )
            ]
        if action.dispatch == "ax" and action.effect == "suspected_noop":
            return [
                Finding(
                    "suspected-no-handler",
                    "error",
                    0.9,
                    f"'{action.target_label}' advertises interaction but produced no observable effect.",
                    {"target": action.target_label},
                    blocks_gate=True,
                )
            ]
        return [
            Finding(
                "click-no-visible-effect",
                "warning",
                0.75,
                f"Clicking '{action.target_label}' produced no visible state change.",
                {"target": action.target_label, "driver_effect": action.effect},
            )
        ]

    def _misroute_findings(
        self, action: ActionEvidence, before: Snapshot, after: Snapshot
    ) -> list[Finding]:
        def active_labels(snapshot: Snapshot) -> set[str]:
            return {
                element.label
                for element in snapshot.elements
                if element.label and (element.focused or element.selected)
            }

        newly_active = active_labels(after) - active_labels(before)
        if newly_active and action.target_label not in newly_active:
            return [
                Finding(
                    "misrouted-click",
                    "error",
                    0.85,
                    f"Click intended for '{action.target_label}' activated {sorted(newly_active)}.",
                    {"target": action.target_label, "activated": sorted(newly_active)},
                    blocks_gate=True,
                )
            ]
        return []

    def _scroll_findings(
        self, action: ActionEvidence, before: Snapshot, after: Snapshot
    ) -> list[Finding]:
        before_rows = {
            key: value
            for key, value in before.by_key.items()
            if value.role == CANONICAL_ROW_ROLE
        }
        after_rows = {
            key: value
            for key, value in after.by_key.items()
            if value.role == CANONICAL_ROW_ROLE
        }
        shared = sorted(set(before_rows) & set(after_rows))
        findings = []
        if shared and action.direction in {"up", "down"}:
            delta = statistics.median(
                after_rows[key].frame.y - before_rows[key].frame.y for key in shared
            )
            wrong = action.direction == "down" and delta > GEOMETRY_EPSILON_PX
            wrong = wrong or action.direction == "up" and delta < -GEOMETRY_EPSILON_PX
            if wrong:
                findings.append(
                    Finding(
                        "wrong-scroll-direction",
                        "error",
                        0.98,
                        f"A {action.direction} scroll moved shared rows in the opposite direction.",
                        {"median_row_delta_px": delta},
                        blocks_gate=True,
                    )
                )
            if abs(delta) > max(before.height * 1.75, 900):
                findings.append(
                    Finding(
                        "scroll-jump",
                        "warning",
                        0.8,
                        "One bounded scroll moved the viewport by more than 1.75 window heights.",
                        {"median_row_delta_px": delta},
                    )
                )
        selected_before = {item.label for item in before.elements if item.selected and item.label}
        selected_after = {item.label for item in after.elements if item.selected and item.label}
        if selected_before and not selected_before.issubset(selected_after):
            findings.append(
                Finding(
                    "scroll-lost-selection",
                    "error",
                    0.95,
                    "Scrolling changed or cleared the user's selection.",
                    {"before": sorted(selected_before), "after": sorted(selected_after)},
                    blocks_gate=True,
                )
            )
        return findings

    def _timing_findings(
        self, action: ActionEvidence, timeline: Sequence[Snapshot]
    ) -> list[Finding]:
        findings = []
        # Every raw number here is wall time that contains the harness itself:
        # one get_window_state is a subprocess round-trip (spawn, tree walk,
        # PNG) costing hundreds of milliseconds, and the settle schedule sleeps
        # on purpose. Judged raw, all three oracles fired on every single step.
        app_first_change_ms = (
            None
            if action.first_change_ms is None
            else max(
                0, action.first_change_ms - action.snapshot_ms_before_first_change
            )
        )
        if app_first_change_ms is not None and app_first_change_ms > SLOW_FEEDBACK_MS:
            findings.append(
                Finding(
                    "slow-visible-feedback",
                    "warning",
                    0.7,
                    "Visible feedback arrived unusually late; timing remains a manual UX judgement.",
                    {
                        "app_first_change_ms": app_first_change_ms,
                        "first_change_ms": action.first_change_ms,
                        "blind_snapshot_ms": action.snapshot_ms_before_first_change,
                    },
                )
            )
        waiting_visible = any(self._has_waiting_state(snapshot) for snapshot in timeline)
        harness_ms = action.settle_delay_ms + sum(action.snapshot_ms)
        app_observation_ms = max(0, action.observation_ms - harness_ms)
        waited_without_feedback = (
            action.expect_effect == "required"
            and action.first_change_ms is None
            and app_observation_ms >= SILENT_WAIT_MS
        )
        if (action.expect_status or waited_without_feedback) and not waiting_visible:
            findings.append(
                Finding(
                    "missing-waiting-feedback",
                    "warning",
                    0.85,
                    "The operation took noticeable time without exposing progress or a waiting state.",
                    {
                        "dispatch_ms": action.elapsed_ms,
                        "app_observation_ms": app_observation_ms,
                        "observation_ms": action.observation_ms,
                        "harness_ms": harness_ms,
                        "status_expected": action.expect_status,
                    },
                )
            )
        # The cheapest snapshot of this same step is what a round-trip costs
        # when the UI thread is free; anything a sample spends beyond that is
        # time the main loop kept the accessibility bus waiting.
        baseline_ms = min(action.snapshot_ms) if action.snapshot_ms else 0
        excess_ms = [
            round(gap - baseline_ms)
            for gap in action.sample_gaps_ms
            if gap - baseline_ms >= STALL_EXCESS_MS
        ]
        if excess_ms:
            findings.append(
                Finding(
                    "main-loop-stall",
                    "warning",
                    0.8,
                    "Observation sampling detected one or more long UI response gaps.",
                    {
                        "excess_ms": excess_ms,
                        "baseline_ms": round(baseline_ms),
                        "gaps_ms": list(action.sample_gaps_ms),
                    },
                )
            )
        return findings

    def _has_waiting_state(self, snapshot: Snapshot) -> bool:
        for element in snapshot.elements:
            text = f"{element.role} {element.label} {element.value}".lower()
            if element.role in BUSY_ROLES or any(word in text for word in BUSY_WORDS):
                return True
        return False

    def _has_offline_status(self, snapshot: Snapshot) -> bool:
        return any(
            any(word in f"{element.label} {element.value}".casefold() for word in OFFLINE_WORDS)
            for element in snapshot.elements
        )

    def _layout_findings(self, timeline: Sequence[Snapshot]) -> list[Finding]:
        shifted: dict[str, float] = {}
        for earlier, later in zip(timeline, timeline[1:]):
            earlier_by_key = earlier.by_key
            later_by_key = later.by_key
            for key in set(earlier_by_key) & set(later_by_key):
                left = earlier_by_key[key]
                right = later_by_key[key]
                if left.role in BUSY_ROLES:
                    continue
                lx, ly = left.frame.center
                rx, ry = right.frame.center
                distance = ((rx - lx) ** 2 + (ry - ly) ** 2) ** 0.5
                if distance > GEOMETRY_EPSILON_PX:
                    shifted[key] = max(shifted.get(key, 0.0), distance)
        if not shifted:
            return []
        return [
            Finding(
                "uninvited-layout-shift",
                "warning",
                0.75,
                "Stable visible elements moved after the direct interaction had already landed.",
                {
                    "elements": [
                        {"key": key, "distance_px": round(distance, 2)}
                        for key, distance in sorted(shifted.items())
                    ]
                },
            )
        ]

    def _deduplicate(self, findings: Iterable[Finding]) -> list[Finding]:
        unique = {}
        for finding in findings:
            key = finding.code, json.dumps(finding.evidence, sort_keys=True)
            unique[key] = finding
        return list(unique.values())
