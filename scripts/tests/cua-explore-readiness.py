#!/usr/bin/env python3
"""The runner must wait for a usable accessibility tree, not just a window."""

from __future__ import annotations

import json
import pathlib
import sys
import tempfile
import unittest
from unittest import mock


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
sys.path.insert(0, str(EXPLORE_ROOT))

import runner  # noqa: E402
from runner import (  # noqa: E402
    AppLifecycle,
    RunError,
    accessibility_tree_ready,
    startup_timeout_seconds,
    wait_for_accessibility_tree,
)


WINDOW_ID = 0x1400007

DEGRADED_REASON = "x11_property_fallback_partial: at-spi registry has no application"


def degraded_state() -> dict:
    """What cua-driver returns before the AT-SPI bridge registers itself."""
    return {
        "degraded": True,
        "degraded_reason": DEGRADED_REASON,
        "element_count": 1,
        "screenshot_width": 1280,
        "screenshot_height": 800,
        "structuredContent": {
            "elements": [{"label": "Reprise", "role": "window", "frame": {}}]
        },
    }


def usable_state(element_count: int = 169) -> dict:
    return {
        "degraded": None,
        "element_count": element_count,
        "screenshot_width": 1280,
        "screenshot_height": 800,
        "structuredContent": {
            "elements": [
                {"label": f"Element {index}", "role": "button", "frame": {}}
                for index in range(element_count)
            ]
        },
    }


def lonely_state() -> dict:
    """Not flagged degraded, but still only the bare window element."""
    state = usable_state(1)
    return state


class ScriptedTransport:
    """Serves list_windows immediately and get_window_state from a script."""

    def __init__(self, states) -> None:
        self.states = list(states)
        self.calls: list[str] = []

    def call(self, tool, payload):
        self.calls.append(tool)
        if tool == "list_windows":
            return {"windows": [{"window_id": WINDOW_ID, "title": "Reprise"}]}
        if tool == "get_window_state":
            if len(self.states) > 1:
                return self.states.pop(0)
            return self.states[0]
        return {"effect": "confirmed", "verified": True}

    @property
    def state_calls(self) -> int:
        return self.calls.count("get_window_state")

    def resize_window(self, window_id, width, height):  # pragma: no cover - unused
        raise AssertionError("readiness must not resize the window")

    def set_connectivity(self, state):  # pragma: no cover - unused
        raise AssertionError("readiness must not touch connectivity")

    def wmctrl_geometry(self, window_id):  # pragma: no cover - unused
        raise AssertionError("readiness must not query geometry")


