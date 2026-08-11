#!/usr/bin/env python3
"""Production-path regression for run-wide screenshot degradation."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
FIXTURE = (
    REPO_ROOT
    / "scripts"
    / "tests"
    / "fixtures"
    / "night-2026-08-10-ambiguous-cells.json"
)
sys.path.insert(0, str(EXPLORE_ROOT))

from actions import HoverAction  # noqa: E402
from driver import CliTransport, CuaExecutor  # noqa: E402
from driver_transport import CAPTURE_RETRY_DELAYS_SECONDS  # noqa: E402
from hover_geometry import WindowGeometry  # noqa: E402


CAPTURE_ERROR = (
    "Capture error: window screenshot failed for window 8388613:\n"
    "all Linux window capture backends failed\n"
    "- XShm: MIT-SHM capture failed after reconnect for DISPLAY=:2"
)


def completed(stdout: str) -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess([], 0, stdout, "")


class PersistentCaptureFailureTransport(CliTransport):
    """Keep the image backend broken while the accessibility tree stays healthy."""

    def __init__(self, *, evidence_dir: pathlib.Path) -> None:
        super().__init__(evidence_dir=evidence_dir)
        self.commands: list[list[str]] = []
        self.snapshot = FIXTURE.read_text(encoding="utf-8")

    def _run(self, command):
        self.commands.append(list(command))
        tool = command[1]
        if tool == "get_window_state":
            payload = json.loads(command[2])
            return completed(
                CAPTURE_ERROR if "screenshot_out_file" in payload else self.snapshot
            )
        if tool == "set_agent_cursor_enabled":
            return completed('{"enabled": false}')
        return completed('{"effect": "unverifiable"}')


class CaptureDegradationTests(unittest.TestCase):
    def test_one_exhausted_capture_budget_degrades_every_later_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = pathlib.Path(directory)
            transport = PersistentCaptureFailureTransport(evidence_dir=evidence_dir)
            executor = CuaExecutor(
                transport,
                pid=1,
                window_id=8388613,
                session="capture-fault",
                evidence_dir=evidence_dir / "states",
                hover_geometry=WindowGeometry(0, 0, 1200, 800),
            )

            with mock.patch("driver.time.sleep"), mock.patch(
                "driver_transport.time.sleep"
            ) as retry_sleep:
                result = executor.execute_hover(
                    HoverAction("state-1", "Writable Batch 0001")
                )

            screenshot_calls = [
                json.loads(command[2])
                for command in transport.commands
                if command[1] == "get_window_state"
            ]
            retry_delays = [
                call.args[0]
                for call in retry_sleep.call_args_list
                if call.args[0] in CAPTURE_RETRY_DELAYS_SECONDS
            ]
            before = json.loads(
                (evidence_dir / "states" / "step-0001-hover-before.json").read_text(
                    encoding="utf-8"
                )
            )
            after = json.loads(
                (evidence_dir / "states" / "step-0001-hover-after.json").read_text(
                    encoding="utf-8"
                )
            )
            png_files = list((evidence_dir / "states").glob("*.png"))

        self.assertEqual(len(screenshot_calls), 6)
        self.assertEqual(retry_delays, list(CAPTURE_RETRY_DELAYS_SECONDS))
        self.assertNotIn("screenshot_out_file", screenshot_calls[4])
        self.assertNotIn("screenshot_out_file", screenshot_calls[5])
        self.assertFalse(result.before.screenshot_available)
        self.assertFalse(result.after.screenshot_available)
        self.assertFalse(png_files)
        self.assertEqual(
            before["screenshot_unavailable_reason"], CAPTURE_ERROR.splitlines()[0]
        )
        self.assertEqual(
            after["screenshot_unavailable_reason"], CAPTURE_ERROR.splitlines()[0]
        )
        codes = {finding.code for finding in result.findings}
        self.assertIn("driver-transport-fault", codes)
        self.assertIn("hover-skipped", codes)
        self.assertNotIn("hover-affordance-missing", codes)
        self.assertNotIn("hover-affordance-weak", codes)


if __name__ == "__main__":
    unittest.main(verbosity=2)
