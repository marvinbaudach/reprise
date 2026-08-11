#!/usr/bin/env python3
"""Outcome and exit-code tests for exploratory mission reports."""

from __future__ import annotations

import contextlib
import io
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from typing import Any, Callable, Mapping
from unittest import mock


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
sys.path.insert(0, str(EXPLORE_ROOT))

import protocol  # noqa: E402
import runner  # noqa: E402
import launch  # noqa: E402
from atspi_geometry import GeometryError, GeometryNode  # noqa: E402
from driver import CliTransport, DriverError  # noqa: E402
from hover_geometry import WindowGeometry  # noqa: E402
from oracles import Finding, normalize_snapshot  # noqa: E402
from protocol import ActionGateway, load_mission  # noqa: E402
from report import RunReport  # noqa: E402


RESTART_ACTION = {"kind": "restart", "reason": "budget probe"}
ACTIVATE_OVER_PIXELS = {
    "kind": "activate",
    "target": {"label": "Music"},
    "dispatch": "px",
    "expect_effect": "required",
}
HOVER_TARGET = {"kind": "hover", "target": {"label": "Music"}}


class FakeLifecycle:
    """Enough of AppLifecycle to reach the action loop without an app."""

    def __init__(self, **_kwargs: Any) -> None:
        self.process = SimpleNamespace(poll=lambda: None)
        self.startup_timings: list[Mapping[str, Any]] = []

    def start(self) -> tuple[int, int, int]:
        return 12, 34, 1

    def restart(self) -> tuple[int, int, int]:
        return 13, 35, 2

    def stop(self) -> None:
        return None

    def assert_clean_logs(self) -> None:
        return None


class FakeExecutor:
    """Answers the two questions the loop asks between accepted actions."""

    def __init__(
        self,
        observation: Mapping[str, Any],
        execute: Callable[[Any], Any] | None,
    ) -> None:
        self.observation = dict(observation)
        self._execute = execute

    def observe(self) -> Mapping[str, Any]:
        return self.observation

    def observation_from_snapshot(self, _snapshot: Any) -> Mapping[str, Any]:
        return self.observation

    def execute(self, accepted: Any) -> Any:
        if self._execute is None:
            raise AssertionError("this run was not supposed to execute an action")
        return self._execute(accepted)


class StationaryPointerTransport:
    """A driver boundary that acknowledges moves but leaves the X11 pointer still."""

    def __init__(self, **_kwargs: Any) -> None:
        self.position = (800.0, 500.0)
        self.calls: list[tuple[str, Mapping[str, Any]]] = []
        self.transport_faults = 0

    def call(self, tool: str, payload: Mapping[str, Any]) -> Mapping[str, Any]:
        self.calls.append((tool, dict(payload)))
        if tool == "get_cursor_position":
            return {"source": "x11", "x": self.position[0], "y": self.position[1]}
        if tool == "move_cursor":
            return {
                "effect": "unverifiable",
                "delivery": {"mode": "not_applicable"},
                "route": "global_input",
            }
        if tool == "get_window_state":
            return {"elements": []}
        raise AssertionError(f"unexpected stationary-pointer tool: {tool}")


class GeometryRunTransport:
    """The driver snapshot boundary used by the restart evidence regression."""

    def __init__(self, **_kwargs: Any) -> None:
        self.transport_faults = 0

    def call(self, tool: str, payload: Mapping[str, Any]) -> Mapping[str, Any]:
        if tool != "get_window_state":
            raise AssertionError(f"unexpected geometry-run tool: {tool}")
        return {
            "snapshot_id": f"snapshot-{payload['pid']}",
            "window": {"x": 200, "y": 50, "width": 1200, "height": 800},
            "structuredContent": {
                "elements": [
                    {
                        "element_index": 0,
                        "role": "frame",
                        "label": "Reprise",
                        "depth": 0,
                        "frame": {"x": 200, "y": 50, "w": 1200, "h": 800},
                    },
                    {
                        "element_index": 3,
                        "role": "push button",
                        "label": "Music",
                        "depth": 1,
                        "frame": {"x": 200, "y": 50, "w": 34, "h": 34},
                    },
                ]
            },
        }

    def take_findings(self) -> list[Any]:
        return []