class RecordingClock:
    """A monotonic clock that only advances when the caller sleeps."""

    def __init__(self) -> None:
        self.now = 0.0
        self.sleeps: list[float] = []

    def monotonic(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


class AccessibilityReadinessTests(unittest.TestCase):
    def _wait(self, transport, clock, **overrides):
        arguments = {
            "pid": 4242,
            "window_id": WINDOW_ID,
            "session": "readiness",
            "timeout_seconds": 60.0,
            "poll_seconds": 0.25,
            "monotonic": clock.monotonic,
            "sleep": clock.sleep,
        }
        arguments.update(overrides)
        return wait_for_accessibility_tree(transport, **arguments)

    def test_a_degraded_single_element_snapshot_is_not_usable(self) -> None:
        self.assertFalse(accessibility_tree_ready(degraded_state()))

    def test_a_lone_window_element_is_not_usable_even_without_the_degraded_flag(
        self,
    ) -> None:
        self.assertFalse(accessibility_tree_ready(lonely_state()))

    def test_a_full_tree_is_usable(self) -> None:
        self.assertTrue(accessibility_tree_ready(usable_state()))

    def test_waiting_polls_until_the_bridge_publishes_the_tree(self) -> None:
        transport = ScriptedTransport(
            [degraded_state(), degraded_state(), degraded_state(), usable_state()]
        )
        clock = RecordingClock()

        state = self._wait(transport, clock)

        self.assertTrue(accessibility_tree_ready(state))
        self.assertEqual(transport.state_calls, 4)
        self.assertEqual(clock.sleeps, [0.25, 0.25, 0.25])

    def test_waiting_keeps_polling_past_a_lone_window_element(self) -> None:
        transport = ScriptedTransport([lonely_state(), usable_state()])
        clock = RecordingClock()

        state = self._wait(transport, clock)

        self.assertEqual(state["element_count"], 169)
        self.assertEqual(transport.state_calls, 2)

    def test_the_deadline_reports_the_driver_reason_and_the_element_count(self) -> None:
        transport = ScriptedTransport([degraded_state()])
        clock = RecordingClock()

        with self.assertRaises(RunError) as caught:
            self._wait(transport, clock, timeout_seconds=1.0)

        message = str(caught.exception)
        self.assertIn("the accessibility tree never became available", message)
        self.assertIn(DEGRADED_REASON, message)
        self.assertIn("element_count=1", message)

    def test_a_dead_application_aborts_the_wait_immediately(self) -> None:
        transport = ScriptedTransport([degraded_state()])
        clock = RecordingClock()

        with self.assertRaisesRegex(RunError, "exited"):
            self._wait(transport, clock, is_alive=lambda: False)

        self.assertEqual(transport.state_calls, 0)


class FakeProcess:
    def __init__(self, pid: int = 9911) -> None:
        self.pid = pid
        self.terminated = False

    def poll(self):
        return None

    def terminate(self) -> None:
        self.terminated = True

    def wait(self, timeout=None):
        return 0

    def kill(self) -> None:  # pragma: no cover - only on a stuck process
        self.terminated = True


class AppLifecycleReadinessTests(unittest.TestCase):
    def _lifecycle(self, transport, root: pathlib.Path) -> AppLifecycle:
        return AppLifecycle(
            app_binary=root / "reprise",
            profile_root=root,
            evidence_dir=root / "evidence",
            connectivity_file=root / "connectivity.state",
            quit_delay_seconds=30,
            transport=transport,
            session="readiness",
            ready_timeout_seconds=5.0,
            ready_poll_seconds=0.0,
        )

    def test_start_returns_only_after_the_tree_is_usable(self) -> None:
        transport = ScriptedTransport(
            [degraded_state(), degraded_state(), usable_state()]
        )
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "evidence").mkdir()
            lifecycle = self._lifecycle(transport, root)
            with mock.patch.object(
                runner.subprocess, "Popen", return_value=FakeProcess()
            ):
                pid, window_id, generation = lifecycle.start()
                self.assertEqual((pid, window_id, generation), (9911, WINDOW_ID, 1))
                self.assertEqual(transport.state_calls, 3)
                lifecycle.stop()

    def test_restart_waits_for_the_tree_again(self) -> None:
        transport = ScriptedTransport(
            [
                usable_state(),
                degraded_state(),
                degraded_state(),
                usable_state(),
            ]
        )
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "evidence").mkdir()
            lifecycle = self._lifecycle(transport, root)
            with mock.patch.object(
                runner.subprocess, "Popen", side_effect=lambda *a, **k: FakeProcess()
            ):
                lifecycle.start()
                self.assertEqual(transport.state_calls, 1)
                lifecycle.restart()
                self.assertEqual(transport.state_calls, 4)
                lifecycle.stop()

    def test_start_surfaces_the_deadline_as_a_run_error(self) -> None:
        transport = ScriptedTransport([degraded_state()])
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "evidence").mkdir()
            lifecycle = self._lifecycle(transport, root)
            lifecycle.ready_timeout_seconds = 0.0
            with mock.patch.object(
                runner.subprocess, "Popen", return_value=FakeProcess()
            ):
                with self.assertRaisesRegex(
                    RunError, "the accessibility tree never became available"
                ):
                    lifecycle.start()
                lifecycle.stop()


