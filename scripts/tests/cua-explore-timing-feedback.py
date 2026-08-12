#!/usr/bin/env python3
"""Production-path regressions for waiting-feedback timing evidence."""

from __future__ import annotations

import os
import pathlib
import sys
import time
import unittest
from unittest import mock


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "scripts" / "cua-explore"))

from actions import ScrollAction, WaitAction  # noqa: E402
from driver import CuaExecutor  # noqa: E402
from hover_geometry import WindowGeometry  # noqa: E402
from oracles import ActionEvidence  # noqa: E402


SNAPSHOT_ID = "s00000001"
REAL_SLEEP = time.sleep
UNDELIVERED = {
    "effect": "unverifiable",
    "escalation": {"reason": "delivery_failed", "target": "foreground"},
}


def raw_snapshot(*, role: str = "button", actions: tuple[str, ...] = ("click",)):
    return {
        "snapshot_id": SNAPSHOT_ID,
        "screenshot_width": 800,
        "screenshot_height": 600,
        "structuredContent": {
            "elements": [
                {
                    "element_index": 1,
                    "element_token": f"{SNAPSHOT_ID}:1",
                    "label": "Target",
                    "role": role,
                    "frame": {"x": 20, "y": 20, "w": 120, "h": 40},
                    "actions": list(actions),
                    "enabled": True,
                }
            ]
        },
    }


class TimingTransport:
    def __init__(self, snapshots, *, response=None) -> None:
        self.snapshots = list(snapshots)
        self.response = response or {"effect": "confirmed", "verified": True}

    def call(self, tool, payload):
        if tool == "get_window_state":
            return self.snapshots.pop(0)
        return dict(self.response)

    def resize_window(self, window_id, width, height):
        return {"effect": "confirmed", "verified": True}

    def set_connectivity(self, state):
        return {"effect": "confirmed", "verified": True}


class TimingFeedbackTests(unittest.TestCase):
    def executor(self, snapshots, *, response=None, settle_delays=(0.0,)):
        return CuaExecutor(
            TimingTransport(snapshots, response=response),
            pid=os.getpid(),
            window_id=7,
            session="timing-feedback",
            settle_delays=settle_delays,
            window_origin=WindowGeometry(0, 0, 800, 600),
        )

    def run_slow_unchanged_activation(self, *, role, actions, dispatch):
        snapshot_count = 4 if dispatch == "px" else 3
        executor = self.executor(
            [raw_snapshot(role=role, actions=actions)] * snapshot_count
        )
        with mock.patch(
            "driver.time.sleep", side_effect=lambda _delay: REAL_SLEEP(0.8)
        ):
            return executor.execute_evidence(
                ActionEvidence.activate("Target", dispatch=dispatch)
            )

    def test_explained_activation_failures_do_not_repeat_as_waiting_failures(self):
        scenarios = (
            ("cell", (), "ax", "no-accessible-action"),
            ("button", ("click",), "px", "click-no-visible-effect"),
            ("button", ("click",), "ax", "suspected-no-handler"),
        )
        for role, actions, dispatch, explanation in scenarios:
            with self.subTest(explanation=explanation):
                result = self.run_slow_unchanged_activation(
                    role=role, actions=actions, dispatch=dispatch
                )
                codes = {finding.code for finding in result.findings}
                self.assertIn(explanation, codes)
                self.assertNotIn("missing-waiting-feedback", codes)

    def test_undelivered_action_cannot_create_a_waiting_failure(self) -> None:
        snapshots = [raw_snapshot()] * 3
        executor = self.executor(snapshots, response=UNDELIVERED)
        with mock.patch(
            "driver.time.sleep", side_effect=lambda _delay: REAL_SLEEP(0.8)
        ):
            result = executor.execute_evidence(ActionEvidence.activate("Target"))

        codes = {finding.code for finding in result.findings}
        self.assertIn("driver-action-undelivered", codes)
        self.assertNotIn("missing-waiting-feedback", codes)

    def test_scroll_does_not_require_a_state_signature_change(self) -> None:
        snapshots = [raw_snapshot()] * 3
        executor = self.executor(snapshots)
        with mock.patch(
            "driver.time.sleep", side_effect=lambda _delay: REAL_SLEEP(0.8)
        ):
            result = executor.execute(
                ScrollAction("state-1", "down", 1, "page")
            )

        self.assertEqual(result.evidence.expect_effect, "none")
        self.assertEqual(result.findings, ())

    def test_explicit_status_wait_still_reports_missing_feedback(self) -> None:
        snapshots = [raw_snapshot()] * 2
        executor = self.executor(snapshots, settle_delays=())

        result = executor.execute(WaitAction("state-1", 1, True))

        self.assertIn(
            "missing-waiting-feedback", {finding.code for finding in result.findings}
        )

    def test_dispatch_round_trip_is_part_of_reported_harness_time(self) -> None:
        snapshots = [raw_snapshot()] * 2
        executor = self.executor(snapshots, settle_delays=())

        result = executor.execute(WaitAction("state-1", 30, True))

        waiting = next(
            finding
            for finding in result.findings
            if finding.code == "missing-waiting-feedback"
        )
        expected_harness_ms = (
            result.evidence.elapsed_ms
            + result.evidence.settle_delay_ms
            + sum(result.evidence.snapshot_ms)
        )
        self.assertEqual(waiting.evidence["dispatch_ms"], result.evidence.elapsed_ms)
        self.assertEqual(waiting.evidence["harness_ms"], expected_harness_ms)


if __name__ == "__main__":
    unittest.main(verbosity=2)
