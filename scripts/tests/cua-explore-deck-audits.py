#!/usr/bin/env python3
"""The stress deck's audits and plans against the shapes the 2026-10-10 rerun recorded.

Fixtures named `stress-2026-10-10-*` are verbatim excerpts of that run's evidence
(cua-driver 0.34.0, seed 11). Hand-written driver output disagreed with the real
driver three times, so what the driver says is quoted, and only what the harness
adds to it is built here.
"""

from __future__ import annotations

import dataclasses
import json
import pathlib
import random
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
FIXTURES = REPO_ROOT / "scripts" / "tests" / "fixtures"
sys.path.insert(0, str(EXPLORE_ROOT))

from agents.plans import POPOVER_SCROLL_STEPS, build_phases  # noqa: E402
from agents.vocabulary import LabelMatcher  # noqa: E402
from atspi_geometry import GeometryNode, column_header_elements  # noqa: E402
from driver import CuaExecutor  # noqa: E402
from hover_geometry import WindowGeometry  # noqa: E402
from oracles import ActionEvidence, normalize_snapshot  # noqa: E402
from protocol import ContractError, load_mission  # noqa: E402
from runner import _trace_from_observations  # noqa: E402
from tree_labels import unindexed_labels, unindexed_nodes  # noqa: E402
from ui_vocabulary import (  # noqa: E402
    ACTIONABLE_ROLES,
    COLUMN_HEADER_ROLE,
    column_header_label,
    hover_strictness,
)
from workload_audit import (  # noqa: E402
    ActionTrace,
    audit_action_workload,
    chip_shown,
    workloads_click_column_headers,
)

ORIGIN = WindowGeometry(200, 50, 1200, 800)
MISSION = load_mission(EXPLORE_ROOT / "missions" / "large-library-stress.json")


def fixture(name: str) -> dict:
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


def node(role, label, x, y, w, h):
    return GeometryNode(role=role, label=label, x=x, y=y, width=w, height=h)


# The frame node sits at (-5, -5) in WINDOW coordinates, as measured on Reprise.
FRAME = node("frame", "Reprise", -5, -5, 1200, 800)
HEADERS = [
    node("column header", name, 60 + index * 150, 84, 150, 28)
    for index, name in enumerate(("Title", "Artist", "Album", "Year", "Length", "Rating"))
]
# A header's own child is a plain label with the same name; it is not a header.
HEADER_CHILD = node("label", "Title", 62, 86, 40, 20)


class TreeLabelTests(unittest.TestCase):
    def setUp(self) -> None:
        self.dialog = fixture("stress-2026-10-10-batch-dialog.json")

    def test_the_dialog_title_is_an_unindexed_label_in_the_tree(self) -> None:
        labels = unindexed_labels(self.dialog["tree_markdown"])

        self.assertIn("Edit 512 Tracks", labels)
        self.assertIn("512 of 100,000 tracks", labels)
        indexed = [item["label"] for item in self.dialog["elements"] if item.get("label")]
        self.assertNotIn("Edit 512 Tracks", indexed)

    def test_an_indexed_line_is_never_read_as_an_unindexed_node(self) -> None:
        nodes = unindexed_nodes(self.dialog["tree_markdown"])

        self.assertNotIn("Save 512", [label for role, label in nodes if role == "button"])
        self.assertTrue(all(role != "list item" for role, _label in nodes))

    def test_a_disabled_button_keeps_its_label_without_the_flag(self) -> None:
        markdown = '          - button = "Cancel" (disabled)\n          - column header = "Year"\n'

        self.assertEqual(
            unindexed_nodes(markdown),
            (("button", "Cancel"), ("column header", "Year")),
        )

    def test_a_snapshot_carries_the_tree_labels_in_order(self) -> None:
        state = normalize_snapshot(
            {"structuredContent": {"elements": [], "tree_markdown": self.dialog["tree_markdown"]}},
            state_id="s",
            captured_ms=0,
        )

        self.assertEqual(state.tree_labels[0], "512 of 100,000 tracks")
        self.assertLess(
            state.tree_labels.index("Edit 512 Tracks"),
            state.tree_labels.index("Only changed fields will be written to all selected tracks"),
        )

    def test_the_driver_observation_hands_them_to_the_agent_and_the_audit(self) -> None:
        raw = {"elements": self.dialog["elements"], "tree_markdown": self.dialog["tree_markdown"]}
        executor = CuaExecutor(_Transport(raw), pid=1, window_id=2, session="t", settle_delays=())

        observation = executor.observe()
        trace = _trace_from_observations(
            {"kind": "activate", "target_label": "Edit tags…"}, observation, observation
        )

        self.assertIn("Edit 512 Tracks", observation["tree_labels"])
        self.assertIn("Edit 512 Tracks", trace.after_tree_labels)
        self.assertIn("Edit 512 Tracks", trace.before_tree_labels)


