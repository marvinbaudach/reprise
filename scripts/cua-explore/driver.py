#!/usr/bin/env python3
"""CUA snapshot/action executor with retained evidence and UX classification."""

from __future__ import annotations

import dataclasses
import json
import os
import pathlib
import subprocess
import time
from dataclasses import dataclass
from typing import Any, Mapping, Protocol, Sequence

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
from oracles import ActionEvidence, Finding, OracleEngine, Snapshot, normalize_snapshot
from protocol import ContractError, SCHEMA_VERSION
from ui_vocabulary import ACTIONABLE_ROLES, canonical_role


class DriverError(RuntimeError):
    """The native driver or target window could not honor the bounded action."""


class Transport(Protocol):
    def call(self, tool: str, payload: Mapping[str, Any]) -> Mapping[str, Any]: ...
    def resize_window(self, window_id: int, width: int, height: int) -> Mapping[str, Any]: ...
    def set_connectivity(self, state: str) -> Mapping[str, Any]: ...
    def wmctrl_geometry(self, window_id: int) -> Any: ...


class CliTransport:
    """No-shell adapter for cua-driver, wmctrl, and the test connectivity file."""

    def __init__(
        self,
        *,
        driver_binary: str = "cua-driver",
        socket_path: pathlib.Path | None = None,
        connectivity_file: pathlib.Path | None = None,
        timeout_seconds: int = 30,
    ) -> None:
        self.driver_binary = driver_binary
        self.socket_path = socket_path
        self.connectivity_file = connectivity_file
        self.timeout_seconds = timeout_seconds

    def call(self, tool: str, payload: Mapping[str, Any]) -> Mapping[str, Any]:
        command = [self.driver_binary, tool, json.dumps(payload, separators=(",", ":"))]
        if self.socket_path is not None:
            command.extend(["--socket", str(self.socket_path)])
        completed = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            timeout=self.timeout_seconds,
            env={**os.environ, "CUA_DRIVER_RS_UPDATE_CHECK": "0"},
        )
        if completed.returncode != 0:
            message = completed.stderr.strip() or completed.stdout.strip()
            raise DriverError(f"cua-driver {tool} failed: {message[:500]}")
        try:
            response = json.loads(completed.stdout)
        except json.JSONDecodeError:
            if completed.stdout.strip().startswith("✅"):
                return {"effect": "confirmed", "verified": True}
            raise DriverError(f"cua-driver {tool} returned invalid JSON")
        if not isinstance(response, dict):
            raise DriverError(f"cua-driver {tool} returned a non-object response")
        return response

    def resize_window(self, window_id: int, width: int, height: int) -> Mapping[str, Any]:
        completed = subprocess.run(
            ["wmctrl", "-i", "-r", f"0x{window_id:x}", "-e", f"0,-1,-1,{width},{height}"],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
        if completed.returncode != 0:
            raise DriverError(f"wmctrl resize failed: {completed.stderr.strip()[:300]}")
        return {"effect": "unverifiable", "verified": False}

    def set_connectivity(self, state: str) -> Mapping[str, Any]:
        if self.connectivity_file is None:
            raise DriverError("connectivity perturbation has no private control file")
        self.connectivity_file.write_text(state + "\n", encoding="utf-8")
        return {"effect": "confirmed", "verified": True}

    def wmctrl_geometry(self, window_id: int) -> Any:
        from hover_geometry import WindowGeometry

        completed = subprocess.run(
            ["wmctrl", "-lG"],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
        if completed.returncode != 0:
            raise DriverError(f"wmctrl geometry failed: {completed.stderr.strip()[:300]}")
        expected = f"0x{window_id:08x}".casefold()
        for line in completed.stdout.splitlines():
            fields = line.split(maxsplit=7)
            if len(fields) < 6 or fields[0].casefold() != expected:
                continue
            try:
                x, y, width, height = (int(value) for value in fields[2:6])
            except ValueError as error:
                raise DriverError("wmctrl returned invalid window geometry") from error
            return WindowGeometry(x, y, width, height)
        raise DriverError("wmctrl did not find the target window")


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
    ) -> None:
        self.transport = transport
        self.pid = pid
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
        self.geometry_failures: list[str] = []
        self.geometry_calibration: Any | None = None
        self.geometry_resolution: Any | None = None
        self._hover_cursor_disabled = False
        self._state_counter = 0
        self._step_counter = 0
        self._snapshot_durations_ms: list[int] = []
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
        findings = list(
            self.oracle_engine.analyze(evidence, before, after, settled=(after,))
        )
        if self.evidence_dir is None:
            hover_findings = analyze_hover(
                pathlib.Path("missing-hover-before.png"),
                pathlib.Path("missing-hover-after.png"),
                target,
                origin=self.hover_geometry,
                exclude_cursor=self.exclude_cursor,
            )
        else:
            hover_findings = analyze_hover(
                self.evidence_dir / f"{stem}-before.png",
                self.evidence_dir / f"{stem}-after.png",
                target,
                origin=self.hover_geometry,
                exclude_cursor=self.exclude_cursor,
            )
        findings.extend(hover_findings)
        result = StepResult(
            before=before,
            after=after,
            settled=(after,),
            action_response=response,
            evidence=evidence,
            findings=tuple(findings),
        )
        self._retain_step(result)
        return result

    def _move_pointer(self, point: tuple[float, float]) -> Mapping[str, Any]:
        return self.transport.call(
            "move_cursor",
            {
                "pid": self.pid,
                "window_id": self.window_id,
                "session": self.session,
                "scope": "desktop",
                "x": point[0],
                "y": point[1],
            },
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
        response = self._dispatch(accepted, evidence, before_raw)
        action_elapsed_ms = round((time.monotonic() - started) * 1000)
        after_raw, after = self._snapshot(f"step-{self._step_counter:04}-after")
        first_change_ms = action_elapsed_ms if before.state_signature != after.state_signature else None
        # A change caught by the after-snapshot was seen at the first
        # opportunity, so nothing was blind before it.
        snapshot_ms_before_first_change = 0
        settled = [after]
        ax_probe_changed = False
        if (
            evidence.kind == "activate"
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
                    **self._address(target, "ax"),
                },
            )
            response = {**response, "ax_probe": probe_response}
            _probe_raw, probe = self._snapshot(
                f"step-{self._step_counter:04}-ax-probe"
            )
            ax_probe_changed = after.state_signature != probe.state_signature
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
            evidence.kind == "activate"
            and evidence.dispatch == "ax"
            and evidence.expect_effect == "required"
            and not visible_change
        ):
            effect = "suspected_noop"
        completed_evidence = dataclasses.replace(
            evidence,
            effect=str(effect) if effect is not None else None,
            ax_probe_changed=ax_probe_changed,
            elapsed_ms=action_elapsed_ms,
            observation_ms=observation_ms,
            first_change_ms=first_change_ms,
            sample_gaps_ms=tuple(sample_gaps),
            settle_delay_ms=round(sum(self.settle_delays) * 1000),
            snapshot_ms=tuple(self._snapshot_durations_ms),
            snapshot_ms_before_first_change=snapshot_ms_before_first_change,
        )
        findings = self.oracle_engine.analyze(
            completed_evidence,
            before,
            after,
            settled=tuple(settled),
        )
        result = StepResult(
            before=before,
            after=after,
            settled=tuple(settled),
            action_response=response,
            evidence=completed_evidence,
            findings=tuple(findings),
        )
        self._retain_step(result)
        return result

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
            payload = {**base, **self._address(target, evidence.dispatch)}
            return self.transport.call("click", payload)
        if isinstance(accepted, TypeAction):
            try:
                text = self.fixture_tokens[accepted.fixture_token]
            except KeyError as error:
                raise DriverError(f"missing trusted fixture token: {accepted.fixture_token}") from error
            target = self._target(before_raw, accepted.target_label)
            payload = {**base, **self._address(target, accepted.dispatch), "text": text}
            return self.transport.call("type_text", payload)
        if isinstance(accepted, PressAction):
            payload = {**base, "key": accepted.key}
            if accepted.target_label is not None:
                payload.update(self._address(self._target(before_raw, accepted.target_label), "ax"))
            return self.transport.call("press_key", payload)
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
                payload.update(self._address(self._target(before_raw, target_label), "ax"))
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
                "click", {**base, **self._address(target, evidence.dispatch)}
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
                payload.update(self._address(target, "ax"))
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
        raw = self.transport.call("get_window_state", payload)
        raw = self.with_measured_geometry(raw)
        # Retained so the timing oracles can subtract the harness's own cost.
        self._snapshot_durations_ms.append(
            round((time.monotonic() - round_trip_started) * 1000)
        )
        if json_path is not None:
            json_path.write_text(
                json.dumps(raw, indent=2, sort_keys=True) + "\n", encoding="utf-8"
            )
        return raw, normalize_snapshot(raw, state_id=state_id, captured_ms=captured_ms)

    def with_measured_geometry(self, raw: Mapping[str, Any]) -> Mapping[str, Any]:
        """Replace the driver's placeholder positions with measured ones."""
        origin = self.window_origin or self.hover_geometry
        if self.geometry_provider is None or origin is None:
            return raw
        from atspi_geometry import GeometryError, resolve_driver_geometry

        structured = raw.get("structuredContent")
        container = structured if isinstance(structured, dict) else raw
        elements = container.get("elements")
        if not isinstance(elements, list):
            return self._untrusted(raw, "snapshot carries no element list")
        try:
            resolution = resolve_driver_geometry(
                elements, self.geometry_provider(), origin
            )
        except GeometryError as error:
            self.geometry_failures.append(str(error))
            return self._untrusted(raw, str(error))
        self.geometry_calibration = resolution.calibration
        self.geometry_resolution = resolution.as_record()
        frames = resolution.frames
        if not resolution.trusted:
            self.geometry_failures.append(
                "no element could be matched to a measured position "
                f"({resolution.driver_elements} driver elements, "
                f"{resolution.walk_nodes} walk nodes)"
            )
            return self._untrusted(raw, "no element resolved")
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
        actionable = [
            item
            for item in matches
            if item.get("actions")
            or canonical_role(str(item.get("role", ""))) in ACTIONABLE_ROLES
        ]
        return actionable[0] if actionable else matches[0]

    def _address(self, target: Mapping[str, Any], dispatch: str) -> Mapping[str, Any]:
        if dispatch == "ax":
            index = target.get("element_index")
            if not isinstance(index, int):
                raise DriverError("fresh target has no element_index")
            return {"element_index": index}
        frame = target.get("frame")
        if not isinstance(frame, dict):
            raise DriverError("pointer target has no frame")
        x = frame.get("x")
        y = frame.get("y")
        width = frame.get("w", frame.get("width"))
        height = frame.get("h", frame.get("height"))
        if not all(isinstance(value, (int, float)) for value in (x, y, width, height)):
            raise DriverError("pointer target has incomplete geometry")
        if width <= 0 or height <= 0:
            raise DriverError("pointer target has non-positive geometry")
        return {"x": x + width / 2, "y": y + height / 2}

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


def hover_preflight(
    transport: Transport,
    *,
    pid: int,
    window_id: int,
    session: str,
    origin: Any,
) -> dict[str, Any]:
    """Verify desktop-pointer dispatch before spending a mission action budget."""
    try:
        transport.call(
            "move_cursor",
            {
                "pid": pid,
                "window_id": window_id,
                "session": session,
                "scope": "desktop",
                "x": origin.x + origin.width / 2,
                "y": origin.y + origin.height / 2,
            },
        )
        cursor = transport.call(
            "get_cursor_position", {"pid": pid, "window_id": window_id, "session": session}
        )
        window = transport.call(
            "get_window_state", {"pid": pid, "window_id": window_id, "session": session}
        )
    except (DriverError, OSError, subprocess.SubprocessError) as error:
        raise DriverError("hover dispatch is unsafe on this driver build") from error
    return {"cursor_position": cursor, "window_state": window}
