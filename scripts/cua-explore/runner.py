#!/usr/bin/env python3
"""Run one mission inside an already isolated X11/D-Bus/AT-SPI session."""

from __future__ import annotations

import argparse
import contextlib
import dataclasses
import json
import os
import pathlib
import re
import subprocess
import sys
import time
from typing import Any, Iterator, Mapping, Sequence

from actions import (
    CompleteWorkloadAction,
    FinishAction,
    RestartAction,
    action_to_dict,
)
from agent_adapter import ExternalAgent
from driver import CliTransport, CuaExecutor, DriverError, hover_preflight
from hover_geometry import WindowGeometry, resolve_window_origin
from explorer import DeterministicExplorer
from fixtures import FixtureError, audit_batch_edit
from protocol import ActionGateway, ContractError, Mission, load_mission
from report import RunReport
from ui_vocabulary import BUSY_ROLES, BUSY_WORDS, is_row
from workload_audit import ActionTrace, audit_action_workload


FAILURE_LOG_PATTERN = re.compile(
    r"Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|"
    r"BorrowError|BorrowMutError|already borrowed",
    re.IGNORECASE,
)

APP_NAMESPACE_ARGV: tuple[str, ...] = (
    "unshare",
    "--user",
    "--map-current-user",
    "--net",
    "--",
)


def app_launch_argv(app_binary: pathlib.Path) -> list[str]:
    """Use a private network namespace without breaking D-Bus EXTERNAL auth."""
    return [*APP_NAMESPACE_ARGV, str(app_binary)]


def write_gtk_animation_settings(profile_root: pathlib.Path, mode: str) -> pathlib.Path | None:
    """Set GTK animations only inside the disposable profile."""
    if mode == "on":
        return None
    if mode != "off":
        raise RunError("--gtk-animations must be on or off")
    path = profile_root / "config" / "gtk-4.0" / "settings.ini"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("[Settings]\ngtk-enable-animations=0\n", encoding="utf-8")
    return path


def parse_window_origin(value: str | None) -> tuple[int, int] | None:
    if value is None:
        return None
    try:
        x_text, y_text = value.split(",", maxsplit=1)
        return int(x_text), int(y_text)
    except (TypeError, ValueError) as error:
        raise RunError("--window-origin must be X,Y") from error


def prepare_hover(
    transport: CliTransport,
    *,
    pid: int,
    window_id: int,
    session: str,
    evidence_dir: pathlib.Path,
    window: Mapping[str, Any],
    origin_override: tuple[int, int] | None = None,
) -> WindowGeometry:
    if origin_override is None:
        geometry = resolve_window_origin(transport, pid=pid, window_id=window_id)
    else:
        width = window.get("width")
        height = window.get("height")
        if not isinstance(width, int) or not isinstance(height, int):
            raise RunError("hover window dimensions are unavailable")
        geometry = WindowGeometry(*origin_override, width, height)
    evidence = hover_preflight(
        transport,
        pid=pid,
        window_id=window_id,
        session=session,
        origin=geometry,
    )
    path = evidence_dir / "hover-preflight.json"
    path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return geometry


class RunError(RuntimeError):
    """The isolated runner could not establish trustworthy evidence."""


class HoverSmokeComplete(RuntimeError):
    """Internal control flow after a successful preflight-only run."""


def _private_environment_required() -> None:
    required = {
        "GDK_BACKEND": "x11",
        "WAYLAND_DISPLAY": "",
        "REPRISE_AUDIO_SINK": "fakesink",
    }
    for name, expected in required.items():
        if os.environ.get(name) != expected:
            raise RunError(f"private runner requires {name}={expected!r}")
    for name in ("DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR"):
        if not os.environ.get(name):
            raise RunError(f"private runner requires isolated {name}")


def _walk_objects(value: Any) -> Iterator[Mapping[str, Any]]:
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _walk_objects(child)
    elif isinstance(value, list):
        for child in value:
            yield from _walk_objects(child)


def _window_id(response: Mapping[str, Any]) -> int | None:
    for item in _walk_objects(response):
        candidate = item.get("window_id")
        title = " ".join(
            str(item.get(key, "")) for key in ("title", "class", "wm_class")
        ).casefold()
        if isinstance(candidate, int) and "reprise" in title:
            return candidate
    return None


