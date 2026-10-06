#!/usr/bin/env python3
"""CUA snapshot/action executor with retained evidence and UX classification."""

from __future__ import annotations

import dataclasses
import json
import pathlib
import time
from dataclasses import dataclass
from typing import Any, Mapping, Sequence

from actions import (
    AcceptedAction,
    ActivateAction,
    ConnectivityAction,
    FinishAction,
    HoverAction,
    HotkeyAction,
    PressAction,
    ResizeAction,
    RestartAction,
    ScrollAction,
    TypeAction,
    WaitAction,
)
from click_routing import (
    label_carries_action,
    route_actionless_click,
    undelivered_finding,
)
from driver_transport import (
    RAW_INPUT_DELIVERY_MODE,
    CliTransport,
    DriverError,
    Transport,
    response_dispatched,
)
from hover_preflight import hover_preflight  # noqa: F401  (re-exported)
from oracles import ActionEvidence, Finding, OracleEngine, Snapshot, normalize_snapshot
from protocol import ContractError, SCHEMA_VERSION
from pointer_dispatch import desktop_pointer_payload
from process_activity import ProcessActivityProbe
from ui_vocabulary import (
    ACTIONABLE_ROLES,
    canonical_role,
    invocable_actions,
    is_entry,
)


def _target_order(item: Mapping[str, Any]) -> tuple[float, float, int]:
    frame = item.get("frame") if isinstance(item.get("frame"), dict) else {}
    def coordinate(name: str) -> float:
        value = frame.get(name)
        return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else float("inf")
    index = item.get("element_index")
    return coordinate("y"), coordinate("x"), index if isinstance(index, int) else 2**63


def snapshot_element_address(
    snapshot: Mapping[str, Any], target: Mapping[str, Any]
) -> Mapping[str, Any]:
    """Return an element handle bound to the snapshot that exposed it."""

    snapshot_id = snapshot.get("snapshot_id")
    token = target.get("element_token")
    if isinstance(token, str) and token:
        if not isinstance(snapshot_id, str) or not snapshot_id:
            raise DriverError(
                "fresh target's snapshot does not name itself, so its "
                "element_token cannot be proven current"
            )
        if token.partition(":")[0] != snapshot_id:
            raise DriverError(
                "fresh target element_token does not belong to its snapshot_id"
            )
        return {"element_token": token}
    index = target.get("element_index")
    if not isinstance(index, int):
        raise DriverError("fresh target has no element_token or element_index")
    if not isinstance(snapshot_id, str) or not snapshot_id:
        raise DriverError(
            "fresh target has no element_token and its snapshot has no snapshot_id"
        )
    return {"element_index": index, "snapshot_id": snapshot_id}


@dataclass(frozen=True)
class StepResult:
    before: Snapshot
    after: Snapshot
    settled: tuple[Snapshot, ...]
    action_response: Mapping[str, Any]
    evidence: ActionEvidence
    findings: tuple[Finding, ...]


