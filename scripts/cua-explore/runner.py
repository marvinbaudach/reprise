#!/usr/bin/env python3
"""Run one mission inside an already isolated X11/D-Bus/AT-SPI session."""

from __future__ import annotations

import argparse
import contextlib
import dataclasses
import json
import pathlib
import shutil
import subprocess
import sys
from typing import Any, Callable, Iterator, Mapping, Sequence

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
from oracles import Finding
from ui_vocabulary import BUSY_ROLES, BUSY_WORDS, is_row
from workload_audit import ActionTrace, audit_action_workload
from window_setup import (
    AppLifecycle,
    HoverSmokeComplete,
    RunError,
    accessibility_tree_ready,
    app_launch_argv,
    apply_window_size,
    parse_window_origin,
    prepare_hover,
    private_environment_required as _private_environment_required,
    startup_timeout_seconds,
    wait_for_accessibility_tree,
    write_gtk_animation_settings,
)

# AppLifecycle launches through: unshare --user --map-current-user --net --

ORACLE_FINDING_CODES = {
    "feedback": {"slow-visible-feedback"},
    "waiting-state": {"missing-waiting-feedback"},
    "layout-shift": {"uninvited-layout-shift"},
    "pointer-reachability": {"suspected-occlusion", "misrouted-click"},
    "scroll-direction": {"wrong-scroll-direction", "scroll-jump", "scroll-lost-selection"},
    "main-loop-stall": {"main-loop-stall"},
    "accessibility": {
        "degraded-accessibility",
        "invisible-actionable",
        "no-accessible-action",
        "suspected-no-handler",
    },
    "offline-continuity": {
        "offline-broke-local-music",
        "offline-lost-cached-content",
        "reconnect-kept-offline-status",
    },
    "hover-affordance": {
        "hover-skipped",
        "hover-unmeasurable",
        "hover-affordance-missing",
        "hover-affordance-weak",
    },
}


class OracleActivityTracker:
    """Counts whether each declared oracle reached an applicable observation."""

    def __init__(self, names: Sequence[str]) -> None:
        self.activity = {
            str(name): {"evaluated": 0, "fired": 0} for name in names
        }

    def record(self, evidence: Any, findings: Sequence[Any]) -> None:
        codes = {str(item.code) for item in findings}
        for name, record in self.activity.items():
            if name == "clean-runtime" or not self._applies(name, evidence):
                continue
            record["evaluated"] += 1
            record["fired"] += len(codes & ORACLE_FINDING_CODES.get(name, set()))

    def record_clean_runtime(self, *, fired: bool) -> None:
        record = self.activity.get("clean-runtime")
        if record is not None:
            record["evaluated"] += 1
            record["fired"] += int(fired)

    def supersede(self, name: str, finding_code: str) -> None:
        if name in self.activity:
            self.activity[name]["superseded_by"] = finding_code

    @staticmethod
    def _applies(name: str, evidence: Any) -> bool:
        kind = str(getattr(evidence, "kind", ""))
        return {
            "feedback": kind == "activate",
            "waiting-state": kind == "wait",
            "layout-shift": kind == "wait",
            "pointer-reachability": kind == "activate" and evidence.dispatch == "px",
            "scroll-direction": kind == "scroll",
            "main-loop-stall": bool(kind),
            "accessibility": kind == "activate" and evidence.dispatch == "ax",
            "offline-continuity": kind == "set-connectivity",
            "hover-affordance": kind == "hover",
        }.get(name, False)


def retain_agent_notes(profile_root: pathlib.Path, evidence_dir: pathlib.Path) -> None:
    source = profile_root / "agent-home"
    if not source.is_dir():
        return
    files = sorted((*source.glob("*.jsonl"), *source.glob("*.json")))
    if not files:
        return
    target = evidence_dir / "agent"
    target.mkdir(parents=True, exist_ok=True)
    for path in files:
        shutil.copy2(path, target / path.name)


def read_agent_dispatch_policy(profile_root: pathlib.Path) -> Mapping[str, Any] | None:
    path = profile_root / "agent-home" / "dispatch-policy.json"
    if not path.is_file():
        return None
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return value if isinstance(value, dict) else None