class AppLifecycle:
    def __init__(
        self,
        *,
        app_binary: pathlib.Path,
        profile_root: pathlib.Path,
        evidence_dir: pathlib.Path,
        connectivity_file: pathlib.Path,
        quit_delay_seconds: int,
        transport: CliTransport,
    ) -> None:
        self.app_binary = app_binary
        self.profile_root = profile_root
        self.evidence_dir = evidence_dir
        self.connectivity_file = connectivity_file
        self.quit_delay_seconds = quit_delay_seconds
        self.transport = transport
        self.process: subprocess.Popen[str] | None = None
        self.log_handle = None
        self.launch_count = 0
        self.log_paths: list[pathlib.Path] = []

    def start(self) -> tuple[int, int, int]:
        if self.process is not None:
            raise RunError("application is already running")
        self.launch_count += 1
        log_path = self.evidence_dir / f"app-{self.launch_count}.log"
        self.log_paths.append(log_path)
        self.log_handle = log_path.open("w", encoding="utf-8")
        environment = {
            **os.environ,
            "XDG_DATA_HOME": str(self.profile_root / "data"),
            "XDG_CACHE_HOME": str(self.profile_root / "cache"),
            "XDG_CONFIG_HOME": str(self.profile_root / "config"),
            "GDK_BACKEND": "x11",
            "WAYLAND_DISPLAY": "",
            "GTK_A11Y": "atspi",
            "NO_AT_BRIDGE": "0",
            "REPRISE_AUDIO_SINK": "fakesink",
            "REPRISE_SMOKE_QUIT": "1",
            "REPRISE_SMOKE_QUIT_DELAY_SECS": str(self.quit_delay_seconds),
            "REPRISE_TEST_CONNECTIVITY_FILE": str(self.connectivity_file),
            "REPRISE_LOG": "debug",
        }
        self.process = subprocess.Popen(
            app_launch_argv(self.app_binary),
            stdout=self.log_handle,
            stderr=subprocess.STDOUT,
            env=environment,
            text=True,
        )
        window_id = self._wait_for_window(self.process.pid)
        return self.process.pid, window_id, self.launch_count

    def restart(self) -> tuple[int, int, int]:
        self.stop()
        return self.start()

    def stop(self) -> None:
        if self.process is not None:
            process = self.process
            self.process = None
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        if self.log_handle is not None:
            self.log_handle.close()
            self.log_handle = None

    def assert_clean_logs(self) -> None:
        for path in self.log_paths:
            contents = path.read_text(encoding="utf-8", errors="replace")
            match = FAILURE_LOG_PATTERN.search(contents)
            if match:
                raise RunError(f"application log contains runtime failure marker: {match.group(0)}")
            for required in ("starting Reprise", "database ready"):
                if required not in contents:
                    raise RunError(f"application log is missing '{required}'")

    def _wait_for_window(self, pid: int) -> int:
        for _ in range(120):
            if self.process is None or self.process.poll() is not None:
                raise RunError("Reprise exited before exposing a window")
            response = self.transport.call("list_windows", {"pid": pid})
            window_id = _window_id(response)
            if window_id is not None:
                return window_id
            time.sleep(0.25)
        raise RunError("Reprise did not expose a CUA window within 30 seconds")


def _mission_for_agent(mission: Mission) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "id": mission.mission_id,
        "goal": mission.goal,
        "persona": mission.persona,
        "mode": mission.mode,
        "agent": mission.agent,
        "profile": mission.profile,
        "budgets": dataclasses.asdict(mission.budgets),
        "capabilities": sorted(mission.capabilities),
        "fixture_tokens": sorted(mission.fixture_tokens),
        "oracles": list(mission.oracles),
        "workloads": list(mission.workloads),
        "success": list(mission.success),
        "forbidden": list(mission.forbidden),
    }


def _action_for_report(action: Any) -> Mapping[str, Any]:
    return action_to_dict(action)


def _observation_labels(observation: Mapping[str, Any]) -> tuple[str, ...]:
    elements = observation.get("elements", [])
    if not isinstance(elements, list):
        return ()
    return tuple(
        str(element["label"])
        for element in elements
        if isinstance(element, dict) and element.get("label")
    )