class ColumnHeaderElementTests(unittest.TestCase):
    def test_each_header_becomes_an_element_with_a_label_of_its_own(self) -> None:
        elements = column_header_elements([FRAME, *HEADERS, HEADER_CHILD], ORIGIN)

        self.assertEqual(
            [item["label"] for item in elements],
            [column_header_label(name) for name in ("Title", "Artist", "Album", "Year", "Length", "Rating")],
        )
        self.assertTrue(all(item["role"] == COLUMN_HEADER_ROLE for item in elements))

    def test_the_frame_is_the_walk_rectangle_in_the_driver_s_coordinate_space(self) -> None:
        title = column_header_elements([FRAME, *HEADERS], ORIGIN)[0]

        # Normalised against the frame node (-5, -5), then the window origin added.
        self.assertEqual(title["frame"], {"x": 200 + 65.0, "y": 50 + 89.0, "w": 150.0, "h": 28.0})

    def test_a_header_offers_no_accessibility_action_so_a_click_goes_by_pixel(self) -> None:
        elements = column_header_elements([FRAME, *HEADERS], ORIGIN)

        self.assertTrue(all(item["actions"] == [] for item in elements))
        self.assertTrue(all(item["geometry_trusted"] for item in elements))

    def test_a_header_outside_the_window_is_left_out(self) -> None:
        elsewhere = node("column header", "Elsewhere", 5000, 84, 150, 28)

        labels = [item["label"] for item in column_header_elements([FRAME, elsewhere], ORIGIN)]

        self.assertEqual(labels, [])

    def test_a_header_is_actionable_but_the_hover_sweep_has_no_contract_for_it(self) -> None:
        self.assertIn(COLUMN_HEADER_ROLE, ACTIONABLE_ROLES)
        self.assertEqual(hover_strictness(COLUMN_HEADER_ROLE), "skip")

    def test_the_label_never_collides_with_the_facet_or_the_tag_field_of_the_same_name(self) -> None:
        for name in ("Title", "Year", "Rating"):
            self.assertNotEqual(column_header_label(name), name)


class DriverColumnHeaderTests(unittest.TestCase):
    NODES = [FRAME, *HEADERS]

    def _executor(self, *, column_headers: bool):
        frame = {
            "element_index": 0,
            "role": "frame",
            "label": "Reprise",
            "depth": 0,
            "frame": {"x": 200, "y": 50, "w": 1200, "h": 800},
        }
        transport = _Transport({"elements": [frame], "frame_scale": 1.0})
        executor = CuaExecutor(
            transport,
            pid=1,
            window_id=2,
            session="t",
            settle_delays=(),
            geometry_provider=lambda: list(self.NODES),
            window_origin=ORIGIN,
            column_headers=column_headers,
        )
        return transport, executor

    def test_headers_are_added_to_the_observation_only_when_a_mission_sorts(self) -> None:
        _t, plain = self._executor(column_headers=False)
        _t, sorting = self._executor(column_headers=True)

        self.assertNotIn("Title column header", plain.observe()["actionable_labels"])
        self.assertIn("Title column header", sorting.observe()["actionable_labels"])

    def test_the_stress_mission_asks_for_headers_and_the_others_do_not(self) -> None:
        self.assertTrue(workloads_click_column_headers(MISSION.workloads))
        for name in ("offline-recovery", "section-search-isolation", "first-time-exploration"):
            mission = load_mission(EXPLORE_ROOT / "missions" / f"{name}.json")
            self.assertFalse(workloads_click_column_headers(mission.workloads), name)

    def test_a_header_click_is_a_pixel_click_inside_the_header_not_a_row_click(self) -> None:
        transport, executor = self._executor(column_headers=True)

        result = executor.execute_evidence(
            ActionEvidence.activate("Artist column header", dispatch="px")
        )

        clicks = [payload for tool, payload in transport.calls if tool == "click"]
        # Only the click itself: an element-addressed probe has no index to aim at.
        self.assertEqual(len(clicks), 1)
        self.assertNotIn("element_token", clicks[0])
        # The Artist header is WINDOW (210, 84) 150x28; the frame node sits at
        # (-5, -5), so its window-local centre is (210 + 5 + 75, 84 + 5 + 14).
        self.assertEqual((clicks[0]["x"], clicks[0]["y"]), (290.0, 103.0))
        self.assertTrue(result.evidence.dispatched)

    def test_the_header_row_label_is_not_what_the_agent_aims_at(self) -> None:
        _t, executor = self._executor(column_headers=True)

        labels = executor.observe()["actionable_labels"]

        self.assertNotIn("Title Artist Album Year Length Rating", labels)


