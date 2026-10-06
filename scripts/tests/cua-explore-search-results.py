#!/usr/bin/env python3
"""Count search results the way the section-search audit has to see them.

cua-driver 0.33 reports a list item, a table row and a source-card button the
same way for the sidebar, the column header and the results. The audit once
counted every one of them, so `len(after_rows) == 1` failed on a screen that
showed exactly the right single result. These tests start from recorded 0.33
snapshots (`fixtures/cua033-*.json`, verbatim `elements` of the settled state
the runner reads after each step) and go through the real driver code, the real
trace projection and the real audit, so a tree shape cannot be invented here.
"""

from __future__ import annotations

import copy
import json
import pathlib
import sys
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
FIXTURES = pathlib.Path(__file__).resolve().parent / "fixtures"
sys.path.insert(0, str(EXPLORE_ROOT))

import runner  # noqa: E402
from agents.agent_core import AgentSession, observation_to_trace  # noqa: E402
from agents.assertions import assertion_codes  # noqa: E402
from driver import CuaExecutor  # noqa: E402
from protocol import load_mission  # noqa: E402
from search_results import result_elements  # noqa: E402
from workload_audit import audit_action_workload  # noqa: E402

MISSION = load_mission(EXPLORE_ROOT / "missions" / "section-search-isolation.json")
TOKENS = dict(MISSION.fixture_tokens)
WORKLOAD = MISSION.workloads[0]

# The sources in the order the bundled agent visits them. The agent reads the
# mission with sorted keys; the mission file lists YouTube before Radio.
AGENT_ORDER = ("Music", "Podcasts", "Radio", "YouTube")
MUSIC_NEEDLE = "Writable Batch 0042"


def raw(name: str) -> dict:
    return json.loads((FIXTURES / f"cua033-{name}.json").read_text(encoding="utf-8"))


class Transport:
    def __init__(self, snapshot: dict) -> None:
        self.snapshot = snapshot

    def call(self, tool, payload):
        return self.snapshot if tool == "get_window_state" else {}


def observe(snapshot: dict, state: str = "recorded") -> dict:
    """The observation the runner and the agent see, built by the driver code."""
    return CuaExecutor(
        Transport(snapshot), pid=1, window_id=2, session=state, settle_delays=()
    ).observe()


def labels(elements) -> list[str]:
    return [str(item["label"]) for item in elements]


class ResultScopeTests(unittest.TestCase):
    def test_music_shows_one_result_not_the_sidebar_and_the_header(self) -> None:
        found = labels(result_elements(observe(raw("search-music-settled"))))

        self.assertEqual(len(found), 1, found)
        self.assertIn(MUSIC_NEEDLE, found[0])

    def test_radio_shows_one_station_not_the_header_row(self) -> None:
        found = labels(result_elements(observe(raw("search-radio-settled"))))

        self.assertEqual(len(found), 1, found)
        self.assertIn(TOKENS["RADIO_ONLY_NEEDLE"], found[0])

    def test_podcasts_result_is_the_episode_button_not_its_controls(self) -> None:
        found = labels(result_elements(observe(raw("search-podcasts-settled"))))

        self.assertEqual(found, [TOKENS["PODCAST_ONLY_NEEDLE"]])

    def test_youtube_result_is_the_episode_button_not_its_controls(self) -> None:
        found = labels(result_elements(observe(raw("search-youtube-settled"))))

        self.assertEqual(found, [TOKENS["YOUTUBE_ONLY_NEEDLE"]])

    def test_a_view_without_results_counts_none(self) -> None:
        self.assertEqual(result_elements(observe(raw("my-stats-settled"))), [])

    def test_the_unfiltered_list_is_still_many_results(self) -> None:
        # Control arm: scoping must not collapse everything to one.
        found = result_elements(observe(raw("search-music-unfiltered")))

        self.assertEqual(len(found), 5)

    def test_a_second_result_row_is_counted(self) -> None:
        snapshot = raw("search-music-settled")
        by_index = {item["element_index"]: item for item in snapshot["elements"]}
        data_row = next(
            item
            for item in snapshot["elements"]
            if item["role"] == "row" and by_index[item["parent_index"]]["role"] == "list"
        )
        extra = dict(data_row, element_index=900, label=data_row["label"] + " again")
        snapshot["elements"].append(extra)

        self.assertEqual(len(result_elements(observe(snapshot))), 2)

    def test_a_snapshot_without_hierarchy_falls_back_to_rows(self) -> None:
        observation = {
            "elements": [
                {"label": "Title", "role": "row", "frame": {"y": 1.0}},
                {"label": "Button", "role": "button", "frame": {"y": 2.0}},
            ]
        }

        self.assertEqual(labels(result_elements(observation)), ["Title"])


