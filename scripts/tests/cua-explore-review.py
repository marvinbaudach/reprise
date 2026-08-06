#!/usr/bin/env python3

import pathlib
import hashlib
import json
import sqlite3
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
sys.path.insert(0, str(EXPLORE_ROOT))

from fixtures import (  # noqa: E402
    FixtureError,
    audit_batch_edit,
    build_plan,
    validate_scratch_root,
)
from actions import ActivateAction, ResizeAction, TypeAction  # noqa: E402
from explorer import DeterministicExplorer  # noqa: E402
from oracles import ActionEvidence, OracleEngine, normalize_snapshot  # noqa: E402
from protocol import ActionGateway, ContractError, load_mission  # noqa: E402
from report import RunReport  # noqa: E402
from runner import _mission_for_agent, ensure_run_complete  # noqa: E402
from workload_audit import ActionTrace, audit_action_workload  # noqa: E402


class ScratchLocationReviewTests(unittest.TestCase):
    def test_large_profiles_use_an_approved_disk_backed_parent(self) -> None:
        cache_profile = (
            pathlib.Path.home()
            / ".cache"
            / "reprise-scratch"
            / "reprise-cua-explore-profile-review"
        )
        worktree_profile = (
            REPO_ROOT
            / ".worktrees"
            / "cua-explore-scratch"
            / "reprise-cua-explore-profile-review"
        )

        self.assertEqual(validate_scratch_root(cache_profile), cache_profile)
        self.assertEqual(validate_scratch_root(worktree_profile), worktree_profile)

    def test_large_profiles_reject_tmp_even_with_the_expected_prefix(self) -> None:
        with self.assertRaisesRegex(FixtureError, "disk-backed"):
            validate_scratch_root(
                pathlib.Path("/tmp/reprise-cua-explore-profile-review")
            )


def snapshot(elements, *, state_id="state"):
    return normalize_snapshot(
        {
            "screenshot_width": 1200,
            "screenshot_height": 800,
            "structuredContent": {"elements": elements},
        },
        state_id=state_id,
        captured_ms=0,
    )


def element(index, label, *, x=10, y=10, role="button"):
    return {
        "element_index": index,
        "label": label,
        "role": role,
        "frame": {"x": x, "y": y, "w": 100, "h": 32},
        "actions": ["click"] if role in {"button", "row"} else [],
        "enabled": True,
    }