class MatcherTests(unittest.TestCase):
    def setUp(self) -> None:
        self.popover = fixture("stress-2026-10-10-year-popover.json")
        raw = {"elements": self.popover["elements"], "popups": self.popover["popups"]}
        self.observation = CuaExecutor(
            _Transport(raw), pid=1, window_id=2, session="t", settle_delays=()
        ).observe()

    @staticmethod
    def value(option: str, *, in_popup: bool = True) -> LabelMatcher:
        import re

        return LabelMatcher(
            patterns=(rf"{re.escape(option)}(?: \(\d[\d,]*\))?",), in_popup=in_popup
        )

    def test_the_observation_carries_the_popup_the_driver_reported(self) -> None:
        self.assertEqual(
            self.observation["popups"],
            [{"x": 568.0, "y": 252.0, "width": 318.0, "height": 288.0}],
        )

    def test_a_value_below_the_fold_is_not_picked_for_a_click(self) -> None:
        # 1993 (106) sits at y=796 and the popup ends at y=540: the click that the
        # run sent there landed on the track list and closed the popover.
        self.assertEqual(self.value("1993").candidates(self.observation), ())
        self.assertEqual(
            self.value("1993", in_popup=False).candidates(self.observation),
            ("1993 (106)",),
        )

    def test_a_value_inside_the_popup_is_picked_with_its_count(self) -> None:
        self.assertEqual(self.value("1985").candidates(self.observation), ("1985 (106)",))

    def test_a_placeholder_frame_never_counts_as_inside_a_popup(self) -> None:
        # 2004 (105) is unresolved and carries the window origin as its position.
        self.assertEqual(self.value("2004").candidates(self.observation), ())

    def test_the_rating_value_does_not_match_a_track_row_holding_the_digit(self) -> None:
        loose = LabelMatcher(exact=("4",), contains=("4",), require_actionable=False)
        pattern = LabelMatcher(
            patterns=self.value("4", in_popup=False).patterns, require_actionable=False
        )

        self.assertTrue(
            any(label.startswith("Go to album Album 04000") for label in loose.candidates(self.observation)),
        )
        self.assertEqual(pattern.candidates(self.observation), ())

    def test_the_genre_value_is_not_the_writable_fixtures_own_genre(self) -> None:
        observation = {
            "actionable_labels": ["Fixture Genre 00 (64)", "Genre 00 (4,974)"],
            "elements": [
                {"label": label, "role": "list item", "actionable": True, "enabled": True}
                for label in ("Fixture Genre 00 (64)", "Genre 00 (4,974)")
            ],
        }

        self.assertEqual(self.value("Genre 00", in_popup=False).candidates(observation), ("Genre 00 (4,974)",))