def type_trace(source: str, token_name: str, *, after: dict, before: dict):
    action = {
        "kind": "type",
        "dispatch": "ax",
        "target_label": "Search all fields",
        "fixture_token": token_name,
    }
    return runner._trace_from_observations(action, observe(before, "b"), observe(after, "a"))


def activate_trace(label: str, *, before: dict, after: dict):
    action = {"kind": "activate", "dispatch": "ax", "target_label": label}
    return runner._trace_from_observations(action, observe(before, "b"), observe(after, "a"))


def agent_run(overrides: dict | None = None) -> list:
    """The retained trajectory of a run that did everything right."""
    overrides = overrides or {}
    snapshots = {
        "Music": (raw("search-music-unfiltered"), raw("search-music-settled")),
        "Podcasts": (raw("search-podcasts-open"), raw("search-podcasts-settled")),
        "Radio": (raw("search-radio-open"), raw("search-radio-settled")),
        "YouTube": (raw("search-youtube-open"), raw("search-youtube-settled")),
    }
    tokens = WORKLOAD["route_tokens"]
    traces = []
    previous = raw("my-stats-settled")
    for source in AGENT_ORDER:
        opened, settled = snapshots[source]
        settled = overrides.get(source, settled)
        traces.append(activate_trace(source, before=previous, after=opened))
        traces.append(
            type_trace(source, tokens[source], before=opened, after=settled)
        )
        previous = settled
    traces.append(activate_trace("My Stats", before=previous, after=raw("my-stats-settled")))
    return traces


def audit(traces: list) -> dict:
    return audit_action_workload(0, WORKLOAD, traces, TOKENS)


class SectionSearchAuditTests(unittest.TestCase):
    def test_a_correct_run_over_the_recorded_states_completes(self) -> None:
        result = audit(agent_run())

        self.assertEqual(
            result["route_results"],
            {"Music": True, "Podcasts": True, "YouTube": True, "Radio": True},
        )
        self.assertTrue(result["unsupported_search_disabled"]["My Stats"])
        self.assertTrue(result["complete"])

    def test_the_agent_may_visit_the_sources_in_its_own_order(self) -> None:
        # The mission file lists YouTube before Radio and the agent does the
        # opposite; neither order is wrong.
        result = audit(agent_run())

        self.assertTrue(result["route_results"]["Radio"])

    def test_an_unfiltered_music_list_is_not_a_single_result(self) -> None:
        unfiltered = raw("search-music-unfiltered")
        result = audit(agent_run({"Music": unfiltered}))

        self.assertFalse(result["route_results"]["Music"])
        self.assertFalse(result["complete"])

    def test_a_podcast_view_does_not_pass_for_music(self) -> None:
        result = audit(agent_run({"Music": raw("search-podcasts-settled")}))

        self.assertFalse(result["route_results"]["Music"])

    def test_a_leaked_second_podcast_result_fails_the_route(self) -> None:
        snapshot = raw("search-podcasts-settled")
        episode = next(
            item
            for item in snapshot["elements"]
            if item.get("label") == TOKENS["PODCAST_ONLY_NEEDLE"]
        )
        snapshot["elements"].append(
            dict(copy.deepcopy(episode), element_index=901, label="Fixture Music Leak")
        )
        result = audit(agent_run({"Podcasts": snapshot}))

        self.assertFalse(result["route_results"]["Podcasts"])