class CuaExecutor:
    """Resolves every action from a fresh snapshot and verifies it afterward."""

    def __init__(
        self,
        transport: Transport,
        *,
        pid: int,
        window_id: int,
        session: str,
        state_prefix: str = "state",
        fixture_tokens: Mapping[str, str] | None = None,
        evidence_dir: pathlib.Path | None = None,
        settle_delays: Sequence[float] = (0.10, 0.25, 0.50),
        oracle_engine: OracleEngine | None = None,
        hover_geometry: Any | None = None,
        geometry_provider: Any | None = None,
        window_origin: Any | None = None,
        generation: int | None = None,
        geometry_measurements: list[dict[str, Any]] | None = None,
    ) -> None:
        self.transport = transport
        self.pid = pid
        self.activity_probe = ProcessActivityProbe(pid)
        self.window_id = window_id
        self.session = session
        self.state_prefix = state_prefix
        self.fixture_tokens = dict(fixture_tokens or {})
        self.evidence_dir = evidence_dir
        self.settle_delays = tuple(settle_delays)
        self.oracle_engine = oracle_engine or OracleEngine()
        self.hover_geometry = hover_geometry
        # Set from a measurement once per run; see measure_cursor_in_screenshot.
        self.exclude_cursor = True
        # The driver's own frames carry no usable position under X11/Xvfb, so
        # the geometry provider walks the accessibility tree for us.
        self.geometry_provider = geometry_provider
        self.window_origin = window_origin
        self.generation = generation
        self.geometry_measurements = geometry_measurements if geometry_measurements is not None else []
        self.geometry_failures: list[str] = []
        self.geometry_calibration: Any | None = None
        self.geometry_resolution: Any | None = None
        self._ambiguity_notes: dict[tuple[str, str], dict[str, Any]] = {}
        self._pending_findings: list[Finding] = []
        self._hover_cursor_disabled = False
        self._state_counter = 0
        self._step_counter = 0
        self._snapshot_durations_ms: list[int] = []
        self._snapshot_activity: list[Mapping[str, Any]] = []
        if evidence_dir is not None:
            evidence_dir.mkdir(parents=True, exist_ok=True)

    def observe(self) -> dict[str, Any]:
        _raw, state = self._snapshot("observe")
        return self._observation(state)

    def execute(self, action: AcceptedAction) -> StepResult:
        if isinstance(action, HoverAction):
            return self.execute_hover(action)
        if isinstance(action, ActivateAction):
            evidence = ActionEvidence.activate(
                action.target_label,
                dispatch=action.dispatch,
                expect_effect=action.expect_effect,
            )
        elif isinstance(action, TypeAction):
            evidence = ActionEvidence(
                kind=action.kind,
                target_label=action.target_label,
                dispatch=action.dispatch,
            )
        elif isinstance(action, (PressAction, HotkeyAction)):
            evidence = ActionEvidence(
                kind=action.kind,
                target_label=action.target_label,
            )
        elif isinstance(action, ScrollAction):
            evidence = ActionEvidence.scroll(
                action.direction,
                amount=action.amount,
                by=action.by,
                target_label=action.target_label,
            )
        elif isinstance(action, ResizeAction):
            evidence = ActionEvidence(kind=action.kind)
        elif isinstance(action, ConnectivityAction):
            evidence = ActionEvidence.connectivity(action.connectivity)
        elif isinstance(action, WaitAction):
            evidence = ActionEvidence(
                kind=action.kind,
                expect_effect="none",
                expect_status=action.expect_status,
            )
        elif isinstance(action, FinishAction):
            evidence = ActionEvidence(kind=action.kind, expect_effect="none")
        else:
            raise DriverError(f"runner-owned action cannot be executed: {action.kind}")
        return self._execute(action, evidence)

    def disable_agent_cursor(self) -> None:
        if self._hover_cursor_disabled:
            return
        self.transport.call(
            "set_agent_cursor_enabled",
            {"session": self.session, "enabled": False},
        )
        self._hover_cursor_disabled = True

    def execute_hover(self, action: HoverAction) -> StepResult:
        from hover_geometry import element_center, park_point
        from hover_oracle import HOVER_SETTLE_MS, analyze_hover

        if self.hover_geometry is None:
            raise DriverError("hover window geometry has not been resolved")
        self._step_counter += 1
        stem = f"step-{self._step_counter:04}-hover"
        park = park_point(self.hover_geometry)
        self.disable_agent_cursor()
        self._move_pointer(park)
        time.sleep(0.05)
        before_raw, before = self._snapshot(f"{stem}-before")
        target = self._target(before_raw, action.target_label)
        frame = target.get("frame")
        if not isinstance(frame, dict):
            raise DriverError("hover target has no frame")
        pointer = element_center(frame)
        started = time.monotonic()
        try:
            response = self._move_pointer(pointer)
            time.sleep(HOVER_SETTLE_MS / 1000)
            _after_raw, after = self._snapshot(f"{stem}-after")
        finally:
            self._move_pointer(park)
        elapsed_ms = round((time.monotonic() - started) * 1000)
        evidence = ActionEvidence(
            kind="hover",
            target_label=action.target_label,
            expect_effect="none",
            elapsed_ms=elapsed_ms,
            observation_ms=elapsed_ms,
        )
        findings = list(self.oracle_engine.analyze(evidence, before, after, settled=(after,)))
        findings.extend(self._take_harness_findings())
        if self.evidence_dir is None:
            hover_findings = analyze_hover(
                pathlib.Path("missing-hover-before.png"),
                pathlib.Path("missing-hover-after.png"),
                target,
                origin=self.hover_geometry,
                exclude_cursor=self.exclude_cursor,
                screenshots_available=before.screenshot_available and after.screenshot_available,
            )
        else:
            hover_findings = analyze_hover(
                self.evidence_dir / f"{stem}-before.png",
                self.evidence_dir / f"{stem}-after.png",
                target,
                origin=self.hover_geometry,
                exclude_cursor=self.exclude_cursor,
                screenshots_available=before.screenshot_available and after.screenshot_available,
            )
        findings.extend(hover_findings)
        result = StepResult(before, after, (after,), response, evidence, tuple(findings))
        self._retain_step(result)
        return result

    def _move_pointer(self, point: tuple[float, float]) -> Mapping[str, Any]:
        return self.transport.call(
            "move_cursor",
            desktop_pointer_payload(*point),
        )

    def execute_evidence(self, action: ActionEvidence) -> StepResult:
        return self._execute(None, action)

    def _execute(
        self, accepted: AcceptedAction | None, evidence: ActionEvidence
    ) -> StepResult:
        self._step_counter += 1
        before_raw, before = self._snapshot(f"step-{self._step_counter:04}-before")
        started = time.monotonic()
        # From here on every snapshot round-trip is harness cost inside the
        # observation window, so measure it from the same starting line.
        self._snapshot_durations_ms = []
        self._snapshot_activity = []
        requested = evidence
        evidence = self._route_click(requested, before_raw)
        rerouted = evidence is not requested
        response = self._dispatch(accepted, evidence, before_raw)
        if rerouted:
            response = {**response, "dispatch_rerouted": {"from": "ax", "to": "px"}}
        dispatched = self._confirm_dispatch(evidence, response, before_raw)
        action_elapsed_ms = round((time.monotonic() - started) * 1000)
        after_raw, after = self._snapshot(f"step-{self._step_counter:04}-after")
        first_change_ms = action_elapsed_ms if before.state_signature != after.state_signature else None
        # A change caught by the after-snapshot was seen at the first
        # opportunity, so nothing was blind before it.
        snapshot_ms_before_first_change = 0
        settled = [after]
        ax_probe_changed = False
        if (
            dispatched
            and not rerouted
            and evidence.kind == "activate"
            and evidence.dispatch == "px"
            and evidence.expect_effect == "required"
            and before.state_signature == after.state_signature
        ):
            target = self._target(after_raw, evidence.target_label)
            probe_response = self.transport.call(
                "click",
                {
                    "pid": self.pid,
                    "window_id": self.window_id,
                    "session": self.session,
                    **self._address(after_raw, target, "ax"),
                },
            )
            response = {**response, "ax_probe": probe_response}
            _probe_raw, probe = self._snapshot(
                f"step-{self._step_counter:04}-ax-probe"
            )
            # A probe the driver could not deliver proves nothing about the
            # target, whatever the snapshot after it looks like.
            ax_probe_changed = response_dispatched(probe_response) and (
                after.state_signature != probe.state_signature
            )
            settled.append(probe)
        sample_gaps = []
        for index, delay in enumerate(self.settle_delays, start=1):
            time.sleep(delay)
            sample_started = time.monotonic()
            _raw, sample = self._snapshot(f"step-{self._step_counter:04}-settled-{index}")
            now = time.monotonic()
            sample_gaps.append(round((now - sample_started) * 1000))
            if first_change_ms is None and before.state_signature != sample.state_signature:
                first_change_ms = round((now - started) * 1000)
                snapshot_ms_before_first_change = sum(self._snapshot_durations_ms)
            settled.append(sample)
        observation_ms = round((time.monotonic() - started) * 1000)
        effect = response.get("effect")
        visible_change = any(
            before.state_signature != sample.state_signature for sample in settled
        )
        if (
            dispatched
            and evidence.kind == "activate"
            and evidence.dispatch == "ax"
            and evidence.expect_effect == "required"
            and not visible_change
        ):
            effect = "suspected_noop"
        completed_evidence = dataclasses.replace(
            evidence,
            dispatched=dispatched,
            effect=str(effect) if effect is not None else None,
            ax_probe_changed=ax_probe_changed,
            elapsed_ms=action_elapsed_ms,
            observation_ms=observation_ms,
            first_change_ms=first_change_ms,
            sample_gaps_ms=tuple(sample_gaps),
            target_has_action=self.target_carries_action(before_raw, evidence.target_label) if evidence.target_label else None,
            settle_delay_ms=round(sum(self.settle_delays) * 1000),
            snapshot_ms=tuple(self._snapshot_durations_ms),
            snapshot_ms_before_first_change=snapshot_ms_before_first_change,
            response_gaps=tuple(self._snapshot_activity[-len(sample_gaps):]) if sample_gaps else (),
        )
        findings = list(self.oracle_engine.analyze(completed_evidence, before, after, settled=tuple(settled)))
        findings.extend(self._take_harness_findings())
        result = StepResult(before, after, tuple(settled), response, completed_evidence, tuple(findings))
        self._retain_step(result)
        return result

    def _confirm_dispatch(
        self,
        evidence: ActionEvidence,
        response: Mapping[str, Any],
        before_raw: Mapping[str, Any],
    ) -> bool:
        """Return whether the driver's answer proves the action was delivered.

        Until this ran, the mission path booked every response the transport
        did not reject as delivered. An undelivered action then looked exactly
        like a control that ignores its own accessibility action, and the app
        was blamed for it.
        """

        if response_dispatched(response):
            return True
        has_action = self.target_carries_action(before_raw, evidence.target_label)
        self._pending_findings.append(undelivered_finding(evidence, response, has_action))
        return False

    def _dispatch(
        self,
        accepted: AcceptedAction | None,
        evidence: ActionEvidence,
        before_raw: Mapping[str, Any],
    ) -> Mapping[str, Any]:
        base = {"pid": self.pid, "window_id": self.window_id, "session": self.session}
        if accepted is None:
            return self._dispatch_evidence(evidence, before_raw, base)
        if isinstance(accepted, ActivateAction):
            target = self._target(before_raw, evidence.target_label)
            return self.transport.call(
                "click", self._click_payload(base, before_raw, target, evidence.dispatch)
            )
        if isinstance(accepted, TypeAction):
            try:
                text = self.fixture_tokens[accepted.fixture_token]
            except KeyError as error:
                raise DriverError(f"missing trusted fixture token: {accepted.fixture_token}") from error
            target = self._target(before_raw, accepted.target_label)
            if accepted.dispatch == "ax" and not is_entry(str(target.get("role"))):
                # AT-SPI EditableText only exists on an entry. Aimed at anything
                # else (the "Search all fields" toggle that reveals the search
                # box) the driver falls back to key events and calls
                # Component.GrabFocus, which GTK4 does not implement. Focus the
                # target with a click, as a user does, and type into the
                # focused widget.
                focus_response = self._focus_click(base, before_raw, target)
                response = self.transport.call("type_text", {**base, "text": text})
                return {**response, "focus_click": focus_response}
            payload = {
                **base,
                **self._address(before_raw, target, accepted.dispatch),
                "text": text,
            }
            return self.transport.call("type_text", payload)
        if isinstance(accepted, PressAction):
            key_payload = {**base, "key": accepted.key}
            if accepted.target_label is None:
                return self.transport.call("press_key", key_payload)
            # press_key addressed to an element makes the driver call AT-SPI
            # Component.GrabFocus, which GTK4 accessibles do not implement
            # (NotSupported on a plain Gtk.SearchEntry and Gtk.Button too).
            # Focus the target the way a user does, with a click, and send the
            # key to the focused window without an element address.
            target = self._target(before_raw, accepted.target_label)
            focus_response = self._focus_click(base, before_raw, target)
            response = self.transport.call("press_key", key_payload)
            return {**response, "focus_click": focus_response}
        if isinstance(accepted, HotkeyAction):
            payload = {**base, "keys": list(accepted.keys)}
            return self.transport.call("hotkey", payload)
        if isinstance(accepted, ScrollAction):
            target_label = accepted.target_label
            payload = {
                **base,
                "direction": evidence.direction,
                "amount": evidence.amount,
                "by": evidence.by,
            }
            if target_label is not None:
                target = self._target(before_raw, target_label)
                payload.update(self._address(before_raw, target, "ax"))
            return self.transport.call("scroll", payload)
        if isinstance(accepted, ResizeAction):
            return self.transport.resize_window(self.window_id, accepted.width, accepted.height)
        if isinstance(accepted, ConnectivityAction):
            return self.transport.set_connectivity(accepted.connectivity)
        if isinstance(accepted, WaitAction):
            time.sleep(accepted.duration_ms / 1000)
            return {"effect": "confirmed", "verified": True}
        if isinstance(accepted, FinishAction):
            return {"effect": "confirmed", "verified": True}
        if isinstance(accepted, RestartAction):
            raise DriverError("restart is owned by the isolated process runner")
        raise DriverError(f"unsupported typed action: {accepted.kind}")

    def _dispatch_evidence(
        self,
        evidence: ActionEvidence,
        before_raw: Mapping[str, Any],
        base: Mapping[str, Any],
    ) -> Mapping[str, Any]:
        if evidence.kind == "activate":
            target = self._target(before_raw, evidence.target_label)
            return self.transport.call(
                "click", self._click_payload(base, before_raw, target, evidence.dispatch)
            )
        if evidence.kind == "scroll":
            payload = {
                **base,
                "direction": evidence.direction,
                "amount": evidence.amount,
                "by": evidence.by,
            }
            if evidence.target_label is not None:
                target = self._target(before_raw, evidence.target_label)
                payload.update(self._address(before_raw, target, "ax"))
            return self.transport.call("scroll", payload)
        if evidence.kind == "set-connectivity" and evidence.connectivity_state:
            return self.transport.set_connectivity(evidence.connectivity_state)
        raise DriverError(f"unsupported direct evidence action: {evidence.kind}")

    def _snapshot(self, stem: str) -> tuple[Mapping[str, Any], Snapshot]:
        self._state_counter += 1
        state_id = f"{self.state_prefix}-{self._state_counter}"
        payload: dict[str, Any] = {
            "pid": self.pid,
            "window_id": self.window_id,
            "session": self.session,
        }
        json_path = None
        if self.evidence_dir is not None:
            json_path = self.evidence_dir / f"{stem}.json"
            payload["screenshot_out_file"] = str(self.evidence_dir / f"{stem}.png")
        captured_ms = round(time.monotonic() * 1000)
        round_trip_started = time.monotonic()
        activity_started = self.activity_probe.start()
        raw = self.transport.call("get_window_state", payload)
        raw = {**raw, "screenshot_available": json_path is not None and raw.get("screenshot_available") is not False}
        raw = self.with_measured_geometry(raw, state_id=state_id)
        # Retained so the timing oracles can subtract the harness's own cost.
        duration_ms = round((time.monotonic() - round_trip_started) * 1000)
        self._snapshot_durations_ms.append(duration_ms)
        activity = self.activity_probe.finish(activity_started, gap_ms=duration_ms)
        if raw.get("screenshot_unavailable_reason"):
            activity = {**activity, "harness_fault": "screenshot-unavailable"}
        self._snapshot_activity.append(activity)
        if json_path is not None:
            json_path.write_text(json.dumps(raw, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        return raw, normalize_snapshot(raw, state_id=state_id, captured_ms=captured_ms)

    def with_measured_geometry(
        self, raw: Mapping[str, Any], *, state_id: str
    ) -> Mapping[str, Any]:
        """Replace the driver's placeholder positions with measured ones."""
        origin = self.window_origin or self.hover_geometry
        if self.geometry_provider is None or origin is None:
            return raw
        from atspi_geometry import GeometryError, resolve_driver_geometry

        structured = raw.get("structuredContent")
        container = structured if isinstance(structured, dict) else raw
        elements = container.get("elements")
        if not isinstance(elements, list):
            failure = "snapshot carries no element list"
            self.geometry_failures.append(failure)
            self._record_geometry(state_id, trusted=False, failure=failure)
            return self._untrusted(raw, failure)
        try:
            resolution = resolve_driver_geometry(
                elements, self.geometry_provider(), origin
            )
        except GeometryError as error:
            failure = str(error)
            self.geometry_failures.append(failure)
            self._record_geometry(state_id, trusted=False, failure=failure)
            return self._untrusted(raw, failure)
        self.geometry_calibration = resolution.calibration
        self.geometry_resolution = resolution.as_record()
        frames = resolution.frames
        if not resolution.trusted:
            failure = (
                "no element could be matched to a measured position "
                f"({resolution.driver_elements} driver elements, "
                f"{resolution.walk_nodes} walk nodes)"
            )
            self.geometry_failures.append(failure)
            self._record_geometry(
                state_id,
                trusted=False,
                failure=failure,
                resolution=self.geometry_resolution,
                calibration=self.geometry_calibration,
            )
            return self._untrusted(raw, "no element resolved")
        self._record_geometry(
            state_id,
            trusted=True,
            resolution=self.geometry_resolution,
            calibration=self.geometry_calibration,
        )
        rebuilt = []
        for index, element in enumerate(elements):
            if not isinstance(element, dict):
                rebuilt.append(element)
                continue
            rect = frames.get(int(element.get("element_index", index)))
            if rect is None:
                rebuilt.append({**element, "geometry_trusted": False})
                continue
            rebuilt.append(
                {
                    **element,
                    "geometry_trusted": True,
                    "actions": list(resolution.actions.get(
                        int(element.get("element_index", index)), ()
                    )),
                    "frame": {
                        "x": rect[0],
                        "y": rect[1],
                        "w": rect[2],
                        "h": rect[3],
                    },
                }
            )
        if container is raw:
            return {**raw, "elements": rebuilt, "geometry_trusted": True}
        return {
            **raw,
            "structuredContent": {**structured, "elements": rebuilt},
            "geometry_trusted": True,
        }

    def _record_geometry(
        self,
        state_id: str,
        *,
        trusted: bool,
        failure: str | None = None,
        resolution: Mapping[str, Any] | None = None,
        calibration: Mapping[str, Any] | None = None,
    ) -> None:
        record: dict[str, Any] = {
            "generation": self.generation,
            "state_id": state_id,
            "trusted": trusted,
        }
        if failure is not None:
            record["failure"] = failure
        if resolution is not None:
            record["resolution"] = dict(resolution)
        if calibration is not None:
            record["calibration"] = dict(calibration)
        self.geometry_measurements.append(record)

    @staticmethod
    def _untrusted(raw: Mapping[str, Any], note: str) -> Mapping[str, Any]:
        return {**raw, "geometry_trusted": False, "geometry_note": note}

    def _observation(self, state: Snapshot) -> dict[str, Any]:
        return {
            "schema_version": SCHEMA_VERSION,
            "state_id": state.state_id,
            "state_signature": state.raw_signature,
            "window": {"width": state.width, "height": state.height},
            "degraded": state.degraded,
            "geometry_trusted": state.geometry_trusted,
            "actionable_labels": list(state.actionable_labels),
            "elements": [
                {
                    "key": element.stable_key,
                    "label": element.label,
                    "role": element.role,
                    "enabled": element.enabled,
                    "visible": element.visible,
                    "focused": element.focused,
                    "selected": element.selected,
                    "value": element.value,
                    "actionable": element.actionable,
                    "geometry_trusted": element.geometry_trusted,
                    "frame": dataclasses.asdict(element.frame),
                }
                for element in state.elements
                if element.label
            ],
        }

    def observation_from_snapshot(self, state: Snapshot) -> dict[str, Any]:
        return self._observation(state)

    def _target(self, raw: Mapping[str, Any], label: str | None) -> Mapping[str, Any]:
        if label is None:
            raise ContractError("action target label is missing")
        structured = raw.get("structuredContent")
        if not isinstance(structured, dict):
            structured = {}
        elements = structured.get("elements", raw.get("elements", []))
        matches = [item for item in elements if isinstance(item, dict) and item.get("label") == label]
        if not matches:
            raise DriverError(f"fresh snapshot no longer exposes target: {label}")
        # The same label appears several times - a cell, a button and a toggle
        # button can all read "Add filter" - and only one of them carries the
        # AT-SPI action. Picking by role landed on a shell that never had one.
        carrying = [
            item for item in matches if invocable_actions(item.get("actions", ()))
        ]
        if len(carrying) == 1:
            return carrying[0]
        if len(carrying) > 1:
            carrying.sort(key=_target_order)
            chosen = carrying[0]
            role = canonical_role(str(chosen.get("role", "")))
            key = role, label
            if key not in self._ambiguity_notes:
                evidence = {
                    "target": label,
                    "role": role,
                    "count": len(carrying),
                    "chosen": dict(chosen.get("frame", {})),
                    "alternatives": [dict(item.get("frame", {})) for item in carrying[1:9]],
                }
                self._ambiguity_notes[key] = evidence
                self._pending_findings.append(
                    Finding(
                        "ambiguous-accessible-name", "warning", 0.8,
                        f"{len(carrying)} nodes share the accessible name '{label}'; "
                        "assistive technology cannot tell them apart.",
                        evidence, blocks_gate=False,
                    )
                )
            return chosen
        actionable = [
            item
            for item in matches
            if canonical_role(str(item.get("role", ""))) in ACTIONABLE_ROLES
        ]
        return actionable[0] if actionable else matches[0]

    target_carries_action = staticmethod(label_carries_action)

    def _take_harness_findings(self) -> list[Finding]:
        findings, self._pending_findings = self._pending_findings, []
        take_transport = getattr(self.transport, "take_findings", None)
        if callable(take_transport):
            findings.extend(take_transport())
        return findings

    def _route_click(
        self, evidence: ActionEvidence, raw: Mapping[str, Any]
    ) -> ActionEvidence:
        has_action = self.target_carries_action(raw, evidence.target_label)
        if evidence.kind != "activate" or has_action is not False:
            return evidence
        return route_actionless_click(
            evidence,
            self._target(raw, evidence.target_label),
            has_action,
            can_aim_pixels=(self.window_origin or self.hover_geometry) is not None,
        )

    def _focus_click(
        self,
        base: Mapping[str, Any],
        snapshot: Mapping[str, Any],
        target: Mapping[str, Any],
    ) -> Mapping[str, Any]:
        """Click the target to focus it before keys go to the focused window."""

        return self.transport.call(
            "click", {**base, **self._address(snapshot, target, "px")}
        )

    def _click_payload(
        self,
        base: Mapping[str, Any],
        snapshot: Mapping[str, Any],
        target: Mapping[str, Any],
        dispatch: str,
    ) -> dict[str, Any]:
        payload = {**base, **self._address(snapshot, target, dispatch)}
        if (
            dispatch == "ax"
            and "actions" in target
            and not invocable_actions(target.get("actions"))
        ):
            # Nothing to invoke over AT-SPI, so the driver falls to the pointer,
            # which only works in foreground on a private Xvfb. Asking for it
            # here keeps its 5 s background wait out of the measured action.
            payload["delivery_mode"] = RAW_INPUT_DELIVERY_MODE
        return payload

    def _address(
        self,
        snapshot: Mapping[str, Any],
        target: Mapping[str, Any],
        dispatch: str,
    ) -> Mapping[str, Any]:
        if dispatch == "ax":
            return snapshot_element_address(snapshot, target)
        frame = target.get("frame")
        if not isinstance(frame, dict):
            raise DriverError("pointer target has no frame")
        origin = self.window_origin or self.hover_geometry
        if origin is None:
            raise DriverError(
                "a pixel click needs the window origin: cua-driver takes x/y "
                "with window_id in window coordinates"
            )
        from hover_geometry import snapshot_frame_scale, window_pointer_point

        x, y = window_pointer_point(frame, origin, snapshot_frame_scale(snapshot))
        return {"x": x, "y": y}

    def _retain_step(self, result: StepResult) -> None:
        if self.evidence_dir is None:
            return
        payload = {
            "schema_version": SCHEMA_VERSION,
            "before": result.before.state_id,
            "after": result.after.state_id,
            "action": dataclasses.asdict(result.evidence),
            "action_response": result.action_response,
            "findings": [dataclasses.asdict(finding) for finding in result.findings],
        }
        path = self.evidence_dir / f"step-{self._step_counter:04}-result.json"
        path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