class StartupTimeoutTests(unittest.TestCase):
    def test_the_hundred_thousand_row_profile_gets_far_more_than_thirty_seconds(
        self,
    ) -> None:
        # The 100k profile needed longer than the hard-wired 30 seconds and the
        # run aborted at zero steps, which read as a harness failure.
        self.assertGreaterEqual(startup_timeout_seconds("stress-100k"), 600.0)

    def test_a_small_profile_still_gets_a_generous_allowance(self) -> None:
        self.assertGreaterEqual(startup_timeout_seconds("mixed-128"), 120.0)

    def test_the_allowance_grows_with_the_library_size(self) -> None:
        self.assertGreater(
            startup_timeout_seconds("stress-100k"),
            startup_timeout_seconds("stress-10k"),
        )
        self.assertGreater(
            startup_timeout_seconds("stress-10k"),
            startup_timeout_seconds("mixed-128"),
        )

    def test_an_unknown_profile_falls_back_instead_of_failing(self) -> None:
        self.assertGreaterEqual(startup_timeout_seconds("not-a-profile"), 120.0)


class StartupTimingEvidenceTests(unittest.TestCase):
    def _lifecycle(self, transport, root: pathlib.Path, **overrides) -> AppLifecycle:
        arguments = {
            "app_binary": root / "reprise",
            "profile_root": root,
            "evidence_dir": root / "evidence",
            "connectivity_file": root / "connectivity.state",
            "quit_delay_seconds": 30,
            "transport": transport,
            "session": "readiness",
            "ready_timeout_seconds": 5.0,
            "ready_poll_seconds": 0.0,
            "window_timeout_seconds": 5.0,
            "window_poll_seconds": 0.0,
        }
        arguments.update(overrides)
        return AppLifecycle(**arguments)

    def test_every_launch_records_how_long_the_window_and_tree_took(self) -> None:
        transport = ScriptedTransport([usable_state()])
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "evidence").mkdir()
            lifecycle = self._lifecycle(transport, root)
            with mock.patch.object(
                runner.subprocess, "Popen", side_effect=lambda *a, **k: FakeProcess()
            ):
                lifecycle.start()
                lifecycle.restart()
                lifecycle.stop()

        self.assertEqual(
            [timing["launch"] for timing in lifecycle.startup_timings], [1, 2]
        )
        for timing in lifecycle.startup_timings:
            self.assertGreaterEqual(timing["window_ms"], 0)
            self.assertGreaterEqual(
                timing["accessibility_tree_ms"], timing["window_ms"]
            )

    def test_a_window_that_never_appears_names_the_waited_time_and_the_cap(
        self,
    ) -> None:
        class WindowlessTransport(ScriptedTransport):
            def call(self, tool, payload):
                if tool == "list_windows":
                    self.calls.append(tool)
                    return {"windows": []}
                return super().call(tool, payload)

        transport = WindowlessTransport([usable_state()])
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "evidence").mkdir()
            lifecycle = self._lifecycle(transport, root, window_timeout_seconds=0.0)
            with mock.patch.object(
                runner.subprocess, "Popen", return_value=FakeProcess()
            ):
                with self.assertRaisesRegex(RunError, "did not expose a CUA window"):
                    lifecycle.start()
                lifecycle.stop()


class StartupTimingReportTests(unittest.TestCase):
    def test_the_summary_carries_the_measured_startup_times(self) -> None:
        from report import RunReport

        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory)
            report = RunReport(
                output,
                mission_id="first-time-exploration",
                profile="mixed-128",
                seed=1,
                commit="abc123",
            )
            report.set_startup_timings(
                [{"launch": 1, "window_ms": 4200, "accessibility_tree_ms": 12800}]
            )

            summary = report.write()

            self.assertEqual(
                summary["startup_timings"],
                [{"launch": 1, "window_ms": 4200, "accessibility_tree_ms": 12800}],
            )
            written = json.loads((output / "summary.json").read_text(encoding="utf-8"))
            self.assertEqual(
                written["startup_timings"][0]["accessibility_tree_ms"], 12800
            )
            self.assertIn("12800", (output / "report.md").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main(verbosity=1)