class WaitingAndLayoutReviewTests(unittest.TestCase):
    def test_async_operation_without_feedback_uses_the_observation_window(self) -> None:
        state = snapshot([element(1, "Refresh")])

        findings = OracleEngine().analyze(
            ActionEvidence.activate(
                "Refresh",
                elapsed_ms=12,
                observation_ms=1_200,
                first_change_ms=None,
            ),
            state,
            state,
            settled=[state],
        )

        self.assertIn("missing-waiting-feedback", {item.code for item in findings})

    def test_direct_action_shift_is_allowed_but_idle_shift_is_reported(self) -> None:
        before = snapshot([element(1, "Track", y=100, role="row")])
        shifted = snapshot(
            [element(2, "Track", y=124, role="row")], state_id="shifted"
        )

        direct = OracleEngine().analyze(
            ActionEvidence.activate("Track", effect="confirmed"),
            before,
            before,
            settled=[before, shifted],
        )
        idle = OracleEngine().analyze(
            ActionEvidence(kind="wait", expect_effect="none"),
            before,
            before,
            settled=[before, shifted],
        )

        self.assertNotIn("uninvited-layout-shift", {item.code for item in direct})
        self.assertIn("uninvited-layout-shift", {item.code for item in idle})

    def test_wait_action_can_explicitly_expect_a_progress_surface(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "first-time-exploration.json"
        )
        gateway = ActionGateway(mission)
        observation = {
            "schema_version": 1,
            "state_id": "state-1",
            "actionable_labels": [],
        }

        action = gateway.accept(
            {
                "schema_version": 1,
                "state_id": "state-1",
                "kind": "wait",
                "duration_ms": 1_000,
                "expect_status": True,
            },
            observation,
        )

        self.assertEqual(action.duration_ms, 1_000)
        self.assertTrue(action.expect_status)

    def test_builtin_explorer_observes_async_controls_for_missing_progress(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "first-time-exploration.json"
        )
        explorer = DeterministicExplorer(mission, seed=3)
        first = explorer.propose(
            {
                "schema_version": 1,
                "state_id": "state-1",
                "state_signature": "one",
                "actionable_labels": ["Refresh Library"],
            }
        )
        second = explorer.propose(
            {
                "schema_version": 1,
                "state_id": "state-2",
                "state_signature": "two",
                "actionable_labels": ["Refresh Library"],
            }
        )

        self.assertEqual(first["kind"], "activate")
        self.assertEqual(second["kind"], "wait")
        self.assertTrue(second["expect_status"])

    def test_offline_keeps_cached_rows_and_online_clears_offline_status(self) -> None:
        cached = snapshot(
            [
                element(1, "Music"),
                element(2, "Fixture Podcast Needle", role="row"),
            ]
        )
        lost = snapshot([element(3, "Music")], state_id="lost")
        offline = snapshot(
            [element(4, "Music"), element(5, "No connection · Retry")],
            state_id="offline",
        )

        lost_findings = OracleEngine().analyze(
            ActionEvidence.connectivity("offline"), cached, lost, settled=[lost]
        )
        stale_findings = OracleEngine().analyze(
            ActionEvidence.connectivity("online"),
            offline,
            offline,
            settled=[offline],
        )

        self.assertIn(
            "offline-lost-cached-content", {item.code for item in lost_findings}
        )
        self.assertIn(
            "reconnect-kept-offline-status", {item.code for item in stale_findings}
        )