class PlanTests(unittest.TestCase):
    def phases(self, mission=MISSION):
        return {phase.name: phase for phase in build_phases(_agent_mission(mission), seed=11)}

    def test_every_sort_step_aims_at_one_column_header(self) -> None:
        steps = [s for s in self.phases()["sort-cycle"].steps if s.name.startswith("sort-")]
        columns = {s.matcher.exact[0] for s in steps}

        self.assertEqual(len(steps), 24)
        self.assertEqual(
            columns, {column_header_label(c) for c in ("title", "artist", "album", "year", "rating")}
        )
        for step in steps:
            self.assertEqual(step.matcher.roles, (COLUMN_HEADER_ROLE,))
            self.assertTrue(step.matcher.strict_roles)

    def test_the_batch_edit_sorts_by_the_unedited_column_before_it_selects(self) -> None:
        names = [s.name for s in self.phases()["batch-edit"].steps]
        sort = next(s for s in self.phases()["batch-edit"].steps if s.name == "sort-before-edit")

        self.assertEqual(sort.matcher.exact, (column_header_label("title"),))
        self.assertLess(names.index("close-search"), names.index("sort-before-edit"))
        # Focus ends on the list before Ctrl+A.
        self.assertLess(names.index("sort-before-edit"), names.index("focus-first-row"))
        self.assertLess(names.index("focus-first-row"), names.index("select-all"))

    def test_without_sort_by_the_batch_edit_keeps_the_list_order_it_has(self) -> None:
        workloads = [dict(w) for w in MISSION.workloads]
        workloads[0].pop("sort_by")
        mission = _agent_mission(MISSION, workloads=workloads)

        names = [s.name for s in build_phases(mission, seed=11)[0].steps]

        self.assertNotIn("sort-before-edit", names)

    def test_a_facet_value_is_scrolled_into_the_popover_before_it_is_clicked(self) -> None:
        steps = self.phases()["combined-filter"].steps
        names = [s.name for s in steps]
        year = steps[names.index("choose-value-year")]
        scrolls = [s for s in steps if s.name.startswith("scroll-year-values-")]

        self.assertEqual(len(scrolls), POPOVER_SCROLL_STEPS)
        self.assertTrue(year.matcher.in_popup)
        self.assertLess(names.index("choose-facet-year"), names.index(scrolls[0].name))
        self.assertLess(names.index(scrolls[-1].name), names.index("choose-value-year"))
        for scroll in scrolls:
            self.assertEqual(scroll.kind, "scroll")
            self.assertFalse(scroll.required)
            # Once the wanted value is on screen the scrolling stops.
            self.assertEqual(scroll.skip_when, year.matcher)
            self.assertTrue(scroll.matcher.in_popup)

    def test_the_value_step_clicks_what_the_pick_resolved(self) -> None:
        steps = self.phases()["combined-filter"].steps
        value = next(s for s in steps if s.name == "choose-value-rating")

        self.assertEqual(value.matcher.candidates(_listed(("0 (1,658)", "2 (1,658)", "4 (1,658)"))), ("4 (1,658)",))