def agent_product_findings(profile_root: pathlib.Path) -> Iterator[Mapping[str, Any]]:
    path = profile_root / "agent-home" / "agent-notes.jsonl"
    if not path.is_file():
        return
    severities = {
        "semantic-activation-ineffective": "warning",
        "semantic-route-unavailable": "error",
    }
    for line in path.read_text(encoding="utf-8").splitlines():
        try:
            note = json.loads(line)
        except json.JSONDecodeError:
            continue
        code = str(note.get("code", ""))
        if code not in severities:
            continue
        yield {
            "code": code,
            "severity": severities[code],
            "confidence": 1.0,
            "summary": str(note.get("summary", "")),
            "evidence": note.get("evidence", {}),
            "blocks_gate": False,
        }


def run_hover_probe(
    transport: CliTransport,
    *,
    pid: int,
    window_id: int,
    session: str,
    geometry: Any,
    label: str,
    evidence_dir: pathlib.Path,
) -> None:
    """Measure whether a pointer move reaches the app, both ways, and report."""
    from hover_probe import (
        default_x11_cursor,
        default_x11_move,
        probe_hover,
        render_probe_table,
        write_probe_evidence,
    )

    results = probe_hover(
        transport,
        pid=pid,
        window_id=window_id,
        session=session,
        origin=geometry,
        label=label,
        evidence_dir=evidence_dir,
        x11_move=default_x11_move(),
        x11_cursor=default_x11_cursor(),
        geometry_provider=make_geometry_provider(pid, geometry),
    )
    write_probe_evidence(results, evidence_dir)
    print(render_probe_table(results))


def make_geometry_provider(pid: int, origin: Any = None) -> Any:
    """Walk the accessibility tree ourselves; the driver's frames carry no position."""
    from atspi_geometry import walk_window_nodes

    size = (
        (float(origin.width), float(origin.height)) if origin is not None else None
    )

    def provider() -> Any:
        return walk_window_nodes(pid, size)

    return provider


def run_click_probe(
    transport: CliTransport,
    *,
    pid: int,
    window_id: int,
    session: str,
    origin: Any,
    label: str,
    evidence_dir: pathlib.Path,
) -> None:
    """Activate one control over AT-SPI and over pixels, then report both."""
    from click_probe import probe_click, render_click_table, write_click_evidence

    results = probe_click(
        transport,
        pid=pid,
        window_id=window_id,
        session=session,
        origin=origin,
        label=label,
        evidence_dir=evidence_dir,
        geometry_provider=make_geometry_provider(pid, origin),
    )
    write_click_evidence(results, evidence_dir)
    print(render_click_table(results))


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
    if summary.get("outcome") is None and summary.get("mission_complete") is not True:
        raise RunError("mission incomplete after workload evidence audit")


def _snapshot_has_busy_state(snapshot: Any) -> bool:
    return any(
        item.role in BUSY_ROLES
        or any(word in item.label.casefold() for word in BUSY_WORDS)
        for item in snapshot.elements
    )