def _trace_from_observations(
    action: Mapping[str, Any],
    before: Mapping[str, Any],
    after: Mapping[str, Any],
    finding_codes: Sequence[str] = (),
) -> ActionTrace:
    def elements(observation: Mapping[str, Any]) -> list[Mapping[str, Any]]:
        raw = observation.get("elements", [])
        return [item for item in raw if isinstance(item, dict)] if isinstance(raw, list) else []

    def rows(observation: Mapping[str, Any]) -> tuple[tuple[str, float], ...]:
        projected = []
        for item in elements(observation):
            if not is_row(str(item.get("role", ""))) or not item.get("label"):
                continue
            frame = item.get("frame", {})
            y = frame.get("y", 0.0) if isinstance(frame, dict) else 0.0
            projected.append((str(item["label"]), float(y)))
        return tuple(sorted(projected, key=lambda row: (row[1], row[0])))

    def selected(observation: Mapping[str, Any]) -> tuple[str, ...]:
        return tuple(
            str(item["label"])
            for item in elements(observation)
            if item.get("selected") and item.get("label")
        )

    def values(observation: Mapping[str, Any]) -> tuple[tuple[str, str], ...]:
        return tuple(
            (str(item["label"]), str(item.get("value", "")))
            for item in elements(observation)
            if item.get("label")
        )

    after_elements = elements(after)
    return ActionTrace(
        action=action,
        before_labels=_observation_labels(before),
        after_labels=_observation_labels(after),
        before_rows=rows(before),
        after_rows=rows(after),
        before_selected_labels=selected(before),
        after_selected_labels=selected(after),
        after_actionable_labels=tuple(
            str(item["label"])
            for item in after_elements
            if item.get("actionable") and item.get("label")
        ),
        before_values=values(before),
        after_values=values(after),
        after_roles=tuple(
            (str(item["label"]), str(item.get("role", "")))
            for item in after_elements
            if item.get("label")
        ),
        finding_codes=tuple(finding_codes),
        state_changed=before.get("state_signature") != after.get("state_signature"),
        after_busy=any(
            str(item.get("role", "")) in BUSY_ROLES
            or any(
                word in str(item.get("label", "")).casefold()
                for word in BUSY_WORDS
            )
            for item in after_elements
        ),
    )


def ensure_run_complete(finished: bool, summary: Mapping[str, Any]) -> None:
    if not finished:
        raise RunError("mission ended without finish action")
    if summary.get("mission_complete") is not True:
        raise RunError("mission incomplete after workload evidence audit")


def _snapshot_has_busy_state(snapshot: Any) -> bool:
    return any(
        item.role in BUSY_ROLES
        or any(word in item.label.casefold() for word in BUSY_WORDS)
        for item in snapshot.elements
    )