class ChipTests(unittest.TestCase):
    def test_a_chip_is_the_facet_name_followed_by_its_value(self) -> None:
        self.assertTrue(chip_shown([], ["x", "Genre", "Genre 00", "×"], "Genre: Genre 00"))
        self.assertTrue(chip_shown([], ["Rating", "4 stars"], "Rating: 4"))

    def test_the_popover_s_own_value_label_is_not_a_chip(self) -> None:
        # The popover lists "Genre 00" as an item; the facet name does not precede it.
        popover = ["Exploratory Batch Genre", "512", "Genre 00", "4,974", "Genre 01"]

        self.assertFalse(chip_shown([], popover, "Genre: Genre 00"))

    def test_a_chip_that_was_one_label_is_still_read(self) -> None:
        self.assertTrue(chip_shown(["Genre: Genre 00"], [], "Genre: Genre 00"))

    def test_a_chip_is_found_only_when_the_audit_s_facet_matches(self) -> None:
        self.assertFalse(chip_shown([], ["Year", "1993"], "Genre: Genre 00"))

    def _filter_traces(self, *, chip_after: bool):
        facets = {"genre": "Genre: Genre 00", "year": "Year: 1993", "rating": "Rating: 4"}
        pairs = {"genre": ("Genre", "Genre 00"), "year": ("Year", "1993"), "rating": ("Rating", "4")}
        shown: list[str] = []
        traces = []
        for facet in facets:
            before = tuple(shown)
            if chip_after:
                shown.extend(pairs[facet])
            traces.append(
                ActionTrace(
                    action={"kind": "activate", "target_label": f"{pairs[facet][1]} (106)"},
                    before_rows=(("a", 1.0),),
                    after_rows=(("b", 1.0),),
                    before_tree_labels=before,
                    after_tree_labels=tuple(shown),
                    state_changed=True,
                )
            )
        traces.append(
            ActionTrace(
                action={"kind": "type", "target_label": "Search all fields", "fixture_token": "SEARCH_NEEDLE"},
                after_rows=(("Needle 099700", 1.0),),
                after_tree_labels=tuple(shown),
                state_changed=True,
            )
        )
        return traces

    def test_a_facet_that_leaves_the_first_page_alone_is_credited_by_the_counter(self) -> None:
        # Sorted by artist, the hundred "Artist 0000" rows all pass the genre
        # filter: the visible rows are identical before and after, and the
        # counter in the filter bar is what moved (replayed from the 2026-10-10 run).
        workload = MISSION.workloads[2]
        traces = self._filter_traces(chip_after=True)
        same_rows = [
            dataclasses.replace(
                trace,
                after_rows=trace.before_rows,
                before_tree_labels=(*trace.before_tree_labels, "100,000 tracks \u00b7 265 d 3 h"),
                after_tree_labels=(*trace.after_tree_labels, f"{4974 - index} of 100,000 tracks"),
            )
            for index, trace in enumerate(traces[:3])
        ]

        done = audit_action_workload(2, workload, [*same_rows, traces[3]], MISSION.fixture_tokens)
        frozen = audit_action_workload(2, workload, [
            dataclasses.replace(trace, after_rows=trace.before_rows) for trace in traces[:3]
        ] + [traces[3]], MISSION.fixture_tokens)

        self.assertTrue(done["facets_complete"], done)
        self.assertFalse(frozen["facets_complete"])

    def test_the_filter_audit_reads_the_chips_from_the_tree(self) -> None:
        workload = MISSION.workloads[2]

        done = audit_action_workload(2, workload, self._filter_traces(chip_after=True), MISSION.fixture_tokens)
        missing = audit_action_workload(2, workload, self._filter_traces(chip_after=False), MISSION.fixture_tokens)

        self.assertTrue(done["facets_complete"], done)
        self.assertTrue(done["combined_facets_visible"])
        self.assertTrue(done["complete"])
        self.assertFalse(missing["facets_complete"])


class ProtocolTests(unittest.TestCase):
    def _load(self, mutate):
        data = json.loads((EXPLORE_ROOT / "missions" / "large-library-stress.json").read_text(encoding="utf-8"))
        mutate(data["workloads"][0])
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "mission.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            return load_mission(path)

    def test_the_stress_mission_declares_the_sort_and_the_anchor(self) -> None:
        batch = MISSION.workloads[0]

        self.assertEqual(batch["sort_by"], "title")
        self.assertEqual(batch["anchor_title_pattern"], r"Writable Batch \d+")

    def test_an_anchor_pattern_that_is_not_a_regex_is_rejected(self) -> None:
        with self.assertRaises(ContractError):
            self._load(lambda workload: workload.update(anchor_title_pattern="Writable ("))

    def test_a_sort_column_must_be_a_non_empty_string(self) -> None:
        with self.assertRaises(ContractError):
            self._load(lambda workload: workload.update(sort_by=""))


class _Transport:
    """Answers get_window_state with one fixed snapshot and records every call."""

    def __init__(self, raw):
        self.raw = raw
        self.calls = []

    def call(self, tool, payload):
        self.calls.append((tool, payload))
        if tool == "get_window_state":
            return {"structuredContent": dict(self.raw)}
        return {"effect": "confirmed", "summary": "Clicked", "verified": True}

    def resize_window(self, *args):
        return {}

    def set_connectivity(self, state):
        return {}

    def wmctrl_geometry(self, window_id):
        return None


def _agent_mission(mission, *, workloads=None):
    return {
        "schema_version": 1,
        "id": mission.mission_id,
        "workloads": list(workloads if workloads is not None else mission.workloads),
    }


def _listed(labels):
    return {
        "actionable_labels": list(labels),
        "elements": [
            {"label": label, "role": "list item", "actionable": True, "enabled": True}
            for label in labels
        ],
    }


if __name__ == "__main__":
    random.seed(0)
    unittest.main(verbosity=1)
