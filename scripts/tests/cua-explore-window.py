#!/usr/bin/env python3
"""Display-free tests for declarative exploratory-run window setup."""

from __future__ import annotations

import dataclasses
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
sys.path.insert(0, str(EXPLORE_ROOT))

from driver import DriverError  # noqa: E402
from hover_geometry import WindowGeometry  # noqa: E402
from protocol import ContractError, load_mission  # noqa: E402
import runner  # noqa: E402
import window_setup  # noqa: E402
from window_setup import apply_window_size  # noqa: E402


class FakeTransport:
    def __init__(
        self,
        achieved: WindowGeometry,
        *,
        measure_failures: int = 0,
        resize_error: Exception | None = None,
    ) -> None:
        self.achieved = achieved
        self.calls: list[tuple[object, ...]] = []
        self.measure_failures = measure_failures
        self.resize_error = resize_error

    def resize_window(self, window_id: int, width: int, height: int):
        self.calls.append(("resize", window_id, width, height))
        if self.resize_error is not None:
            raise self.resize_error
        return {"effect": "unverifiable", "verified": False}

    def wmctrl_geometry(self, window_id: int) -> WindowGeometry:
        self.calls.append(("measure", window_id))
        if self.measure_failures > 0:
            self.measure_failures -= 1
            raise DriverError("wmctrl geometry failed: connection refused")
        return self.achieved


class MissionWindowTests(unittest.TestCase):
    MISSIONS = (
        "first-time-exploration",
        "hover-affordance-sweep",
        "large-library-stress",
        "offline-recovery",
        "pointer-layout-reachability",
        "section-search-isolation",
    )

    def test_every_mission_declares_its_intended_window(self) -> None:
        for name in self.MISSIONS:
            with self.subTest(mission=name):
                mission = load_mission(EXPLORE_ROOT / "missions" / f"{name}.json")
                expected = (
                    {"width": 1200, "height": 800}
                    if name == "pointer-layout-reachability"
                    else {"width": 1600, "height": 1000}
                )
                self.assertEqual(mission.window, expected)

    def test_missing_window_remains_valid(self) -> None:
        mission = self._mission_json()
        mission.pop("window", None)

        loaded = self._load(mission)

        self.assertIsNone(loaded.window)

    def test_invalid_window_values_and_fields_are_rejected(self) -> None:
        invalid_windows = (
            {"width": 0, "height": 1000},
            {"width": "1600", "height": 1000},
            {"width": -1, "height": 1000},
            {"width": 1600, "height": 1000, "scale": 2},
        )
        for window in invalid_windows:
            with self.subTest(window=window), self.assertRaises(ContractError):
                mission = self._mission_json()
                mission["window"] = window
                self._load(mission)

    def test_window_bounds_are_inclusive(self) -> None:
        for window in (
            {"width": 600, "height": 400},
            {"width": 3840, "height": 2160},
        ):
            with self.subTest(window=window):
                mission = self._mission_json()
                mission["window"] = window
                self.assertEqual(self._load(mission).window, window)

    def _mission_json(self) -> dict:
        return json.loads(
            (EXPLORE_ROOT / "missions" / "first-time-exploration.json").read_text(
                encoding="utf-8"
            )
        )

    def _load(self, mission: dict):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "mission.json"
            path.write_text(json.dumps(mission), encoding="utf-8")
            return load_mission(path)