def scripted_explorer(action: Mapping[str, Any]) -> type:
    """Stands in for the deterministic explorer with one scripted proposal."""

    class Scripted:
        def __init__(self, mission: Any, seed: int) -> None:
            self.mission = mission
            self.seed = seed

        def propose(self, observation: Mapping[str, Any]) -> dict[str, Any]:
            return {
                "schema_version": 1,
                "state_id": observation["state_id"],
                **action,
            }

    return Scripted


def report_at(path: pathlib.Path) -> RunReport:
    return RunReport(
        path,
        mission_id="section-search-isolation",
        profile="mixed-sources-128",
        seed=11,
        commit="abc123",
        required_workloads=1,
        required_audits=(0,),
    )


class OutcomeTests(unittest.TestCase):
    def test_incomplete_audit_is_valid_evidence_but_not_a_completed_checkpoint(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = report_at(pathlib.Path(directory))
            report.add_workload_audit(
                {"workload_index": 0, "kind": "section-search", "complete": False}
            )
            report.add_finding(
                {
                    "code": "workload-incomplete",
                    "severity": "error",
                    "confidence": 1.0,
                    "summary": "Checkpoint evidence was incomplete.",
                    "evidence": {"workload_index": 0},
                    "blocks_gate": True,
                }
            )
            report.add_step(
                action={"kind": "finish", "reason": "budget consumed"},
                before_state="state-1",
                after_state="state-1",
                findings=[],
            )

            summary = report.write()

        self.assertEqual(summary["outcome"], "incomplete")
        self.assertFalse(summary["mission_complete"])
        self.assertEqual(summary["completed_workload_indices"], [])
        self.assertEqual(summary["finding_codes"]["workload-incomplete"], 1)
        runner.ensure_run_complete(True, summary)

    def test_complete_run_has_a_complete_outcome(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = report_at(pathlib.Path(directory))
            report.add_workload_audit(
                {"workload_index": 0, "kind": "section-search", "complete": True}
            )
            for action in (
                {"kind": "complete-workload", "workload_index": 0},
                {"kind": "finish", "reason": "done"},
            ):
                report.add_step(
                    action=action,
                    before_state="state-1",
                    after_state="state-1",
                    findings=[],
                )

            summary = report.write()

        self.assertEqual(summary["outcome"], "complete")
        self.assertTrue(summary["mission_complete"])
        self.assertIsNone(summary["abort_reason"])

    def test_abort_reason_wins_over_completion(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = report_at(pathlib.Path(directory))
            report.set_abort_reason("driver stayed unavailable")

            summary = report.write()

        self.assertEqual(summary["outcome"], "aborted")
        self.assertEqual(summary["abort_reason"], "driver stayed unavailable")

    def test_summary_exposes_the_new_evidence_channels_even_when_empty(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            summary = report_at(pathlib.Path(directory)).write()

        self.assertEqual(summary["transport_faults"], 0)
        self.assertEqual(summary["unknown_action_names"], {})
        self.assertEqual(summary["oracle_activity"], {})
        self.assertIsNone(summary["window_setup"])

    def test_gateway_can_finish_after_an_audited_incomplete_checkpoint(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "section-search-isolation.json"
        )
        gateway = ActionGateway(mission)
        gateway.record_incomplete_workload(0)
        gateway.record_incomplete_workload(1)
        observation = {"state_id": "state-1", "actionable_labels": []}

        accepted = gateway.accept(
            {
                "schema_version": 1,
                "state_id": "state-1",
                "kind": "finish",
                "reason": "audits retained",
            },
            observation,
        )

        self.assertEqual(accepted.kind, "finish")

    def test_only_aborted_tools_return_nonzero(self) -> None:
        with mock.patch.object(runner, "parse_args", return_value=object()), mock.patch.object(
            runner, "run", return_value=0
        ):
            self.assertEqual(runner.main([]), 0)
        with mock.patch.object(runner, "parse_args", return_value=object()), mock.patch.object(
            runner, "run", side_effect=runner.RunError("app died")
        ):
            self.assertEqual(runner.main([]), 1)

    def test_missing_finish_still_aborts_the_tool(self) -> None:
        with self.assertRaisesRegex(runner.RunError, "without finish"):
            runner.ensure_run_complete(False, {"mission_complete": False})

    def test_a_declared_oracle_that_never_evaluates_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = report_at(pathlib.Path(directory))
            report.set_oracle_activity({"accessibility": {"evaluated": 0, "fired": 0}})
            summary = report.write()

        self.assertEqual(summary["finding_codes"]["oracle-never-evaluated"], 1)

    def test_an_evaluated_clean_oracle_is_not_reported_as_silent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = report_at(pathlib.Path(directory))
            report.set_oracle_activity({"accessibility": {"evaluated": 1, "fired": 0}})
            summary = report.write()

        self.assertNotIn("oracle-never-evaluated", summary["finding_codes"])

    def test_a_superseded_oracle_is_not_reported_as_silent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = report_at(pathlib.Path(directory))
            report.set_oracle_activity(
                {
                    "accessibility": {
                        "evaluated": 0,
                        "fired": 0,
                        "superseded_by": "semantic-route-unavailable",
                    }
                }
            )
            summary = report.write()

        self.assertNotIn("oracle-never-evaluated", summary["finding_codes"])

    def test_runner_counts_only_applicable_oracles_and_their_findings(self) -> None:
        tracker = runner.OracleActivityTracker(("accessibility", "scroll-direction"))

        tracker.record(
            SimpleNamespace(kind="activate", dispatch="ax"),
            (SimpleNamespace(code="no-accessible-action"),),
        )

        self.assertEqual(
            tracker.activity,
            {
                "accessibility": {"evaluated": 1, "fired": 1},
                "scroll-direction": {"evaluated": 0, "fired": 0},
            },
        )


class RunPathTests(unittest.TestCase):
    """Drives the real run(), because a hand-built summary hid both blockers."""

    OBSERVATION = {
        "state_id": "state-1",
        "actionable_labels": ["Music", "Delete library"],
        "elements": [
            {
                "label": "Music",
                "role": "button",
                "actionable": True,
                "frame": {"x": 0, "y": 0, "width": 120, "height": 30},
            }
        ],
        "window": {"x": 0, "y": 0, "width": 1600, "height": 1000},
    }

    def drive(
        self,
        *,
        action: Mapping[str, Any],
        budgets: Mapping[str, int],
        execute: Callable[[Any], Any] | None = None,
        monotonic: Callable[[], float] | None = None,
        configure_mission: Callable[[dict[str, Any]], None] | None = None,
        transport_factory: Callable[..., Any] | None = None,
        extra_patches: Mapping[str, Any] | None = None,
        argv_extra: tuple[str, ...] = (),
        real_launch_executor: bool = False,
    ) -> SimpleNamespace:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        root = pathlib.Path(directory.name)
        payload = json.loads(
            (EXPLORE_ROOT / "missions" / "first-time-exploration.json").read_text(
                encoding="utf-8"
            )
        )
        payload["budgets"] = dict(budgets)
        payload["workloads"] = []
        if configure_mission is not None:
            configure_mission(payload)
        mission_path = root / "mission.json"
        mission_path.write_text(json.dumps(payload), encoding="utf-8")
        profile_root = root / "profile"
        profile_root.mkdir()
        (profile_root / "fixture-manifest.json").write_text(
            json.dumps({"profile": payload["profile"]}), encoding="utf-8"
        )
        evidence_dir = root / "evidence"
        transports: list[Any] = []

        def remember(**kwargs: Any) -> Any:
            """Build the transport exactly as the runner asks for it."""
            factory = transport_factory or CliTransport
            transports.append(factory(**kwargs))
            return transports[-1]

        executor = FakeExecutor(self.OBSERVATION, execute)
        argv = [
            "--mission", str(mission_path),
            "--profile-root", str(profile_root),
            "--evidence-dir", str(evidence_dir),
            "--app-binary", str(root / "reprise"),
            "--socket", str(root / "driver.sock"),
            "--session", "outcome-test",
            "--commit", "abc123",
            *argv_extra,
        ]
        with contextlib.ExitStack() as stack:
            replacements = [
                ("_private_environment_required", lambda: None),
                ("AppLifecycle", FakeLifecycle),
                ("CliTransport", remember),
                ("DeterministicExplorer", scripted_explorer(action)),
            ]
            if not real_launch_executor:
                replacements.append(
                    ("launch_executor", lambda *_a, **_k: (12, 34, 1, None, executor))
                )
            for name, replacement in replacements:
                stack.enter_context(mock.patch.object(runner, name, replacement))
            for name, replacement in (extra_patches or {}).items():
                stack.enter_context(mock.patch.object(runner, name, replacement))
            if monotonic is not None:
                stack.enter_context(
                    mock.patch.object(protocol, "time", SimpleNamespace(monotonic=monotonic))
                )
            messages = io.StringIO()
            with contextlib.redirect_stderr(messages):
                exit_code = runner.main(argv)
        return SimpleNamespace(
            exit_code=exit_code,
            summary=json.loads((evidence_dir / "summary.json").read_text(encoding="utf-8")),
            transports=transports,
            evidence_dir=evidence_dir,
            messages=messages.getvalue(),
        )

    def snapshot(self, *, actions: tuple[str, ...] = ()) -> Any:
        return normalize_snapshot(
            {
                "elements": [
                    {
                        "label": "Music",
                        "role": "button",
                        "actions": list(actions),
                        "frame": {"x": 0, "y": 0, "width": 120, "height": 30},
                    }
                ]
            },
            state_id="state-1",
            captured_ms=0,
        )

    def test_the_runner_gives_its_transport_the_run_evidence_directory(self) -> None:
        result = self.drive(
            action=RESTART_ACTION,
            budgets={"actions": 1, "seconds": 900, "restarts": 1},
        )

        self.assertEqual(result.exit_code, 0)
        (transport,) = result.transports
        self.assertEqual(transport.evidence_dir, result.evidence_dir)
        # The retained payload only counts once it is on disk, so drive one
        # fault through the transport the runner itself built.
        transport._run = lambda command: subprocess.CompletedProcess(
            command, 0, "<html>not json</html>", ""
        )
        with self.assertRaises(DriverError):
            transport.call("click", {"pid": 12})

        lines = (result.evidence_dir / "driver-faults.jsonl").read_text(
            encoding="utf-8"
        ).splitlines()
        self.assertEqual(json.loads(lines[0])["tool"], "click")
        self.assertIn("not json", json.loads(lines[0])["stdout_head"])

    def test_a_spent_action_budget_is_incomplete_and_exits_zero(self) -> None:
        result = self.drive(
            action=RESTART_ACTION,
            budgets={"actions": 2, "seconds": 900, "restarts": 2},
        )

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.summary["outcome"], "incomplete")
        self.assertIsNone(result.summary["abort_reason"])
        self.assertFalse(result.summary["finished"])
        self.assertEqual(result.summary["steps"], 2)
        self.assertIn("action budget exhausted", result.messages)

    def test_a_spent_time_budget_is_incomplete_and_exits_zero(self) -> None:
        readings = iter([0.0])

        result = self.drive(
            action=RESTART_ACTION,
            budgets={"actions": 20, "seconds": 10, "restarts": 10},
            # The gateway reads the clock once at construction; every later
            # read sits far beyond the declared ten seconds.
            monotonic=lambda: next(readings, 10_000.0),
        )

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.summary["outcome"], "incomplete")
        self.assertIsNone(result.summary["abort_reason"])
        self.assertEqual(result.summary["steps"], 0)
        self.assertIn("time budget exhausted", result.messages)

    def test_a_forbidden_target_still_aborts_the_tool(self) -> None:
        result = self.drive(
            action={
                "kind": "activate",
                "target": {"label": "Delete library"},
                "dispatch": "ax",
                "expect_effect": "required",
            },
            budgets={"actions": 5, "seconds": 900, "restarts": 1},
        )

        self.assertEqual(result.exit_code, 1)
        self.assertEqual(result.summary["outcome"], "aborted")
        self.assertIn("forbidden target", result.summary["abort_reason"])

    def test_a_pointer_only_run_does_not_blame_the_accessibility_oracle(self) -> None:
        snapshot = self.snapshot()

        result = self.drive(
            action=ACTIVATE_OVER_PIXELS,
            budgets={"actions": 1, "seconds": 900, "restarts": 1},
            execute=lambda _accepted: SimpleNamespace(
                before=snapshot,
                after=snapshot,
                settled=(snapshot,),
                evidence=SimpleNamespace(kind="activate", dispatch="px"),
                findings=(),
            ),
        )

        self.assertEqual(
            result.summary["oracle_activity"]["accessibility"],
            {"evaluated": 0, "fired": 0, "superseded_by": "pointer-dispatch-only"},
        )
        # Only waiting-state, layout-shift and scroll-direction stay silent.
        self.assertEqual(result.summary["finding_codes"]["oracle-never-evaluated"], 3)

    def test_an_invented_action_name_is_counted_in_the_summary(self) -> None:
        before = self.snapshot()
        after = self.snapshot(actions=("foo.bar",))

        result = self.drive(
            action=ACTIVATE_OVER_PIXELS,
            budgets={"actions": 1, "seconds": 900, "restarts": 1},
            execute=lambda _accepted: SimpleNamespace(
                before=before,
                after=after,
                settled=(after,),
                evidence=SimpleNamespace(kind="activate", dispatch="px"),
                findings=(),
            ),
        )

        self.assertEqual(result.summary["unknown_action_names"], {"foo.bar": 1})

    def test_stationary_pointer_aborts_before_hover_can_be_blamed_on_the_product(
        self,
    ) -> None:
        snapshot = self.snapshot()

        def enable_hover(payload: dict[str, Any]) -> None:
            payload["capabilities"].append("hover")
            payload["oracles"].append("hover-affordance")

        with mock.patch.object(
            launch,
            "measure_cursor_visibility",
            return_value={"cursor_in_screenshot": False},
        ):
            result = self.drive(
                action=HOVER_TARGET,
                budgets={"actions": 1, "seconds": 900, "restarts": 0},
                configure_mission=enable_hover,
                transport_factory=StationaryPointerTransport,
                argv_extra=("--window-origin", "0,0"),
                execute=lambda _accepted: SimpleNamespace(
                    before=snapshot,
                    after=snapshot,
                    settled=(snapshot,),
                    evidence=SimpleNamespace(kind="hover", dispatch=None),
                    findings=(
                        Finding(
                            "hover-affordance-missing",
                            "error",
                            1.0,
                            "The hover target did not change.",
                        ),
                    ),
                ),
            )

        self.assertEqual(result.exit_code, 1)
        self.assertEqual(result.summary["outcome"], "aborted")
        self.assertEqual(result.summary["steps"], 0)
        self.assertNotIn("hover-affordance-missing", result.summary["finding_codes"])
        evidence = json.loads(
            (result.evidence_dir / "hover-preflight.json").read_text(encoding="utf-8")
        )
        self.assertEqual(evidence["before"], {"source": "x11", "x": 800.0, "y": 500.0})
        self.assertNotEqual(evidence["target"], {"x": 800.0, "y": 500.0})
        self.assertEqual(evidence["after"], evidence["before"])
        self.assertEqual(evidence["verdict"], "pointer-did-not-reach-target")

    def test_restart_retains_geometry_evidence_from_every_executor_generation(
        self,
    ) -> None:
        origin = WindowGeometry(200, 50, 1200, 800)

        def geometry_provider(pid: int, _origin: Any) -> Callable[[], Any]:
            if pid == 12:
                def failed_walk() -> Any:
                    raise GeometryError("generation one accessibility walk failed")

                return failed_walk
            return lambda: [
                GeometryNode("frame", "Reprise", -5, -5, 1200, 800, ()),
                GeometryNode("push button", "Music", -5, 41, 34, 34, ("click",)),
            ]

        result = self.drive(
            action=RESTART_ACTION,
            budgets={"actions": 1, "seconds": 900, "restarts": 1},
            transport_factory=GeometryRunTransport,
            real_launch_executor=True,
            extra_patches={
                "apply_window_size": lambda *_a, **_k: None,
                "resolve_window_origin": lambda *_a, **_k: origin,
                "make_geometry_provider": geometry_provider,
            },
        )

        self.assertEqual(result.exit_code, 0)
        measurements = result.summary["geometry_measurements"]
        self.assertEqual(
            [(item["generation"], item["state_id"], item["trusted"]) for item in measurements],
            [
                (1, "launch-1-state-1", False),
                (2, "launch-2-state-1", True),
            ],
        )
        self.assertEqual(
            measurements[0]["failure"], "generation one accessibility walk failed"
        )
        self.assertEqual(measurements[1]["resolution"]["resolved"], 2)
        self.assertIsNone(result.summary["geometry_resolution"])
        self.assertIn(
            "generation one accessibility walk failed",
            result.summary["geometry_failures"][0],
        )
        self.assertFalse(result.summary["geometry_trusted"])
        report = (result.evidence_dir / "report.md").read_text(encoding="utf-8")
        self.assertIn("Generation 1", report)
        self.assertIn("generation one accessibility walk failed", report)
        self.assertIn("Generation 2", report)
        self.assertIn("Measured positions: 2 of 2", report)


if __name__ == "__main__":
    unittest.main()
