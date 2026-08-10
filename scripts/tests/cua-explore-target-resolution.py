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
sys.path.insert(0, str(EXPLORE_ROOT))

from ui_vocabulary import (  # noqa: E402
    invocable_actions,
    is_structural_action,
    unknown_action_names,
)


def recorded_elements() -> list[dict]:
    raw = json.loads(AMBIGUOUS_CELLS.read_text(encoding="utf-8"))
    return [item for item in raw["elements"] if isinstance(item, dict)]


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


if __name__ == "__main__":
    unittest.main(verbosity=2)