class WindowSetupTests(unittest.TestCase):
    def test_resize_is_measured_and_four_pixel_drift_is_not_honoured(self) -> None:
        transport = FakeTransport(WindowGeometry(20, 30, 1596, 1000))

        record = apply_window_size(
            transport,
            window_id=41,
            requested={"width": 1600, "height": 1000},
        )

        self.assertEqual(
            transport.calls,
            [("resize", 41, 1600, 1000), ("measure", 41)],
        )
        self.assertEqual(
            record,
            {
                "requested": {"width": 1600, "height": 1000},
                "achieved": {"width": 1596, "height": 1000},
                "honoured": False,
            },
        )

    def test_two_pixel_drift_is_honoured(self) -> None:
        transport = FakeTransport(WindowGeometry(0, 0, 1598, 1002))

        record = apply_window_size(
            transport,
            window_id=9,
            requested={"width": 1600, "height": 1000},
        )

        self.assertTrue(record["honoured"])

    def test_missing_request_does_not_touch_the_transport(self) -> None:
        transport = FakeTransport(WindowGeometry(0, 0, 1440, 900))

        record = apply_window_size(transport, window_id=9, requested=None)

        self.assertIsNone(record)
        self.assertEqual(transport.calls, [])

    def test_a_transient_measurement_failure_is_retried_not_fatal(self) -> None:
        transport = FakeTransport(WindowGeometry(0, 0, 1600, 1000), measure_failures=2)

        with mock.patch.object(window_setup.time, "sleep") as sleep:
            record = apply_window_size(
                transport,
                window_id=7,
                requested={"width": 1600, "height": 1000},
            )

        self.assertTrue(record["honoured"])
        self.assertEqual([call for call in transport.calls if call[0] == "measure"], [("measure", 7)] * 3)
        self.assertEqual(
            [call.args[0] for call in sleep.call_args_list],
            list(window_setup.MEASURE_RETRY_DELAYS_SECONDS),
        )

    def test_a_permanent_measurement_failure_degrades_to_a_finding(self) -> None:
        transport = FakeTransport(WindowGeometry(0, 0, 1600, 1000), measure_failures=99)

        with mock.patch.object(window_setup.time, "sleep"):
            record = apply_window_size(
                transport,
                window_id=7,
                requested={"width": 1600, "height": 1000},
            )

        self.assertIsNone(record["achieved"])
        self.assertFalse(record["honoured"])
        self.assertIn("wmctrl geometry failed", record["error"])

    def test_a_failed_resize_is_never_repeated_and_never_aborts(self) -> None:
        transport = FakeTransport(
            WindowGeometry(0, 0, 1440, 900),
            resize_error=DriverError("wmctrl resize failed: no such window"),
        )

        record = apply_window_size(
            transport,
            window_id=7,
            requested={"width": 1600, "height": 1000},
        )

        self.assertEqual(transport.calls, [("resize", 7, 1600, 1000)])
        self.assertFalse(record["honoured"])
        self.assertIn("wmctrl resize failed", record["error"])

    def test_a_hung_wmctrl_is_caught_like_any_other_transport_failure(self) -> None:
        transport = FakeTransport(
            WindowGeometry(0, 0, 1600, 1000),
            resize_error=subprocess.TimeoutExpired(["wmctrl"], 10),
        )

        record = apply_window_size(
            transport,
            window_id=7,
            requested={"width": 1600, "height": 1000},
        )

        self.assertIn("TimeoutExpired", record["error"])

    def test_display_canvas_has_room_for_the_declared_window(self) -> None:
        run_script = (EXPLORE_ROOT / "run.sh").read_text(encoding="utf-8")

        self.assertIn(
            'cua_common_start_display "$output_dir" "$scratch_root" "1920x1200x24"',
            run_script,
        )


class RunnerWindowWiringTests(unittest.TestCase):
    def test_launch_orders_resize_measure_origin_and_executor(self) -> None:
        events = []
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "first-time-exploration.json"
        )

        class Lifecycle:
            def start(self):
                events.append("start")
                return 12, 34, 1

        class Transport:
            def resize_window(self, window_id, width, height):
                events.append(("resize", window_id, width, height))

            def wmctrl_geometry(self, window_id):
                events.append(("wmctrl", window_id))
                return WindowGeometry(0, 0, 1596, 1000)

        class Report:
            def __init__(self):
                self.setup = None
                self.findings = []

            def set_window_setup(self, record):
                self.setup = record

            def add_finding(self, finding):
                self.findings.append(finding)

        report = Report()
        args = SimpleNamespace(session="session", evidence_dir=pathlib.Path("evidence"))
        with mock.patch.object(
            runner,
            "resolve_window_origin",
            side_effect=lambda *_args, **_kwargs: events.append("origin")
            or WindowGeometry(0, 0, 1596, 1000),
        ), mock.patch.object(
            runner,
            "make_geometry_provider",
            return_value=lambda: (),
        ), mock.patch.object(
            runner,
            "CuaExecutor",
            side_effect=lambda *_args, **_kwargs: events.append("executor") or object(),
        ):
            runner.launch_executor(
                Lifecycle().start,
                Transport(),
                mission=mission,
                args=args,
                report=report,
            )

        self.assertEqual(
            events,
            [
                "start",
                ("resize", 34, 1600, 1000),
                ("wmctrl", 34),
                "origin",
                "executor",
            ],
        )
        self.assertFalse(report.setup["honoured"])
        self.assertEqual(report.findings[0]["code"], "window-size-not-honoured")

    def test_missing_window_skips_only_the_setup_resize(self) -> None:
        events = []
        mission = dataclasses.replace(
            load_mission(EXPLORE_ROOT / "missions" / "first-time-exploration.json"),
            window=None,
        )

        class Transport:
            def resize_window(self, *_args):
                events.append("resize")

            def wmctrl_geometry(self, _window_id):
                events.append("wmctrl")
                return WindowGeometry(0, 0, 1440, 900)

        report = mock.Mock()
        args = SimpleNamespace(session="session", evidence_dir=pathlib.Path("evidence"))
        with mock.patch.object(
            runner, "resolve_window_origin", return_value=WindowGeometry(0, 0, 1440, 900)
        ), mock.patch.object(
            runner, "make_geometry_provider", return_value=lambda: ()
        ), mock.patch.object(runner, "CuaExecutor", return_value=object()):
            runner.launch_executor(
                lambda: (12, 34, 1),
                Transport(),
                mission=mission,
                args=args,
                report=report,
            )

        self.assertEqual(events, [])
        report.set_window_setup.assert_called_once_with(None)


if __name__ == "__main__":
    unittest.main()
