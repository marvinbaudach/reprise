#!/usr/bin/env python3
"""The hover sweep reaches Playlists through a playlist, because nothing else is operable.

The sidebar draws a PLAYLISTS heading and a new-playlist button, and cua-driver
0.33 exposes neither - not with a playlist, not without one. The only accessible
entry into the section is a playlist row, named by the playlist. The mission
therefore maps the section to the `PLAYLIST_NAME` fixture token, the generated
profile carries a playlist of that name, and both the explorer and the audit
resolve the section through the same mapping. These tests start from a recorded
0.33 snapshot of that sidebar (`fixtures/cua033-hover-playlist-sidebar.json`).
"""

from __future__ import annotations

import json
import pathlib
import sqlite3
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
FIXTURES = pathlib.Path(__file__).resolve().parent / "fixtures"
RECORDED = FIXTURES / "cua033-hover-playlist-sidebar.json"
sys.path.insert(0, str(EXPLORE_ROOT))

from driver import CuaExecutor  # noqa: E402
from explorer import DeterministicExplorer  # noqa: E402
from fixtures import (  # noqa: E402
    FIXTURE_PLAYLIST_NAME,
    PLANS,
    _seed_playlist,
    build_plan,
)
from protocol import ActionGateway, ContractError, load_mission  # noqa: E402
from section_handles import section_handle  # noqa: E402
from workload_audit import ActionTrace, audit_action_workload  # noqa: E402

MISSION_PATH = EXPLORE_ROOT / "missions" / "hover-affordance-sweep.json"
MISSION = load_mission(MISSION_PATH)
WORKLOAD = MISSION.workloads[0]
TOKENS = dict(MISSION.fixture_tokens)


class Transport:
    def __init__(self) -> None:
        self.raw = json.loads(RECORDED.read_text(encoding="utf-8"))

    def call(self, tool, payload):
        return self.raw if tool == "get_window_state" else {}


def recorded_observation() -> dict:
    return CuaExecutor(
        Transport(), pid=1, window_id=2, session="recorded", settle_delays=()
    ).observe()


class PlaylistFixtureTests(unittest.TestCase):
    def test_the_mission_names_the_playlist_the_profile_carries(self) -> None:
        self.assertEqual(TOKENS["PLAYLIST_NAME"], FIXTURE_PLAYLIST_NAME)
        self.assertEqual(
            section_handle(WORKLOAD, TOKENS, "Playlists"), FIXTURE_PLAYLIST_NAME
        )
        self.assertEqual(section_handle(WORKLOAD, TOKENS, "Music"), "Music")

    def test_the_hover_profile_has_enough_tracks_to_sweep(self) -> None:
        plan = build_plan(MISSION.profile)

        self.assertGreaterEqual(
            plan.playlist_track_count, WORKLOAD["min_targets_per_section"]
        )

    def test_only_the_profile_that_needs_a_playlist_gets_one(self) -> None:
        with_playlist = {
            name for name, plan in PLANS.items() if plan.playlist_track_count
        }

        self.assertEqual(with_playlist, {"mixed-sources-128"})

    def test_seeding_writes_the_playlist_and_its_tracks_in_order(self) -> None:
        connection = sqlite3.connect(":memory:")
        self.addCleanup(connection.close)
        connection.executescript(
            """
            CREATE TABLE playlists (
                id INTEGER PRIMARY KEY, name TEXT NOT NULL, position INTEGER NOT NULL
            );
            CREATE TABLE playlist_tracks (
                playlist_id INTEGER NOT NULL, track_id INTEGER NOT NULL,
                position INTEGER NOT NULL, PRIMARY KEY (playlist_id, position)
            );
            """
        )

        _seed_playlist(connection, 4)

        self.assertEqual(
            connection.execute("SELECT name, position FROM playlists").fetchall(),
            [(FIXTURE_PLAYLIST_NAME, 0)],
        )
        self.assertEqual(
            connection.execute(
                "SELECT track_id FROM playlist_tracks ORDER BY position"
            ).fetchall(),
            [(1,), (2,), (3,), (4,)],
        )


class SectionHandleContractTests(unittest.TestCase):
    def mission_with(self, **changes) -> pathlib.Path:
        raw = json.loads(MISSION_PATH.read_text(encoding="utf-8"))
        raw["workloads"][0].update(changes)
        handle = tempfile.NamedTemporaryFile("w", suffix=".json", delete=False)
        self.addCleanup(pathlib.Path(handle.name).unlink)
        json.dump(raw, handle)
        handle.close()
        return pathlib.Path(handle.name)

    def test_a_handle_for_a_section_that_is_not_swept_is_rejected(self) -> None:
        path = self.mission_with(section_handles={"Concerts": "PLAYLIST_NAME"})

        with self.assertRaisesRegex(ContractError, "not swept"):
            load_mission(path)

    def test_a_handle_naming_an_unknown_token_is_rejected(self) -> None:
        path = self.mission_with(section_handles={"Playlists": "NO_SUCH_TOKEN"})

        with self.assertRaisesRegex(ContractError, "unknown hover-sweep section handle"):
            load_mission(path)