class WorkloadCompletionReviewTests(unittest.TestCase):
    def setUp(self) -> None:
        self.mission = load_mission(
            EXPLORE_ROOT / "missions" / "large-library-stress.json"
        )
        self.observation = {
            "schema_version": 1,
            "state_id": "state-1",
            "actionable_labels": [],
        }

    def test_stress_mission_pins_values_and_cannot_finish_before_checkpoints(self) -> None:
        self.assertEqual(self.mission.fixture_tokens["BATCH_GENRE"], "Exploratory Batch Genre")
        self.assertEqual(self.mission.fixture_tokens["BATCH_YEAR"], "2042")
        gateway = ActionGateway(self.mission)
        with self.assertRaisesRegex(ContractError, "workloads incomplete"):
            gateway.accept(
                {
                    "schema_version": 1,
                    "state_id": "state-1",
                    "kind": "finish",
                    "reason": "done",
                },
                self.observation,
            )

        for index in range(len(self.mission.workloads)):
            gateway.accept(
                {
                    "schema_version": 1,
                    "state_id": "state-1",
                    "kind": "complete-workload",
                    "workload_index": index,
                },
                self.observation,
            )
            gateway.confirm_workload(index)
        finished = gateway.accept(
            {
                "schema_version": 1,
                "state_id": "state-1",
                "kind": "finish",
                "reason": "all workload checkpoints recorded",
            },
            self.observation,
        )
        self.assertEqual(finished.kind, "finish")

    def test_typed_actions_do_not_carry_unrelated_optional_fields(self) -> None:
        gateway = ActionGateway(self.mission)
        observation = {
            **self.observation,
            "actionable_labels": ["Search all fields", "Title"],
        }
        typed = gateway.accept(
            {
                "schema_version": 1,
                "state_id": "state-1",
                "kind": "type",
                "target": {"label": "Search all fields"},
                "fixture_token": "SEARCH_NEEDLE",
            },
            observation,
        )
        activated = gateway.accept(
            {
                "schema_version": 1,
                "state_id": "state-1",
                "kind": "activate",
                "target": {"label": "Title"},
            },
            observation,
        )

        self.assertIsInstance(typed, TypeAction)
        self.assertIsInstance(activated, ActivateAction)
        self.assertNotIsInstance(typed, ResizeAction)
        self.assertFalse(hasattr(typed, "width"))

    def test_sort_checkpoint_requires_retained_sort_actions(self) -> None:
        workload = self.mission.workloads[1]
        empty = audit_action_workload(1, workload, [])
        one_column = [
            ActionTrace(
                action={"kind": "activate", "target_label": "Album"},
                before_labels=("Title",),
                after_labels=("Title",),
                before_rows=(("Before", 100.0),),
                after_rows=(("After", 100.0),),
                state_changed=True,
            )
            for _ in range(24)
        ]
        columns = ["Title", "Artist", "Album", "Year", "Rating"]
        traces = [
            ActionTrace(
                action={"kind": "activate", "target_label": columns[index % 5]},
                before_rows=((f"Before {index}", 100.0),),
                after_rows=((f"After {index}", 100.0),),
                state_changed=True,
            )
            for index in range(24)
        ]
        complete = audit_action_workload(1, workload, traces)

        self.assertFalse(empty["complete"])
        self.assertFalse(audit_action_workload(1, workload, one_column)["complete"])
        self.assertTrue(complete["complete"])
        self.assertEqual(complete["matching_actions"], 24)

    def test_filter_checkpoint_requires_each_facet_scoped_search_and_changed_rows(self) -> None:
        workload = self.mission.workloads[2]
        weak = [
            ActionTrace(
                action={"kind": "activate", "target_label": label},
                state_changed=True,
            )
            for label in ("Genre", "Year", "Rating")
        ] + [
            ActionTrace(
                action={
                    "kind": "type",
                    "target_label": "Anything",
                    "fixture_token": "NO_MATCH",
                },
                state_changed=True,
            )
        ]
        valid = [
            ActionTrace(
                action={"kind": "activate", "target_label": label},
                before_labels=tuple(
                    list(workload["active_labels"].values())[:index]
                ),
                after_labels=tuple(
                    list(workload["active_labels"].values())[: index + 1]
                ),
                before_rows=((f"Before {index}", 100.0),),
                after_rows=((f"After {index}", 100.0),),
                state_changed=True,
            )
            for index, label in enumerate(("Genre 00", "1993", "4"))
        ] + [
            ActionTrace(
                action={
                    "kind": "type",
                    "target_label": "Search all fields",
                    "fixture_token": "SEARCH_NEEDLE",
                },
                after_labels=(
                    "Genre: Genre 00  ×",
                    "Year: 1993  ×",
                    "Rating: 4  ×",
                ),
                after_rows=(("Needle 099700", 100.0),),
                state_changed=True,
            )
        ]

        self.assertFalse(
            audit_action_workload(2, workload, weak, self.mission.fixture_tokens)[
                "complete"
            ]
        )
        self.assertTrue(
            audit_action_workload(2, workload, valid, self.mission.fixture_tokens)[
                "complete"
            ]
        )
        lost_combination = list(valid)
        lost_combination[-1] = ActionTrace(
            action=valid[-1].action,
            after_rows=valid[-1].after_rows,
            state_changed=True,
        )
        self.assertFalse(
            audit_action_workload(
                2, workload, lost_combination, self.mission.fixture_tokens
            )["complete"]
        )

    def test_scroll_checkpoint_requires_observed_row_movement(self) -> None:
        workload = self.mission.workloads[3]
        requested_only = [
            ActionTrace(
                action={
                    "kind": "scroll",
                    "direction": direction,
                    "amount": 10,
                    "by": "page",
                },
                before_rows=(("Same", 100.0),),
                after_rows=(("Same", 100.0),),
            )
            for direction in ("down", "up")
            for _ in range(4)
        ]
        moved = [
            ActionTrace(
                action={
                    "kind": "scroll",
                    "direction": direction,
                    "amount": 10,
                    "by": "page",
                },
                before_rows=(("Before", 100.0),),
                after_rows=(("After", 100.0),),
                state_changed=True,
            )
            for direction in ("down", "up")
            for _ in range(4)
        ]

        self.assertFalse(audit_action_workload(3, workload, requested_only)["complete"])
        self.assertTrue(audit_action_workload(3, workload, moved)["complete"])

    def test_section_search_checkpoint_checks_rows_and_unsupported_lens(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "section-search-isolation.json"
        )
        workload = mission.workloads[0]
        traces = []
        for source, token_name in workload["route_tokens"].items():
            traces.append(
                ActionTrace(
                    action={"kind": "activate", "target_label": source},
                    after_selected_labels=(source,),
                    state_changed=True,
                )
            )
            traces.append(
                ActionTrace(
                    action={
                        "kind": "type",
                        "target_label": "Search all fields",
                        "fixture_token": token_name,
                    },
                    before_selected_labels=(source,),
                    after_selected_labels=(source,),
                    after_rows=((mission.fixture_tokens[token_name], 100.0),),
                    state_changed=True,
                )
            )
        traces.append(
            ActionTrace(
                action={"kind": "activate", "target_label": "My Stats"},
                after_actionable_labels=("Music", "Queue"),
                state_changed=True,
            )
        )

        self.assertTrue(
            audit_action_workload(0, workload, traces, mission.fixture_tokens)[
                "complete"
            ]
        )
        fake_lens = list(traces)
        fake_lens[-1] = ActionTrace(
            action={"kind": "activate", "target_label": "My Stats"},
            after_actionable_labels=("Search all fields",),
            state_changed=True,
        )
        self.assertFalse(
            audit_action_workload(0, workload, fake_lens, mission.fixture_tokens)[
                "complete"
            ]
        )

    def test_restart_checkpoint_validates_preserved_section_and_cleared_search(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "section-search-isolation.json"
        )
        workload = mission.workloads[1]
        valid = ActionTrace(
            action={"kind": "restart"},
            before_selected_labels=("Music",),
            after_selected_labels=("Music",),
            before_values=(
                ("Search all fields", mission.fixture_tokens["MUSIC_ONLY_NEEDLE"]),
            ),
            after_values=(("Search all fields", ""),),
            state_changed=True,
        )
        stale = ActionTrace(
            action={"kind": "restart"},
            before_selected_labels=("Music",),
            after_selected_labels=("Music",),
            before_values=(
                ("Search all fields", mission.fixture_tokens["MUSIC_ONLY_NEEDLE"]),
            ),
            after_values=(("Search all fields", "Needle"),),
            state_changed=True,
        )

        self.assertTrue(
            audit_action_workload(
                1, workload, [valid], mission.fixture_tokens
            )["complete"]
        )
        self.assertFalse(
            audit_action_workload(
                1, workload, [stale], mission.fixture_tokens
            )["complete"]
        )

    def test_batch_checkpoint_exercises_progress_selection_and_anchor_contract(self) -> None:
        workload = self.mission.workloads[0]
        weak = audit_action_workload(0, workload, [])
        traces = [
            ActionTrace(
                action={"kind": "scroll", "direction": "down", "amount": 1, "by": "page"},
                before_rows=(("Anchor", 100.0),),
                after_rows=(("Anchor", 80.0),),
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "activate", "target_label": "Edit Tags"},
                after_labels=("512 tracks selected",),
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
                action={"kind": "activate", "target_label": "Apply Changes"},
                after_labels=("512 tracks selected",),
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "wait", "expect_status": True},
                finding_codes=("missing-waiting-feedback",),
            ),
            ActionTrace(
                action={"kind": "scroll", "direction": "up", "amount": 1, "by": "page"},
                before_rows=(("Anchor", 80.0),),
                after_rows=(("Anchor", 100.0),),
                state_changed=True,
            ),
        ]

        self.assertFalse(weak["complete"])
        self.assertTrue(
            audit_action_workload(0, workload, traces, self.mission.fixture_tokens)[
                "complete"
            ]
        )
        broken_anchor = list(traces)
        broken_anchor[-1] = ActionTrace(
            action=traces[-1].action,
            before_rows=traces[-1].before_rows,
            after_rows=(("Anchor", 180.0),),
            state_changed=True,
        )
        self.assertFalse(
            audit_action_workload(
                0, workload, broken_anchor, self.mission.fixture_tokens
            )["complete"]
        )
        lost_selection = list(traces)
        lost_selection[4] = ActionTrace(
            action=traces[4].action,
            state_changed=True,
        )
        self.assertFalse(
            audit_action_workload(
                0, workload, lost_selection, self.mission.fixture_tokens
            )["complete"]
        )

    def test_offline_checkpoint_requires_source_refresh_retry_and_recovery(self) -> None:
        mission = load_mission(EXPLORE_ROOT / "missions" / "offline-recovery.json")
        workload = mission.workloads[0]
        actions = [
            {"kind": "activate", "target_label": "Podcasts"},
            {"kind": "activate", "target_label": "YouTube"},
            {"kind": "activate", "target_label": "Radio"},
            {"kind": "activate", "target_label": "Refresh"},
            {"kind": "set-connectivity", "connectivity": "offline"},
            {"kind": "activate", "target_label": "Podcasts"},
            {"kind": "activate", "target_label": "YouTube"},
            {"kind": "activate", "target_label": "Retry"},
            {"kind": "activate", "target_label": "Radio"},
            {"kind": "set-connectivity", "connectivity": "online"},
            {"kind": "activate", "target_label": "Podcasts"},
            {"kind": "activate", "target_label": "YouTube"},
            {"kind": "activate", "target_label": "Radio"},
        ]
        traces = [
            ActionTrace(
                action=action,
                before_labels=(
                    "Fixture Podcast Needle",
                    "Fixture YouTube Needle",
                    "Fixture Radio Needle",
                ),
                after_labels=(
                    "Fixture Podcast Needle",
                    "Fixture YouTube Needle",
                    "Fixture Radio Needle",
                ),
            )
            for action in actions
        ]

        self.assertIn("complete-workload", mission.capabilities)
        self.assertFalse(
            audit_action_workload(
                0, workload, traces[:-1], mission.fixture_tokens
            )["complete"]
        )
        self.assertTrue(
            audit_action_workload(
                0, workload, traces, mission.fixture_tokens
            )["complete"]
        )
        interrupted_late = list(traces)
        interrupted_late.insert(
            4,
            ActionTrace(
                action={"kind": "activate", "target_label": "Music"},
                before_labels=("Fixture Podcast Needle", "Fixture Radio Needle"),
                after_labels=("Fixture Podcast Needle", "Fixture Radio Needle"),
            ),
        )
        self.assertFalse(
            audit_action_workload(
                0, workload, interrupted_late, mission.fixture_tokens
            )["complete"]
        )

    def test_agent_receives_success_criteria_and_exhaustion_is_not_success(self) -> None:
        mission_payload = _mission_for_agent(self.mission)
        self.assertEqual(mission_payload["success"], list(self.mission.success))
        with self.assertRaisesRegex(Exception, "without finish"):
            ensure_run_complete(False, {"mission_complete": True})
        with self.assertRaisesRegex(Exception, "mission incomplete"):
            ensure_run_complete(True, {"mission_complete": False})

    def test_nested_workload_fields_fail_closed(self) -> None:
        raw = json.loads(
            (EXPLORE_ROOT / "missions" / "large-library-stress.json").read_text()
        )
        raw["workloads"][0]["shell"] = "touch outside"
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "mission.json"
            path.write_text(json.dumps(raw), encoding="utf-8")
            with self.assertRaisesRegex(
                ContractError, "unknown batch-edit workload field"
            ):
                load_mission(path)

    def test_batch_audit_requires_exact_database_values_and_changed_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            profile = pathlib.Path(directory)
            db_root = profile / "data" / "reprise"
            music = profile / "music"
            db_root.mkdir(parents=True)
            music.mkdir()
            baseline = b"unchanged fixture"
            baseline_hash = hashlib.sha256(baseline).hexdigest()
            with sqlite3.connect(db_root / "reprise.db") as conn:
                conn.execute(
                    "CREATE TABLE tracks (path TEXT, title TEXT, genre TEXT, year INTEGER)"
                )
                for index in range(2):
                    path = music / f"Writable Batch {index + 1:04}.flac"
                    path.write_bytes(baseline)
                    conn.execute(
                        "INSERT INTO tracks VALUES (?, ?, ?, ?)",
                        (str(path), f"Writable Batch {index + 1:04}", "Old", 2000),
                    )
            (profile / "fixture-manifest.json").write_text(
                json.dumps(
                    {
                        "writable_track_count": 2,
                        "writable_audio_sha256": baseline_hash,
                    }
                ),
                encoding="utf-8",
            )
            workload = {
                "kind": "batch-edit",
                "selection_count": 2,
                "field_tokens": {"genre": "BATCH_GENRE", "year": "BATCH_YEAR"},
            }
            tokens = {"BATCH_GENRE": "Exploratory Batch Genre", "BATCH_YEAR": "2042"}

            self.assertFalse(audit_batch_edit(profile, workload, tokens)["complete"])
            with sqlite3.connect(db_root / "reprise.db") as conn:
                conn.execute(
                    "UPDATE tracks SET genre = ?, year = ?",
                    (tokens["BATCH_GENRE"], int(tokens["BATCH_YEAR"])),
                )
            for path in music.glob("*.flac"):
                path.write_bytes(baseline + b" updated")

            audit = audit_batch_edit(profile, workload, tokens)

        self.assertTrue(audit["complete"])
        self.assertEqual(audit["database_rows_updated"], 2)
        self.assertEqual(audit["audio_files_changed"], 2)

    def test_report_marks_mission_complete_only_with_checkpoints_and_audits(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            report = RunReport(
                pathlib.Path(directory),
                mission_id="large-library-stress",
                profile="stress-100k",
                seed=7,
                commit="abc123",
                required_workloads=2,
                required_audits=(0,),
            )
            for index in range(2):
                report.add_step(
                    action={"kind": "complete-workload", "workload_index": index},
                    before_state="state-1",
                    after_state="state-1",
                    findings=[],
                )
            report.add_step(
                action={"kind": "finish", "reason": "done"},
                before_state="state-1",
                after_state="state-1",
                findings=[],
            )
            report.add_workload_audit(
                {"workload_index": 0, "kind": "batch-edit", "complete": False}
            )
            report.write()
            incomplete = json.loads(
                (pathlib.Path(directory) / "summary.json").read_text()
            )
            report.add_workload_audit(
                {"workload_index": 0, "kind": "batch-edit", "complete": True}
            )
            report.write()
            complete = json.loads(
                (pathlib.Path(directory) / "summary.json").read_text()
            )

        self.assertFalse(incomplete["mission_complete"])
        self.assertTrue(complete["mission_complete"])
        self.assertEqual(complete["completed_workload_indices"], [0, 1])


class ScopedSearchFixtureReviewTests(unittest.TestCase):
    def test_search_mission_uses_distinct_cached_source_rows_and_checkpoints(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "section-search-isolation.json"
        )
        plan = build_plan(mission.profile)

        self.assertEqual(mission.profile, "mixed-sources-128")
        self.assertEqual(plan.podcast_episode_count, 1)
        self.assertEqual(plan.radio_station_count, 1)
        self.assertEqual(
            mission.fixture_tokens["PODCAST_ONLY_NEEDLE"],
            "Fixture Podcast Needle",
        )
        self.assertEqual(
            mission.fixture_tokens["RADIO_ONLY_NEEDLE"],
            "Fixture Radio Needle",
        )
        self.assertIn("complete-workload", mission.capabilities)

        offline = load_mission(EXPLORE_ROOT / "missions" / "offline-recovery.json")
        self.assertEqual(offline.profile, "mixed-sources-128")


if __name__ == "__main__":
    unittest.main()