class CombinedFilterSearchTests(unittest.TestCase):
    """The combined-filter audit counts the search result the same way."""

    WORKLOAD = {
        "kind": "combined-filter",
        "facets": [],
        "active_labels": {},
        "include_search": True,
        "search_token": "MUSIC_ONLY_NEEDLE",
    }

    def search_complete(self, after: dict) -> bool:
        trace = type_trace(
            "Music",
            "MUSIC_ONLY_NEEDLE",
            before=raw("search-music-unfiltered"),
            after=after,
        )
        result = audit_action_workload(0, self.WORKLOAD, [trace], TOKENS)
        return result["search_complete"]

    def test_one_correct_result_completes_the_search(self) -> None:
        self.assertTrue(self.search_complete(raw("search-music-settled")))

    def test_an_unfiltered_list_does_not(self) -> None:
        self.assertFalse(self.search_complete(raw("search-music-unfiltered")))


class AgentSideCountTests(unittest.TestCase):
    """The agent keeps its own copies of the same two decisions."""

    def test_the_agent_trace_projects_the_same_results(self) -> None:
        before = observe(raw("search-music-unfiltered"))
        after = observe(raw("search-music-settled"))
        trace = observation_to_trace(before, after, {"kind": "type"})

        self.assertEqual(len(trace.after_rows), 1)
        self.assertIn(MUSIC_NEEDLE, trace.after_rows[0][0])

    def test_one_correct_result_is_not_reported_as_a_scope_leak(self) -> None:
        action = {
            "kind": "type",
            "target": {"label": "Search all fields"},
            "fixture_token": "MUSIC_ONLY_NEEDLE",
        }
        observation = observe(raw("search-music-settled"))
        # The entry has to hold the typed text for the assertion to apply.
        for item in observation["elements"]:
            if item["role"] == "search box":
                item["value"] = MUSIC_NEEDLE
        codes = [
            code
            for code, _summary, _evidence in assertion_codes(
                action,
                observation,
                "search-Music",
                section_changed=True,
                known_token_values={"MUSIC_ONLY_NEEDLE": MUSIC_NEEDLE},
            )
        ]

        self.assertNotIn("agent-search-scope-leak", codes)


class DuplicateRowNoteTests(unittest.TestCase):
    """`agent-duplicate-cached-row` is about rows, not about every shared name."""

    def notes_after_opening(self, snapshot: dict) -> set[str]:
        session = AgentSession(seed=11, probe_ratio=1.0)
        session._evaluate_transition_assertions(
            observe(raw("my-stats-settled")),
            observe(snapshot),
            {"kind": "activate", "target": {"label": "Podcasts"}},
            "open-Podcasts",
        )
        return {note.code for note in session.notes}

    def test_menu_buttons_and_sidebar_entries_are_not_duplicate_rows(self) -> None:
        # Every sidebar entry is a list item and a button of one name, and a GTK
        # menu button is a button wrapping a toggle button of one name.
        self.assertEqual(self.notes_after_opening(raw("search-podcasts-settled")), set())

    def test_a_source_listed_twice_is_still_noted(self) -> None:
        snapshot = raw("search-podcasts-settled")
        episode = next(
            item
            for item in snapshot["elements"]
            if item.get("label") == TOKENS["PODCAST_ONLY_NEEDLE"]
        )
        snapshot["elements"].append(dict(copy.deepcopy(episode), element_index=902))

        self.assertIn(
            "agent-duplicate-cached-row", self.notes_after_opening(snapshot)
        )


class RecordedFixtureProvenanceTests(unittest.TestCase):
    def test_every_recorded_state_names_its_driver_and_origin(self) -> None:
        for path in sorted(FIXTURES.glob("cua033-*.json")):
            with self.subTest(fixture=path.name):
                recorded = json.loads(path.read_text(encoding="utf-8"))
                self.assertIn("cua-driver 0.33.3", recorded["_source"])
                self.assertTrue(recorded["_note"])
                self.assertTrue(
                    all("parent_index" in item for item in recorded["elements"][1:])
                )


if __name__ == "__main__":
    unittest.main()
