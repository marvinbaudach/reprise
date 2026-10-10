#!/usr/bin/env python3
"""The sort and batch-edit audits: a click credits its own column, the anchor is the title.

The row labels are the 2026-10-10 stress rerun's: a label is every cell, the year
among them, and the batch edit rewrites the year.
"""

import dataclasses
import pathlib
import sys
import unittest

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
sys.path.insert(0, str(EXPLORE_ROOT))

from protocol import load_mission  # noqa: E402
from workload_audit import ActionTrace, audit_action_workload  # noqa: E402


class BatchAndSortAuditTests(unittest.TestCase):
    def setUp(self) -> None:
        self.mission = load_mission(EXPLORE_ROOT / "missions" / "large-library-stress.json")

    def test_sort_checkpoint_credits_no_column_for_a_click_on_the_header_row(self) -> None:
        # The 2026-10-10 rerun: 24 clicks on the one label of the whole header
        # row, which lands on its middle, credited all five columns.
        workload = self.mission.workloads[1]
        row_clicks = [
            ActionTrace(
                action={
                    "kind": "activate",
                    "target_label": "Title Artist Album Year Length Rating",
                },
                before_rows=((f"Before {index}", 100.0),),
                after_rows=((f"After {index}", 100.0),),
                state_changed=True,
            )
            for index in range(24)
        ]

        result = audit_action_workload(1, workload, row_clicks)

        self.assertFalse(result["complete"])
        self.assertEqual(result["covered_columns"], [])
        self.assertEqual(result["matching_actions"], 0)

    def test_sort_checkpoint_credits_a_column_only_for_its_own_header(self) -> None:
        workload = self.mission.workloads[1]
        # Rows changed on every click, and "Artist" is the only header clicked.
        artist_only = [
            ActionTrace(
                action={"kind": "activate", "target_label": "Artist column header"},
                before_rows=((f"Before {index}", 100.0),),
                after_rows=((f"After {index}", 100.0),),
                state_changed=True,
            )
            for index in range(24)
        ]

        result = audit_action_workload(1, workload, artist_only)

        self.assertFalse(result["complete"])
        self.assertEqual(result["covered_columns"], ["artist"])
        self.assertEqual(result["clicks_per_column"]["artist"], 24)
        self.assertEqual(result["clicks_per_column"]["title"], 0)

    def test_sort_checkpoint_does_not_count_a_facet_named_like_a_column(self) -> None:
        workload = self.mission.workloads[1]
        facet_clicks = [
            ActionTrace(
                action={"kind": "activate", "target_label": "Year"},
                before_rows=((f"Before {index}", 100.0),),
                after_rows=((f"After {index}", 100.0),),
                state_changed=True,
            )
            for index in range(24)
        ]

        self.assertEqual(
            audit_action_workload(1, workload, facet_clicks)["matching_actions"], 0
        )

    @staticmethod
    def _batch_rows(year: int, *, first: int = 1, count: int = 17, y0: float = 100.0):
        """Rows as the audit sees them: the whole label, the year among it."""
        return tuple(
            (
                f"Go to album Fixture Album 00 Writable Batch {number:04} "
                f"Fixture Artist 00 Fixture Album 00 {year} 0:01 \u2014",
                y0 + 38.0 * offset,
            )
            for offset, number in enumerate(range(first, first + count))
        )

    def _batch_traces(self, *, after_year: int = 2042, after_rows=None):
        before = self._batch_rows(1980)
        after = after_rows if after_rows is not None else self._batch_rows(after_year)
        header = {"kind": "activate", "target_label": "Title column header"}
        return [
            ActionTrace(
                action=header,
                before_rows=self._batch_rows(1980, first=40),
                after_rows=before,
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "scroll", "direction": "down", "amount": 1, "by": "page"},
                before_rows=before,
                after_rows=self._batch_rows(1980, first=18),
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "activate", "target_label": "Edit Tags"},
                after_tree_labels=("512 of 100,000 tracks", "Edit 512 Tracks"),
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "type", "target_label": "Genre", "fixture_token": "BATCH_GENRE"},
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "type", "target_label": "Year", "fixture_token": "BATCH_YEAR"},
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "activate", "target_label": "Save 512"},
                before_tree_labels=("Edit 512 Tracks", "will be applied to all 512"),
                after_tree_labels=("Edit 512 Tracks", "Saving\u2026 512/512"),
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "wait", "expect_status": True},
                finding_codes=("missing-waiting-feedback",),
            ),
            ActionTrace(
                action={"kind": "scroll", "direction": "up", "amount": 1, "by": "page"},
                before_rows=self._batch_rows(after_year, first=18),
                after_rows=after,
                state_changed=True,
            ),
        ]

    def test_batch_checkpoint_exercises_progress_selection_and_anchor_contract(self) -> None:
        workload = self.mission.workloads[0]
        weak = audit_action_workload(0, workload, [])
        traces = self._batch_traces()

        self.assertFalse(weak["complete"])
        result = audit_action_workload(0, workload, traces, self.mission.fixture_tokens)
        self.assertTrue(result["complete"], result)
        # The year is the one cell the edit changes; the title keys the row.
        self.assertEqual(result["scroll_anchor_rows_held"], 17)
        self.assertTrue(result["selection_observed"])
        self.assertTrue(result["sorted_by_unedited_column"])

    def test_batch_anchor_is_not_satisfied_by_a_few_rows_keeping_their_place(self) -> None:
        # The 2026-10-10 rerun: sorted by the year the edit overwrites, 2 of 17
        # titles kept their y by coincidence. Keyed by title that is not a hold.
        workload = self.mission.workloads[0]
        shuffled = tuple(
            (label, y if index in (3, 9) else y + 38.0 * (index + 1))
            for index, (label, y) in enumerate(self._batch_rows(2042))
        )
        result = audit_action_workload(
            0,
            workload,
            self._batch_traces(after_rows=shuffled),
            self.mission.fixture_tokens,
        )

        self.assertFalse(result["scroll_anchor_restored"])
        self.assertEqual(result["scroll_anchor_rows_shared"], 17)
        self.assertEqual(result["scroll_anchor_rows_held"], 2)
        self.assertFalse(result["complete"])

    def test_batch_anchor_without_a_title_pattern_keeps_the_whole_label(self) -> None:
        workload = {
            key: value
            for key, value in self.mission.workloads[0].items()
            if key not in {"anchor_title_pattern", "sort_by"}
        }
        result = audit_action_workload(
            0, workload, self._batch_traces(), self.mission.fixture_tokens
        )

        # Every label carries the year, which the edit changed: nothing is shared.
        self.assertEqual(result["scroll_anchor_rows_shared"], 0)
        self.assertFalse(result["scroll_anchor_restored"])

    def test_batch_without_the_sort_click_is_not_complete(self) -> None:
        workload = self.mission.workloads[0]
        traces = self._batch_traces()[1:]

        result = audit_action_workload(0, workload, traces, self.mission.fixture_tokens)

        self.assertFalse(result["sorted_by_unedited_column"])
        self.assertFalse(result["complete"])

    def test_batch_selection_is_read_from_the_dialog_title_not_a_count_of_tracks(self) -> None:
        workload = self.mission.workloads[0]
        traces = self._batch_traces()
        # "512 of 100,000 tracks" is the list's own counter, not a selection.
        no_title = [
            dataclasses.replace(
                trace,
                after_tree_labels=tuple(
                    label for label in trace.after_tree_labels if "Edit 512" not in label
                ),
                before_tree_labels=tuple(
                    label for label in trace.before_tree_labels if "Edit 512" not in label
                ),
            )
            for trace in traces
        ]

        result = audit_action_workload(0, workload, no_title, self.mission.fixture_tokens)

        self.assertFalse(result["selection_observed"])
        self.assertFalse(result["complete"])


if __name__ == "__main__":
    unittest.main(verbosity=1)
