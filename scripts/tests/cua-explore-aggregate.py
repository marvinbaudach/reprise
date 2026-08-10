#!/usr/bin/env python3
"""Regression tests for the checked-in exploratory aggregate report."""

from __future__ import annotations

import pathlib
import sys
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
sys.path.insert(0, str(EXPLORE_ROOT))

from aggregate_report import RunRecord, group_findings, health_line  # noqa: E402


# Exact shortened summary fragments from the real 2026-08-10 night run. They
# live here because Strom I may add only this test file, not more fixture files.
RECORDED_164_OF_168 = {
    "mission_id": "first-time-exploration",
    "seed": 11,
    "outcome": "incomplete",
    "transport_faults": 0,
    "unknown_action_names": {},
    "oracle_activity": {"feedback": {"evaluated": 4, "fired": 1}},
    "geometry_resolution": {
        "driver_elements": 168,
        "walk_nodes": 467,
        "resolved": 164,
        "resolved_unique": 61,
        "resolved_ordered": 103,
        "with_action": 161,
        "unmatched": 3,
        "ambiguous": 1,
        "walk_surplus": 1,
        "resolved_ratio": 0.9762,
    },
}
RECORDED_173_OF_177 = {
    "mission_id": "hover-affordance-sweep",
    "seed": 11,
    "outcome": "incomplete",
    "geometry_resolution": {
        "driver_elements": 177,
        "walk_nodes": 479,
        "resolved": 173,
        "resolved_unique": 62,
        "resolved_ordered": 111,
        "with_action": 170,
        "unmatched": 3,
        "ambiguous": 1,
        "walk_surplus": 1,
        "resolved_ratio": 0.9774,
    },
}


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


if __name__ == "__main__":
    unittest.main(verbosity=2)
