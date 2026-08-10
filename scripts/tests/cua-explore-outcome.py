#!/usr/bin/env python3
"""Outcome and exit-code tests for exploratory mission reports."""

from __future__ import annotations

import pathlib
import sys
import tempfile
import unittest
from unittest import mock


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
sys.path.insert(0, str(EXPLORE_ROOT))

import runner  # noqa: E402
from protocol import ActionGateway, load_mission  # noqa: E402
from report import RunReport  # noqa: E402


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


if __name__ == "__main__":
    unittest.main()