def run(args: argparse.Namespace) -> int:
    _private_environment_required()
    mission = load_mission(args.mission)
    profile_root = args.profile_root.resolve()
    manifest_path = profile_root / "fixture-manifest.json"
    if not manifest_path.is_file():
        raise RunError("fixture profile manifest is missing")
    fixture_manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if fixture_manifest.get("profile") != mission.profile:
        raise RunError("fixture profile does not match mission")
    args.evidence_dir.mkdir(parents=True, exist_ok=True)
    write_gtk_animation_settings(profile_root, args.gtk_animations)
    connectivity_file = profile_root / "connectivity.state"
    connectivity_file.write_text("online\n", encoding="utf-8")
    transport = CliTransport(
        socket_path=args.socket,
        connectivity_file=connectivity_file,
    )
    lifecycle = AppLifecycle(
        app_binary=args.app_binary.resolve(),
        profile_root=profile_root,
        evidence_dir=args.evidence_dir,
        connectivity_file=connectivity_file,
        quit_delay_seconds=min(7_200, mission.budgets.seconds + 60),
        transport=transport,
    )
    report = RunReport(
        args.evidence_dir,
        mission_id=mission.mission_id,
        profile=mission.profile,
        seed=args.seed,
        commit=args.commit,
        required_workloads=len(mission.workloads),
        required_audits=tuple(range(len(mission.workloads))),
    )
    history: list[Mapping[str, Any]] = []
    traces: list[ActionTrace] = []
    agent_context: Any
    if args.agent_command_json:
        try:
            command = json.loads(args.agent_command_json)
        except json.JSONDecodeError as error:
            raise RunError("--agent-command-json must be a JSON argv list") from error
        if not isinstance(command, list):
            raise RunError("--agent-command-json must be a JSON argv list")
        private_agent_home = profile_root / "agent-home"
        private_agent_home.mkdir(exist_ok=True)
        agent_context = ExternalAgent(
            command,
            timeout_seconds=args.agent_timeout,
            private_home=private_agent_home,
        )
    else:
        agent_context = contextlib.nullcontext(DeterministicExplorer(mission, args.seed))

    gateway = ActionGateway(mission)
    finished = False
    try:
        pid, window_id, generation = lifecycle.start()
        executor = CuaExecutor(
            transport,
            pid=pid,
            window_id=window_id,
            session=args.session,
            state_prefix=f"launch-{generation}-state",
            fixture_tokens=mission.fixture_tokens,
            evidence_dir=args.evidence_dir / "states",
        )
        observation = executor.observe()
        hover_geometry = None
        if "hover" in mission.capabilities:
            hover_geometry = prepare_hover(
                transport,
                pid=pid,
                window_id=window_id,
                session=args.session,
                evidence_dir=args.evidence_dir,
                window=observation["window"],
                origin_override=parse_window_origin(args.window_origin),
            )
            executor.hover_geometry = hover_geometry
        if args.hover_smoke:
            finished = True
            raise HoverSmokeComplete
        with agent_context as agent:
            for _ in range(mission.budgets.actions):
                if lifecycle.process is None or lifecycle.process.poll() is not None:
                    raise RunError("Reprise exited during the exploratory mission")
                if isinstance(agent, DeterministicExplorer):
                    proposed = agent.propose(observation)
                else:
                    proposed = agent.propose(
                        _mission_for_agent(mission), observation, history
                    )
                accepted = gateway.accept(proposed, observation)
                if isinstance(accepted, CompleteWorkloadAction):
                    workload = mission.workloads[accepted.workload_index]
                    audit = audit_action_workload(
                        accepted.workload_index,
                        workload,
                        traces,
                        mission.fixture_tokens,
                    )
                    if workload.get("kind") == "batch-edit":
                        fixture_audit = audit_batch_edit(
                            profile_root,
                            dict(workload),
                            dict(mission.fixture_tokens),
                        )
                        audit = {
                            **audit,
                            **fixture_audit,
                            "workload_index": accepted.workload_index,
                            "complete": (
                                audit.get("complete") is True
                                and fixture_audit.get("complete") is True
                            ),
                        }
                    report.add_workload_audit(audit)
                    if audit.get("complete") is not True:
                        raise RunError(
                            f"workload evidence incomplete: {accepted.workload_index}"
                        )
                    gateway.confirm_workload(accepted.workload_index)
                    report.add_step(
                        action=_action_for_report(accepted),
                        before_state=observation["state_id"],
                        after_state=observation["state_id"],
                        findings=[],
                    )
                    history.append(
                        {
                            "action": _action_for_report(accepted),
                            "finding_codes": [],
                            "after_state": observation["state_id"],
                        }
                    )
                    continue
                if isinstance(accepted, FinishAction):
                    report.add_step(
                        action=_action_for_report(accepted),
                        before_state=observation["state_id"],
                        after_state=observation["state_id"],
                        findings=[],
                    )
                    finished = True
                    break
                if isinstance(accepted, RestartAction):
                    before_observation = observation
                    before_state = observation["state_id"]
                    pid, window_id, generation = lifecycle.restart()
                    executor = CuaExecutor(
                        transport,
                        pid=pid,
                        window_id=window_id,
                        session=args.session,
                        state_prefix=f"launch-{generation}-state",
                        fixture_tokens=mission.fixture_tokens,
                        evidence_dir=args.evidence_dir / "states",
                        hover_geometry=hover_geometry,
                    )
                    observation = executor.observe()
                    report.add_step(
                        action=_action_for_report(accepted),
                        before_state=before_state,
                        after_state=observation["state_id"],
                        findings=[],
                    )
                    trace = _trace_from_observations(
                        _action_for_report(accepted), before_observation, observation
                    )
                    traces.append(trace)
                    history.append(
                        {
                            "action": trace.action,
                            "finding_codes": [],
                            "after_state": observation["state_id"],
                        }
                    )
                    continue
                result = executor.execute(accepted)
                finding_dicts = [dataclasses.asdict(item) for item in result.findings]
                report.add_step(
                    action=_action_for_report(accepted),
                    before_state=result.before.state_id,
                    after_state=result.after.state_id,
                    findings=finding_dicts,
                )
                history.append(
                    {
                        "action": _action_for_report(accepted),
                        "finding_codes": [item.code for item in result.findings],
                        "after_state": result.after.state_id,
                    }
                )
                before_observation = executor.observation_from_snapshot(result.before)
                observation = executor.observation_from_snapshot(result.settled[-1])
                trace = _trace_from_observations(
                        _action_for_report(accepted),
                        before_observation,
                        observation,
                        [item.code for item in result.findings],
                    )
                if any(_snapshot_has_busy_state(item) for item in result.settled):
                    trace = dataclasses.replace(trace, after_busy=True)
                traces.append(
                    trace
                )
    except HoverSmokeComplete:
        pass
    finally:
        lifecycle.stop()
        report.write()
    lifecycle.assert_clean_logs()
    summary = report.write()
    if args.hover_smoke:
        return 0
    ensure_run_complete(finished, summary)
    return 0


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mission", required=True, type=pathlib.Path)
    parser.add_argument("--profile-root", required=True, type=pathlib.Path)
    parser.add_argument("--evidence-dir", required=True, type=pathlib.Path)
    parser.add_argument("--app-binary", required=True, type=pathlib.Path)
    parser.add_argument("--socket", required=True, type=pathlib.Path)
    parser.add_argument("--session", required=True)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--agent-command-json")
    parser.add_argument("--agent-timeout", type=float, default=30.0)
    parser.add_argument("--gtk-animations", choices=("on", "off"), default="on")
    parser.add_argument("--hover-smoke", action="store_true")
    parser.add_argument("--window-origin")
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    try:
        return run(parse_args(argv))
    except (
        RunError,
        DriverError,
        FixtureError,
        ContractError,
        OSError,
        ValueError,
    ) as error:
        print(f"exploratory run failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
