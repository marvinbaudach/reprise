#!/usr/bin/env python3
"""Pin what each recorded fixture is, so none of them drifts back into a lie.

The exploratory harness died five times in one night against a suite that was
green, because `hover-sweep-observe.json` was recorded before the harness began
injecting AT-SPI actions and therefore had *no* element carrying `actions`. The
ambiguity trap it was supposed to cover could not exist in it.

These tests do not check behaviour. They check that every fixture still is the
kind of recording its `_note` claims it is:

  measured  - taken after CuaExecutor.with_measured_geometry: has `actions`,
              has real `frame` values. What _target and the agent see.
  raw       - straight cua-driver output: no `actions`, every `frame.y` is 0.
              Good for roles and labels, useless for actions.
"""

from __future__ import annotations

import json
import pathlib
import unittest


FIXTURES = pathlib.Path(__file__).resolve().parent / "fixtures"

AMBIGUOUS_CELLS = FIXTURES / "night-2026-08-10-ambiguous-cells.json"
MUSIC_COLLAPSED = FIXTURES / "night-2026-08-10-music-collapsed.json"
SIDEBAR_OPEN = FIXTURES / "postfix-2026-08-10-sidebar-open.json"
SEARCH_OPEN = FIXTURES / "postfix-2026-08-10-search-open.json"
PRE_INJECTION = FIXTURES / "hover-sweep-observe.json"

SECTION_LABELS = ("Music", "Podcasts", "Radio", "YouTube")


def load(path: pathlib.Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def elements(path: pathlib.Path) -> list[dict]:
    return [item for item in (load(path).get("elements") or []) if isinstance(item, dict)]


def with_actions(path: pathlib.Path) -> list[dict]:
    return [item for item in elements(path) if item.get("actions")]


class FixtureProvenance(unittest.TestCase):
    def test_every_fixture_says_where_it_came_from(self) -> None:
        for path in (AMBIGUOUS_CELLS, MUSIC_COLLAPSED, SIDEBAR_OPEN, SEARCH_OPEN):
            with self.subTest(fixture=path.name):
                raw = load(path)
                self.assertTrue(raw.get("_source"), "fixture without a recorded origin")
                self.assertTrue(raw.get("_note"), "fixture without a kind note")
                self.assertTrue(elements(path), "fixture without elements")


class MeasuredFixtures(unittest.TestCase):
    """Recorded after with_measured_geometry - these must carry actions."""

    def test_measured_fixtures_carry_actions(self) -> None:
        for path in (AMBIGUOUS_CELLS, MUSIC_COLLAPSED, SIDEBAR_OPEN):
            with self.subTest(fixture=path.name):
                self.assertTrue(
                    with_actions(path),
                    "a measured fixture with no element carrying `actions` is the "
                    "exact drift that let the ambiguity trap through the suite",
                )

    def test_ambiguous_cells_really_contains_the_collision(self) -> None:
        by_label: dict[str, int] = {}
        for item in with_actions(AMBIGUOUS_CELLS):
            label = str(item.get("label") or "")
            if label:
                by_label[label] = by_label.get(label, 0) + 1
        colliding = {label: n for label, n in by_label.items() if n > 1}
        self.assertTrue(
            colliding,
            "this fixture exists to prove two action-carrying nodes can share a "
            "label; without one it proves nothing",
        )

    def test_music_collapsed_has_no_sidebar_section(self) -> None:
        labels = {str(item.get("label") or "") for item in elements(MUSIC_COLLAPSED)}
        for section in SECTION_LABELS:
            with self.subTest(section=section):
                self.assertNotIn(
                    section,
                    labels,
                    "this fixture records a 1200px window whose side panels are "
                    "closed - a section in it means the wrong snapshot was copied",
                )

    def test_sidebar_open_has_the_sections_and_their_selection(self) -> None:
        rows = {
            str(item.get("label") or ""): item
            for item in elements(SIDEBAR_OPEN)
            if str(item.get("role") or "") in ("list item", "row")
        }
        self.assertIn("Music", rows, "the open-sidebar fixture must show the sections")
        self.assertTrue(
            rows["Music"].get("selected"),
            "the audit relies on sidebar rows reporting `selected`",
        )
        self.assertEqual(
            rows["Music"].get("actions"),
            [],
            "measured on 2026-08-10: sidebar rows carry no action at all. If this "
            "ever changes, no-accessible-action and the dispatch rule need a "
            "fresh measurement, not an edited fixture",
        )


class RawFixtures(unittest.TestCase):
    """Straight driver output - no actions, no measured frames."""

    def test_search_open_is_raw_driver_output(self) -> None:
        self.assertFalse(
            with_actions(SEARCH_OPEN),
            "this fixture is raw driver output; an `actions` key means it was "
            "recorded through a different path than its note claims",
        )

    def test_search_open_carries_the_opened_entry(self) -> None:
        entries = [
            item
            for item in elements(SEARCH_OPEN)
            if str(item.get("role") or "") in ("search box", "entry", "text field")
        ]
        self.assertEqual(
            [str(item.get("label") or "") for item in entries],
            ["Search all fields"],
            "the opened search entry carries the same accessible name as the "
            "toggle that opens it - that is why every `type` step needs a role "
            "filter",
        )

    def test_the_pre_injection_fixture_stays_recognisable(self) -> None:
        self.assertFalse(
            with_actions(PRE_INJECTION),
            "hover-sweep-observe.json was recorded on 2026-08-07, before the "
            "harness injected AT-SPI actions. It still covers the role path. If "
            "it ever grows actions it was re-recorded, and the tests that treat "
            "it as action-free have to be revisited",
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