class RecordedSidebarTests(unittest.TestCase):
    def test_the_recording_offers_the_playlist_but_no_section_named_playlists(
        self,
    ) -> None:
        offered = set(recorded_observation()["actionable_labels"])

        self.assertIn(FIXTURE_PLAYLIST_NAME, offered)
        self.assertNotIn("Playlists", offered)

    def test_the_explorer_opens_playlists_through_the_playlist_row(self) -> None:
        explorer = DeterministicExplorer(MISSION, 1)
        gateway = ActionGateway(MISSION)
        observation = recorded_observation()
        activated = []
        for index in range(MISSION.budgets.actions):
            action = explorer.propose(observation)
            self.assertIsNotNone(action)
            try:
                gateway.accept(action, observation)
            except ContractError as error:
                self.fail(f"the gateway rejected {action}: {error}")
            if action["kind"] == "complete-workload":
                gateway.confirm_workload(int(action["workload_index"]))
            if action["kind"] == "activate":
                activated.append(action["target"]["label"])
            if action["kind"] == "finish":
                break
            observation = {
                **observation,
                "state_id": f"s-{index + 1}",
                "state_signature": f"sig-{index + 1}",
            }

        self.assertIn(FIXTURE_PLAYLIST_NAME, activated)
        self.assertNotIn("Playlists", activated)
        playlists = [
            entry for entry in explorer.hover_coverage if entry["section"] == "Playlists"
        ]
        self.assertEqual(len(playlists), 1)
        self.assertTrue(playlists[0]["reachable"])

    def test_a_section_without_an_entry_is_still_reported_unreachable(self) -> None:
        raw = json.loads(RECORDED.read_text(encoding="utf-8"))
        raw["elements"] = [
            item for item in raw["elements"] if item.get("label") != FIXTURE_PLAYLIST_NAME
        ]

        class NoPlaylist(Transport):
            def __init__(self) -> None:
                self.raw = raw

        observation = CuaExecutor(
            NoPlaylist(), pid=1, window_id=2, session="recorded", settle_delays=()
        ).observe()
        explorer = DeterministicExplorer(MISSION, 1)
        for index in range(MISSION.budgets.actions):
            action = explorer.propose(observation)
            if action["kind"] == "finish":
                break
            observation = {
                **observation,
                "state_id": f"s-{index + 1}",
                "state_signature": f"sig-{index + 1}",
            }

        missed = [
            entry["section"] for entry in explorer.hover_coverage if not entry["reachable"]
        ]
        self.assertIn("Playlists", missed)


def hover_run(activations: list[str]) -> dict:
    """An audit over a run that activated each label, then hovered five buttons."""
    traces = []
    for section in activations:
        traces.append(
            ActionTrace(
                action={"kind": "activate", "target_label": section},
                state_changed=True,
            )
        )
        for index in range(5):
            traces.append(
                ActionTrace(
                    action={"kind": "hover", "target_label": f"Button {index}"},
                    state_changed=False,
                )
            )
    return audit_action_workload(0, WORKLOAD, traces, TOKENS)


class HoverAuditSectionTests(unittest.TestCase):
    SWEPT = [
        "Music",
        "Queue",
        FIXTURE_PLAYLIST_NAME,
        "Podcasts",
        "YouTube",
        "Radio",
        "My Stats",
    ]

    def test_the_playlist_row_counts_as_visiting_playlists(self) -> None:
        result = hover_run(self.SWEPT)

        self.assertTrue(result["sections_visited"]["Playlists"])
        self.assertEqual(result["hovered_per_section"]["Playlists"], 5)
        self.assertTrue(result["complete"])

    def test_without_the_playlist_the_section_stays_unvisited(self) -> None:
        result = hover_run([item for item in self.SWEPT if item != FIXTURE_PLAYLIST_NAME])

        self.assertFalse(result["sections_visited"]["Playlists"])
        self.assertFalse(result["complete"])

    def test_the_hovers_after_the_playlist_belong_to_it(self) -> None:
        result = hover_run(self.SWEPT)

        self.assertNotIn(FIXTURE_PLAYLIST_NAME, result["sections_visited"])
        self.assertEqual(sum(result["hovered_per_section"].values()), 7 * 5)


if __name__ == "__main__":
    unittest.main()
