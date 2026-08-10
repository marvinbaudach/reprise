#!/usr/bin/env python3
"""Regression tests for measured action names and fresh target resolution."""

from __future__ import annotations

import json
import pathlib
import sys
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
FIXTURES = pathlib.Path(__file__).resolve().parent / "fixtures"
AMBIGUOUS_CELLS = FIXTURES / "night-2026-08-10-ambiguous-cells.json"
MUSIC_COLLAPSED = FIXTURES / "night-2026-08-10-music-collapsed.json"
sys.path.insert(0, str(EXPLORE_ROOT))

from cua_explore_expectations import (  # noqa: E402
    MUSIC_COLLAPSED_ACTIONABLE_COUNT_BEFORE,
    MUSIC_COLLAPSED_ACTIONABLE_LABELS,
)
from oracles import OracleEngine, normalize_snapshot  # noqa: E402
from ui_vocabulary import (  # noqa: E402
    invocable_actions,
    is_structural_action,
    unknown_action_names,
)


def recorded_elements() -> list[dict]:
    raw = json.loads(AMBIGUOUS_CELLS.read_text(encoding="utf-8"))
    return [item for item in raw["elements"] if isinstance(item, dict)]


def load_fixture(path: pathlib.Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


class ActionVocabularyTests(unittest.TestCase):
    def test_measured_structural_names_are_not_invocable(self) -> None:
        structural = (
            "listitem.scroll-to",
            "list.select-all",
            "win.about",
            "window.close",
            "default.activate",
        )

        self.assertTrue(all(is_structural_action(name) for name in structural))
        self.assertEqual(invocable_actions(structural), ())

    def test_measured_click_and_unknown_names_remain_visible(self) -> None:
        names = ("click", "foo.bar", "listitem.scroll-to")

        self.assertEqual(invocable_actions(names), ("click", "foo.bar"))
        self.assertEqual(unknown_action_names(names), ("foo.bar",))

    def test_recorded_cells_are_structural_but_star_buttons_are_invocable(self) -> None:
        elements = recorded_elements()
        cells = [item for item in elements if item.get("role") == "grid cell"]
        stars = [item for item in elements if item.get("label") in {"★", "☆"}]

        self.assertTrue(cells)
        self.assertTrue(stars)
        self.assertTrue(
            all(not invocable_actions(item.get("actions", ())) for item in cells)
        )
        self.assertTrue(
            all(invocable_actions(item.get("actions", ())) == ("click",) for item in stars)
        )


class NormalizedSnapshotTests(unittest.TestCase):
    def setUp(self) -> None:
        self.raw = load_fixture(MUSIC_COLLAPSED)
        self.snapshot = normalize_snapshot(
            self.raw, state_id="recorded", captured_ms=0
        )

    def test_structural_actions_shrink_the_recorded_labels_from_83_to_31(self) -> None:
        old_labels = {
            str(item.get("label"))
            for item in self.raw["elements"]
            if item.get("label")
            and (
                item.get("actions")
                or str(item.get("role"))
                in {
                    "button",
                    "toggle button",
                    "row",
                    "entry",
                    "search box",
                    "text field",
                    "switch",
                }
            )
        }

        self.assertEqual(len(old_labels), MUSIC_COLLAPSED_ACTIONABLE_COUNT_BEFORE)
        self.assertEqual(
            self.snapshot.actionable_labels, MUSIC_COLLAPSED_ACTIONABLE_LABELS
        )

    def test_cells_are_not_actionable_but_rows_and_header_remain(self) -> None:
        actionable = [item for item in self.snapshot.elements if item.actionable]

        self.assertFalse(any(item.role == "grid cell" for item in actionable))
        self.assertTrue(any(item.role == "row" for item in actionable))
        self.assertIn(
            "Title Artist Album Year Length Rating",
            self.snapshot.actionable_labels,
        )

    def test_snapshot_collects_unknown_action_names(self) -> None:
        raw = load_fixture(MUSIC_COLLAPSED)
        raw["elements"][0]["actions"] = ["foo.bar", "listitem.scroll-to"]

        snapshot = normalize_snapshot(raw, state_id="unknown", captured_ms=0)

        self.assertEqual(snapshot.unknown_action_names, ("foo.bar",))

    def test_unknown_action_name_warns_once_per_oracle_run(self) -> None:
        raw = load_fixture(MUSIC_COLLAPSED)
        raw["elements"][0]["actions"] = ["foo.bar"]
        snapshot = normalize_snapshot(raw, state_id="unknown", captured_ms=0)
        engine = OracleEngine()

        first = [item for item in engine.inspect_snapshot(snapshot) if item.code == "unknown-action-name"]
        second = [item for item in engine.inspect_snapshot(snapshot) if item.code == "unknown-action-name"]

        self.assertEqual(len(first), 1)
        self.assertEqual(first[0].evidence["action_name"], "foo.bar")
        self.assertEqual(second, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
