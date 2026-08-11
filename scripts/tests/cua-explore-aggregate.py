#!/usr/bin/env python3
"""Regression tests for the exploratory aggregate report.

They run against the checked-in shortened copies of two real 2026-08-10 night
summaries (`scripts/tests/fixtures/summary-2026-08-10-*.json`) and, for the
loading half, against a temporary evidence tree those copies are placed in — so
the glob, the parse and the trajectory fallback are exercised, not only the
in-memory record shapes.
"""

from __future__ import annotations

import json
import pathlib
import shutil
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
FIXTURES = pathlib.Path(__file__).resolve().parent / "fixtures"
sys.path.insert(0, str(EXPLORE_ROOT))

from aggregate_report import (  # noqa: E402
    RunRecord,
    discover_runs,
    group_findings,
    health_line,
    load_run,
    main,
)


FIXTURE_164_OF_168 = FIXTURES / "summary-2026-08-10-first-time-exploration-seed-11.json"
FIXTURE_173_OF_177 = FIXTURES / "summary-2026-08-10-section-search-isolation-seed-11.json"


def load_fixture(path: pathlib.Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


RECORDED_164_OF_168 = load_fixture(FIXTURE_164_OF_168)
RECORDED_173_OF_177 = load_fixture(FIXTURE_173_OF_177)


def main_report(root: pathlib.Path) -> str:
    output = root / "report.md"
    assert main([str(root), "--output", str(output)]) == 0
    return output.read_text(encoding="utf-8")


def record(mission: str, seed: int, *findings: dict) -> RunRecord:
    return RunRecord(
        pathlib.Path(f"{mission}-{seed}"),
        {"mission_id": mission, "seed": seed, "outcome": "incomplete"},
        findings,
    )


def finding(code: str, target: str) -> dict:
    return {"code": code, "evidence": {"target": target}}


class GeometryHealthTests(unittest.TestCase):
    def test_real_summaries_use_driver_elements_as_the_denominator(self) -> None:
        self.assertIn("geometry=164/168", health_line(RECORDED_164_OF_168))
        self.assertIn("geometry=173/177", health_line(RECORDED_173_OF_177))
        self.assertNotIn("1129", health_line(RECORDED_164_OF_168))

    def test_unrelated_integer_fields_can_never_change_the_denominator(self) -> None:
        summary = {
            "mission_id": "regression",
            "seed": 1,
            "geometry_resolution": {
                "resolved": 7,
                "driver_elements": 9,
                "walk_nodes": 10_000,
                "resolved_unique": 2_000,
                "resolved_ordered": 3_000,
            },
        }

        self.assertIn("geometry=7/9", health_line(summary))

    def test_a_clean_restart_cannot_hide_a_blind_first_generation(self) -> None:
        # The B3 failure scenario: generation 1 measured nothing, generation 2
        # was clean. Reporting the last executor alone called the run healthy.
        summary = {
            "mission_id": "restart",
            "seed": 11,
            "geometry_measurements": [
                {
                    "generation": 1,
                    "state_id": "launch-1-state-1",
                    "trusted": False,
                    "failure": "accessibility walk failed",
                },
                {
                    "generation": 2,
                    "state_id": "launch-2-state-1",
                    "trusted": True,
                    "resolution": {"resolved": 164, "driver_elements": 168},
                },
            ],
        }

        line = health_line(summary)

        self.assertIn("geometry=1/2 snapshots trusted", line)
        self.assertIn("geometry_positions=164/168", line)

    def test_health_reports_new_run_signals(self) -> None:
        summary = {
            **RECORDED_164_OF_168,
            "outcome": "aborted",
            "abort_reason": "app exited",
            "transport_faults": 2,
            "unknown_action_names": {"foo.bar": 3},
            "oracle_activity": {
                "feedback": {"evaluated": 4, "fired": 1},
                "layout": {"evaluated": 0, "fired": 0},
            },
        }

        line = health_line(summary)

        self.assertIn("outcome=aborted", line)
        self.assertIn("abort=app exited", line)
        self.assertIn("transport_faults=2", line)
        self.assertIn("unknown_actions=3", line)
        self.assertIn("oracles=4 evaluated/1 fired/2 declared", line)


class ReproducibilityTests(unittest.TestCase):
    def test_groups_sort_reproduced_findings_before_singletons(self) -> None:
        repeated = finding("no-accessible-action", "Music")
        runs = [
            record("first", 11, repeated, finding("one-off", "Retry")),
            record("first", 29, repeated),
            record("second", 11, repeated),
            record("second", 29, repeated),
        ]

        groups = group_findings(runs)

        self.assertEqual(
            (groups[0]["code"], groups[0]["target"]),
            ("no-accessible-action", "Music"),
        )
        self.assertEqual(
            (groups[0]["runs"], groups[0]["missions"], groups[0]["seeds"]),
            (4, 2, 2),
        )
        self.assertEqual(groups[-1]["runs"], 1)

    def test_repeated_occurrences_in_one_run_count_as_one_run(self) -> None:
        duplicate = finding("ambiguous-accessible-name", "☆")

        groups = group_findings([record("stars", 11, duplicate, duplicate)])

        self.assertEqual(groups[0]["runs"], 1)
        self.assertEqual(groups[0]["occurrences"], 2)


class EvidenceTreeTests(unittest.TestCase):
    """The glob/load/parse path, driven over a real directory tree."""

    def setUp(self) -> None:
        self.root = pathlib.Path(tempfile.mkdtemp(prefix="cua-aggregate-"))
        self.addCleanup(shutil.rmtree, self.root, True)

    def run_dir(self, name: str) -> pathlib.Path:
        path = self.root / name
        path.mkdir()
        return path

    def place(self, fixture: pathlib.Path, name: str) -> pathlib.Path:
        directory = self.run_dir(name)
        shutil.copyfile(fixture, directory / "summary.json")
        return directory

    def truncate(self, fixture: pathlib.Path, name: str) -> pathlib.Path:
        """A summary.json whose writer died mid-flush - seen in night runs."""
        directory = self.run_dir(name)
        text = fixture.read_text(encoding="utf-8")
        (directory / "summary.json").write_text(text[: len(text) // 2], encoding="utf-8")
        return directory

    def test_report_reads_the_recorded_geometry_from_disk(self) -> None:
        self.place(FIXTURE_164_OF_168, "first-time-exploration-seed-11")
        self.place(FIXTURE_173_OF_177, "section-search-isolation-seed-11")
        output = self.root / "report.md"

        self.assertEqual(main([str(self.root), "--output", str(output)]), 0)

        report = output.read_text(encoding="utf-8")
        self.assertIn("Runs: 2", report)
        self.assertIn("geometry=164/168", report)
        self.assertIn("geometry=173/177", report)
        self.assertNotIn("1129", report)

    def test_a_truncated_summary_becomes_a_named_gap_not_a_traceback(self) -> None:
        self.place(FIXTURE_164_OF_168, "first-time-exploration-seed-11")
        self.place(FIXTURE_173_OF_177, "section-search-isolation-seed-11")
        self.truncate(FIXTURE_164_OF_168, "first-time-exploration-seed-29")
        output = self.root / "report.md"

        self.assertEqual(main([str(self.root), "--output", str(output)]), 0)

        report = output.read_text(encoding="utf-8")
        self.assertIn("Unreadable runs: 1", report)
        self.assertIn("first-time-exploration-seed-29", report)
        self.assertIn("Runs: 2", report)
        self.assertIn("geometry=164/168", report)
        self.assertIn("geometry=173/177", report)

    def test_a_broken_trajectory_costs_one_run_not_the_report(self) -> None:
        self.place(FIXTURE_164_OF_168, "first-time-exploration-seed-11")
        broken = self.place(FIXTURE_173_OF_177, "section-search-isolation-seed-11")
        (broken / "trajectory.jsonl").write_text('{"step": 0, "findings"\n', encoding="utf-8")

        discovery = discover_runs(self.root)

        self.assertEqual([run.path.name for run in discovery.runs], ["first-time-exploration-seed-11"])
        self.assertEqual([gap.path.name for gap in discovery.gaps], ["section-search-isolation-seed-11"])
        self.assertIn("trajectory.jsonl:1", discovery.gaps[0].reason)

    def test_findings_fall_back_to_the_trajectory_when_the_summary_has_none(self) -> None:
        # Recorded shape from section-search-isolation seed 11, step 0.
        directory = self.place(FIXTURE_173_OF_177, "section-search-isolation-seed-11")
        (directory / "trajectory.jsonl").write_text(
            json.dumps(
                {
                    "step": 0,
                    "findings": [
                        {
                            "code": "slow-visible-feedback",
                            "severity": "warning",
                            "blocks_gate": False,
                            "evidence": {"first_change_ms": 4253},
                        }
                    ],
                }
            )
            + "\n",
            encoding="utf-8",
        )

        record = load_run(directory / "summary.json")

        self.assertEqual([item["code"] for item in record.findings], ["slow-visible-feedback"])
        self.assertIn("`slow-visible-feedback`", main_report(self.root))

    def test_embedded_summary_findings_win_over_the_trajectory(self) -> None:
        directory = self.run_dir("embedded-seed-11")
        summary = {**RECORDED_173_OF_177, "findings": [{"code": "from-summary"}]}
        (directory / "summary.json").write_text(json.dumps(summary), encoding="utf-8")
        (directory / "trajectory.jsonl").write_text(
            json.dumps({"step": 0, "findings": [{"code": "from-trajectory"}]}) + "\n",
            encoding="utf-8",
        )

        record = load_run(directory / "summary.json")

        self.assertEqual([item["code"] for item in record.findings], ["from-summary"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