def launch_executor(
    lifecycle_action: Callable[[], tuple[int, int, int]],
    transport: Any,
    *,
    mission: Mission,
    args: argparse.Namespace,
    report: RunReport,
    hover_geometry: Any | None = None,
) -> tuple[int, int, int, WindowGeometry, CuaExecutor]:
    """Start one app generation, apply mission geometry, then build its executor."""
    pid, window_id, generation = lifecycle_action()
    window_setup = apply_window_size(
        transport,
        window_id=window_id,
        requested=mission.window,
    )
    report.set_window_setup(window_setup)
    if window_setup is not None and window_setup["honoured"] is not True:
        report.add_finding(
            dataclasses.asdict(
                Finding(
                    "window-size-not-honoured",
                    "warning",
                    1.0,
                    "The window manager did not honour the mission's requested size.",
                    window_setup,
                    blocks_gate=False,
                )
            )
        )
    window_origin = resolve_window_origin(
        transport, pid=pid, window_id=window_id
    )
    executor = CuaExecutor(
        transport,
        pid=pid,
        window_id=window_id,
        session=args.session,
        state_prefix=f"launch-{generation}-state",
        fixture_tokens=mission.fixture_tokens,
        evidence_dir=args.evidence_dir / "states",
        hover_geometry=hover_geometry,
        geometry_provider=make_geometry_provider(pid, window_origin),
        window_origin=window_origin,
    )
    return pid, window_id, generation, window_origin, executor


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
        session=args.session,
        window_timeout_seconds=startup_timeout_seconds(mission.profile),
        ready_timeout_seconds=startup_timeout_seconds(mission.profile),
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
    oracle_activity = OracleActivityTracker(mission.oracles)
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
        explorer = None
    else:
        explorer = DeterministicExplorer(mission, args.seed)
        agent_context = contextlib.nullcontext(explorer)

    gateway = ActionGateway(mission)
    finished = False
    executor: Any = None
    try:
        pid, window_id, generation, window_origin, executor = launch_executor(
            lifecycle.start,
            transport,
            mission=mission,
            args=args,
            report=report,
        )
        observation = executor.observe()
        hover_geometry = None
        if "hover" in mission.capabilities:
            hover_geometry, cursor_visibility = prepare_hover(
                transport,
                pid=pid,
                window_id=window_id,
                session=args.session,
                evidence_dir=args.evidence_dir,
                window=observation["window"],
                origin_override=parse_window_origin(args.window_origin),
            )
            executor.hover_geometry = hover_geometry
            executor.exclude_cursor = bool(
                cursor_visibility.get("cursor_in_screenshot")
            )
            report.set_cursor_visibility(cursor_visibility)
        if args.click_probe:
            run_click_probe(
                transport,
                pid=pid,
                window_id=window_id,
                session=args.session,
                origin=window_origin,
                label=args.click_probe,
                evidence_dir=args.evidence_dir,
            )
            finished = True
            raise HoverSmokeComplete
        if args.hover_probe:
            if hover_geometry is None:
                raise RunError("--hover-probe requires a mission with hover capability")
            run_hover_probe(
                transport,
                pid=pid,
                window_id=window_id,
                session=args.session,
                geometry=hover_geometry,
                label=args.hover_probe,
                evidence_dir=args.evidence_dir,
            )
            finished = True
            raise HoverSmokeComplete
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
                        report.add_finding(
                            dataclasses.asdict(
                                Finding(
                                    "workload-incomplete",
                                    "error",
                                    1.0,
                                    "The workload checkpoint did not produce complete retained evidence.",
                                    {
                                        "workload_index": accepted.workload_index,
                                        "kind": workload.get("kind"),
                                        "audit": audit,
                                    },
                                    blocks_gate=True,
                                )
                            )
                        )
                        gateway.record_incomplete_workload(accepted.workload_index)
                        history.append(
                            {
                                "action": _action_for_report(accepted),
                                "finding_codes": ["workload-incomplete"],
                                "after_state": observation["state_id"],
                            }
                        )
                        continue
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
                    (
                        pid,
                        window_id,
                        generation,
                        restart_origin,
                        executor,
                    ) = launch_executor(
                        lifecycle.restart,
                        transport,
                        mission=mission,
                        args=args,
                        report=report,
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
                oracle_activity.record(result.evidence, result.findings)
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
    except Exception as error:
        report.set_abort_reason(str(error))
        raise
    finally:
        lifecycle.stop()
        retain_agent_notes(profile_root, args.evidence_dir)
        report.set_startup_timings(lifecycle.startup_timings)
        report.set_geometry_failures(
            list(getattr(executor, "geometry_failures", []) or [])
        )
        report.set_geometry_calibration(
            getattr(executor, "geometry_calibration", None)
        )
        report.set_geometry_resolution(
            getattr(executor, "geometry_resolution", None)
        )
        report.set_hover_coverage(getattr(explorer, "hover_coverage", None))
        report.set_transport_faults(getattr(transport, "transport_faults", 0))
        report.set_unknown_action_names(
            getattr(executor, "unknown_action_names", {}) or {}
        )
        dispatch_policy = read_agent_dispatch_policy(profile_root)
        report.set_dispatch_policy(
            dispatch_policy or getattr(explorer, "dispatch_policy", None)
        )
        if dispatch_policy and dispatch_policy.get("reason") == "semantic-route-unavailable":
            oracle_activity.supersede("accessibility", "semantic-route-unavailable")
        for finding in agent_product_findings(profile_root):
            report.add_finding(finding)
        report.set_oracle_activity(oracle_activity.activity)
        report.write()
    try:
        lifecycle.assert_clean_logs()
    except Exception as error:
        oracle_activity.record_clean_runtime(fired=True)
        report.set_oracle_activity(oracle_activity.activity)
        report.set_abort_reason(str(error))
        report.write()
        raise
    oracle_activity.record_clean_runtime(fired=False)
    report.set_oracle_activity(oracle_activity.activity)
    summary = report.write()
    if args.hover_smoke or args.hover_probe or args.click_probe:
        return 0
    if not finished:
        report.set_abort_reason("mission ended without finish action")
        summary = report.write()
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
    parser.add_argument("--hover-probe")
    parser.add_argument("--click-probe")
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
