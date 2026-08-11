#!/usr/bin/env python3

import json
import pathlib
import sys
import tempfile
import time
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
sys.path.insert(0, str(EXPLORE_ROOT))

from protocol import ActionGateway, ContractError, load_mission  # noqa: E402
from fixtures import FixtureError, build_plan, validate_scratch_root  # noqa: E402
from oracles import (  # noqa: E402
    ActionEvidence,
    OracleEngine,
    element_flag,
    normalize_snapshot,
)
from explorer import DeterministicExplorer  # noqa: E402
from agent_adapter import AgentError, ExternalAgent  # noqa: E402
from driver import CuaExecutor  # noqa: E402
from hover_geometry import WindowGeometry  # noqa: E402
from report import RunReport, confirm_findings, minimize_actions  # noqa: E402
from runner import app_launch_argv  # noqa: E402


class AppLaunchArgvTests(unittest.TestCase):
    def test_app_launch_argv_keeps_a_private_network_namespace(self) -> None:
        argv = app_launch_argv(pathlib.Path("/fixture/reprise"))

        self.assertIn("--net", argv)

    def test_app_launch_argv_does_not_map_root_because_dbus_external_auth_rejects_it(
        self,
    ) -> None:
        argv = app_launch_argv(pathlib.Path("/fixture/reprise"))

        self.assertIn("--map-current-user", argv)
        self.assertNotIn("--map-root-user", argv)

    def test_app_launch_argv_puts_the_binary_after_the_argument_separator(self) -> None:
        binary = pathlib.Path("/fixture/reprise")
        argv = app_launch_argv(binary)

        self.assertEqual(argv[-2:], ["--", str(binary)])


class MissionContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.mission_path = EXPLORE_ROOT / "missions" / "large-library-stress.json"

    def test_large_library_mission_pins_stress_and_batch_boundaries(self) -> None:
        mission = load_mission(self.mission_path)

        self.assertEqual(mission.profile, "stress-100k")
        self.assertEqual(mission.persona, "experienced library power user")
        self.assertEqual(mission.budgets.actions, 130)
        self.assertIn("feedback", mission.oracles)
        self.assertIn("layout-shift", mission.oracles)
        self.assertIn("pointer-reachability", mission.oracles)
        workloads = {workload["kind"]: workload for workload in mission.workloads}
        self.assertEqual(workloads["batch-edit"]["selection_count"], 512)
        self.assertGreaterEqual(workloads["sort-cycle"]["repetitions"], 20)
        self.assertGreaterEqual(len(workloads["combined-filter"]["facets"]), 3)

    def test_unknown_mission_fields_fail_closed(self) -> None:
        raw = json.loads(self.mission_path.read_text(encoding="utf-8"))
        raw["shell"] = "rm -rf /"
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "mission.json"
            path.write_text(json.dumps(raw), encoding="utf-8")
            with self.assertRaisesRegex(ContractError, "unknown mission field"):
                load_mission(path)

    def test_every_supplied_mission_has_a_valid_persona_contract(self) -> None:
        missions = [load_mission(path) for path in sorted((EXPLORE_ROOT / "missions").glob("*.json"))]

        self.assertEqual(
            {mission.mission_id for mission in missions},
            {
                "first-time-exploration",
                "hover-affordance-sweep",
                "large-library-stress",
                "offline-recovery",
                "pointer-layout-reachability",
                "section-search-isolation",
            },
        )
        self.assertTrue(all(mission.persona for mission in missions))


class ActionGatewayTests(unittest.TestCase):
    def setUp(self) -> None:
        self.mission = load_mission(
            EXPLORE_ROOT / "missions" / "first-time-exploration.json"
        )
        self.gateway = ActionGateway(self.mission)
        self.observation = {
            "schema_version": 1,
            "state_id": "state-4",
            "actionable_labels": ["Music", "Search all fields", "Retry"],
        }

    def test_semantic_action_is_bound_to_the_fresh_observation(self) -> None:
        action = self.gateway.accept(
            {
                "schema_version": 1,
                "state_id": "state-4",
                "kind": "activate",
                "target": {"label": "Music"},
                "dispatch": "ax",
                "expect_effect": "required",
            },
            self.observation,
        )

        self.assertEqual(action.target_label, "Music")
        self.assertEqual(action.dispatch, "ax")

    def test_stale_or_unknown_targets_are_rejected(self) -> None:
        for state_id, label, message in [
            ("state-3", "Music", "stale observation"),
            ("state-4", "Delete Library", "not actionable"),
        ]:
            with self.subTest(state_id=state_id, label=label):
                with self.assertRaisesRegex(ContractError, message):
                    self.gateway.accept(
                        {
                            "schema_version": 1,
                            "state_id": state_id,
                            "kind": "activate",
                            "target": {"label": label},
                        },
                        self.observation,
                    )

    def test_forbidden_destructive_targets_are_rejected_for_external_agents(self) -> None:
        observation = {
            **self.observation,
            "actionable_labels": [*self.observation["actionable_labels"], "Delete Library"],
        }
        with self.assertRaisesRegex(ContractError, "forbidden target"):
            self.gateway.accept(
                {
                    "schema_version": 1,
                    "state_id": "state-4",
                    "kind": "activate",
                    "target": {"label": "Delete Library"},
                },
                observation,
            )

    def test_text_actions_accept_fixture_tokens_not_raw_text_or_urls(self) -> None:
        accepted = self.gateway.accept(
            {
                "schema_version": 1,
                "state_id": "state-4",
                "kind": "type",
                "target": {"label": "Search all fields"},
                "fixture_token": "SEARCH_NEEDLE",
            },
            self.observation,
        )
        self.assertEqual(accepted.fixture_token, "SEARCH_NEEDLE")

        for unsafe in [
            {"text": "anything"},
            {"fixture_token": "https://outside.example"},
            {"fixture_token": "SHELL_COMMAND"},
        ]:
            action = {
                "schema_version": 1,
                "state_id": "state-4",
                "kind": "type",
                "target": {"label": "Search all fields"},
                **unsafe,
            }
            with self.assertRaises(ContractError):
                self.gateway.accept(action, self.observation)

    def test_budget_exhaustion_fails_closed(self) -> None:
        self.gateway._accepted_actions = self.mission.budgets.actions
        with self.assertRaisesRegex(ContractError, "action budget exhausted"):
            self.gateway.accept(
                {
                    "schema_version": 1,
                    "state_id": "state-4",
                    "kind": "finish",
                    "reason": "done",
                },
                self.observation,
            )


class FixtureProfileTests(unittest.TestCase):
    def test_stress_profile_combines_a_large_catalog_with_real_batch_files(self) -> None:
        plan = build_plan("stress-100k")

        self.assertEqual(plan.track_count, 100_000)
        self.assertEqual(plan.writable_track_count, 512)
        self.assertTrue(plan.generated_metadata_only)
        self.assertIn("genre", plan.metadata_dimensions)
        self.assertIn("rating", plan.metadata_dimensions)

    def test_writable_profile_keeps_every_batch_target_disposable(self) -> None:
        plan = build_plan("writable-512")

        self.assertEqual(plan.track_count, 512)
        self.assertEqual(plan.writable_track_count, 512)
        self.assertTrue(plan.all_paths_disposable)

    def test_fixture_root_rejects_existing_and_real_user_locations(self) -> None:
        for unsafe in [
            pathlib.Path.home() / "Music",
            pathlib.Path.home() / ".local" / "share" / "reprise",
            REPO_ROOT,
        ]:
            with self.subTest(path=unsafe):
                with self.assertRaises(FixtureError):
                    validate_scratch_root(unsafe)

        with tempfile.TemporaryDirectory(prefix="reprise-cua-explore-existing-") as directory:
            with self.assertRaisesRegex(FixtureError, "already exists"):
                validate_scratch_root(pathlib.Path(directory))

        unsafe_child = REPO_ROOT / "reprise-cua-explore-unsafe"
        with self.assertRaisesRegex(FixtureError, "protected"):
            validate_scratch_root(unsafe_child)


def window_element(x=0, y=0, w=1200, h=800, **extra):
    """The depth-0 root cua-driver puts in front of every real snapshot."""
    return {
        "element_index": 0,
        "label": "Reprise",
        "role": "frame",
        "depth": 0,
        "parent_index": None,
        "frame": {"x": x, "y": y, "w": w, "h": h},
        "actions": [],
        "enabled": True,
        **extra,
    }


def snapshot(elements, *, state_id="state", width=1200, height=800, window=True):
    raw = {"structuredContent": {"elements": list(elements)}}
    if width is not None and height is not None:
        raw["screenshot_width"] = width
        raw["screenshot_height"] = height
    if window is True:
        window = (0, 0, width or 1200, height or 800)
    if window is not None:
        raw["structuredContent"]["elements"].insert(0, window_element(*window))
    return normalize_snapshot(raw, state_id=state_id, captured_ms=0)


SNAPSHOT_ID = "s00000001"


def raw_snapshot(elements, *, width=800, height=600):
    """The envelope cua-driver returns: the snapshot names itself.

    Every element_token embeds that name, and the address contract refuses a
    token it cannot check against one.
    """
    return {
        "snapshot_id": SNAPSHOT_ID,
        "screenshot_width": width,
        "screenshot_height": height,
        "structuredContent": {"elements": list(elements)},
    }


def element(index, label, role="button", x=10, y=10, w=100, h=32, **extra):
    return {
        "element_index": index,
        "element_token": f"{SNAPSHOT_ID}:{index}",
        "label": label,
        "role": role,
        "frame": {"x": x, "y": y, "w": w, "h": h},
        "actions": ["click"] if role in {"button", "row"} else [],
        "enabled": True,
        **extra,
    }


def driver_element(index, label, role, x, y, w, h, *, depth=3, parent_index=0, enabled=True):
    """Exactly the key set cua-driver emits: no 'visible', no 'states'."""
    return {
        "depth": depth,
        "element_index": index,
        "element_token": f"tok-{index}",
        "enabled": enabled,
        "frame": {"x": x, "y": y, "w": w, "h": h},
        "label": label,
        "parent_index": parent_index,
        "role": role,
        "value": "",
    }


class DriverFieldSetTests(unittest.TestCase):
    """The oracle must not invent findings from fields the driver never sends."""

    def setUp(self) -> None:
        self.engine = OracleEngine()
        self.raw = {
            "structuredContent": {
                "elements": [
                    driver_element(
                        0, "Reprise", "frame", 200, 50, 1200, 800,
                        depth=0, parent_index=None,
                    ),
                    driver_element(7, "Add filter", "button", 260, 110, 96, 34),
                    driver_element(9, "Music", "row", 220, 200, 400, 28),
                    driver_element(
                        11, "Save", "button", 900, 700, 80, 30, enabled=False
                    ),
                ]
            }
        }

    def _snapshot(self):
        return normalize_snapshot(self.raw, state_id="probe", captured_ms=0)

    def test_absent_visible_key_falls_back_to_the_declared_default(self) -> None:
        elements = {element.label: element for element in self._snapshot().elements}

        self.assertTrue(elements["Add filter"].visible)
        self.assertTrue(elements["Add filter"].enabled)
        self.assertFalse(elements["Save"].enabled)

    def test_absent_focus_and_selection_keys_stay_false(self) -> None:
        elements = {element.label: element for element in self._snapshot().elements}

        self.assertFalse(elements["Add filter"].focused)
        self.assertFalse(elements["Add filter"].selected)

    def test_a_real_driver_snapshot_produces_no_findings(self) -> None:
        findings = self.engine.inspect_snapshot(self._snapshot())

        self.assertEqual([finding.code for finding in findings], [])

    def test_an_element_outside_the_window_is_still_reported(self) -> None:
        self.raw["structuredContent"]["elements"].append(
            driver_element(13, "Off window", "button", 1500, 110, 60, 34)
        )

        findings = self.engine.inspect_snapshot(self._snapshot())

        self.assertEqual(
            [finding.code for finding in findings], ["invisible-actionable"]
        )


class BooleanStateTests(unittest.TestCase):
    def test_a_direct_boolean_wins(self) -> None:
        self.assertFalse(element_flag({"visible": False}, "visible", True))
        self.assertTrue(element_flag({"visible": True}, "visible", False))

    def test_a_present_states_list_decides(self) -> None:
        self.assertTrue(
            element_flag({"states": ["visible", "enabled"]}, "visible", False)
        )
        self.assertFalse(
            element_flag({"states": ["enabled"]}, "visible", True)
        )

    def test_a_direct_boolean_outranks_the_states_list(self) -> None:
        self.assertFalse(
            element_flag({"visible": False, "states": ["visible"]}, "visible", True)
        )

    def test_neither_key_nor_states_falls_back_to_the_default(self) -> None:
        self.assertTrue(element_flag({}, "visible", True))
        self.assertFalse(element_flag({}, "selected", False))

    def test_an_empty_states_list_carries_no_information(self) -> None:
        self.assertTrue(element_flag({"states": []}, "visible", True))
        self.assertFalse(element_flag({"states": []}, "selected", False))


class SlowRoundTripTransport:
    """A driver whose get_window_state costs real time, like the CLI does."""

    def __init__(self, round_trip_seconds=0.06):
        self.round_trip_seconds = round_trip_seconds
        self.calls = []

    def call(self, tool, payload):
        self.calls.append(tool)
        if tool == "get_window_state":
            time.sleep(self.round_trip_seconds)
            return raw_snapshot([
                        driver_element(
                            0, "Reprise", "frame", 0, 0, 800, 600,
                            depth=0, parent_index=None,
                        ),
                        driver_element(4, "Music", "button", 10, 10, 90, 30),
                    ])
        return {"effect": "confirmed", "verified": True}

    def resize_window(self, window_id, width, height):
        return {"effect": "unverifiable"}

    def set_connectivity(self, state):
        return {"effect": "confirmed"}

    def wmctrl_geometry(self, window_id):
        raise AssertionError("not used")


class DriverTimingEvidenceTests(unittest.TestCase):
    """The driver must hand the oracles its own cost, or nothing is subtracted."""

    def _run_step(self, settle_delays):
        transport = SlowRoundTripTransport()
        executor = CuaExecutor(
            transport,
            pid=1,
            window_id=2,
            session="test",
            settle_delays=settle_delays,
        )
        return executor._execute(
            None, ActionEvidence.connectivity("online")
        )

    def test_the_requested_settle_time_is_reported(self) -> None:
        result = self._run_step((0.02, 0.03))

        self.assertEqual(result.evidence.settle_delay_ms, 50)

    def test_every_snapshot_after_dispatch_is_measured(self) -> None:
        result = self._run_step((0.01, 0.01))

        # after-snapshot plus one per settle sample, and none of them free.
        self.assertEqual(len(result.evidence.snapshot_ms), 3)
        self.assertTrue(all(value > 0 for value in result.evidence.snapshot_ms))

    def test_a_quiet_app_produces_no_timing_findings_through_the_real_path(
        self,
    ) -> None:
        result = self._run_step((0.01, 0.02, 0.03))

        codes = {finding.code for finding in result.findings}
        self.assertNotIn("main-loop-stall", codes)
        self.assertNotIn("missing-waiting-feedback", codes)
        self.assertNotIn("slow-visible-feedback", codes)

    def test_the_reported_observation_time_still_holds_the_raw_wall_time(self) -> None:
        result = self._run_step((0.05,))

        harness = result.evidence.settle_delay_ms + sum(result.evidence.snapshot_ms)
        self.assertGreaterEqual(result.evidence.observation_ms, harness - 5)


class TimingAttributionTests(unittest.TestCase):
    """Timing oracles must judge the app, never the harness's own cost."""

    # Shape measured in a real run: one cua-driver round-trip costs ~470 ms
    # (subprocess spawn, tree walk, PNG), and the settle schedule sleeps
    # 100 + 250 + 500 = 850 ms on purpose.
    REAL_SNAPSHOTS = (472, 467, 481, 495)
    REAL_SETTLE_MS = 850

    def setUp(self) -> None:
        self.engine = OracleEngine()
        self.state = snapshot([element(1, "Music", visible=True)])

    def _codes(self, **evidence):
        defaults = {
            "kind": "activate",
            "target_label": "Music",
            "expect_effect": "required",
            "elapsed_ms": 5,
            "observation_ms": 2765,
            "first_change_ms": None,
            "settle_delay_ms": self.REAL_SETTLE_MS,
            "snapshot_ms": self.REAL_SNAPSHOTS,
            "snapshot_ms_before_first_change": 0,
            "sample_gaps_ms": self.REAL_SNAPSHOTS[1:],
        }
        defaults.update(evidence)
        findings = self.engine.analyze(
            ActionEvidence(**defaults), self.state, self.state, settled=(self.state,)
        )
        return [finding.code for finding in findings]

    def test_the_drivers_own_round_trip_is_not_a_main_loop_stall(self) -> None:
        self.assertNotIn("main-loop-stall", self._codes())

    def test_a_sample_far_above_the_steps_own_baseline_is_a_stall(self) -> None:
        codes = self._codes(
            snapshot_ms=(472, 467, 481, 1400), sample_gaps_ms=(467, 481, 1400)
        )

        self.assertIn("main-loop-stall", codes)

    def test_the_stall_evidence_reports_the_excess_not_the_wall_time(self) -> None:
        findings = self.engine.analyze(
            ActionEvidence(
                kind="activate",
                target_label="Music",
                expect_effect="required",
                observation_ms=2765,
                first_change_ms=None,
                settle_delay_ms=self.REAL_SETTLE_MS,
                snapshot_ms=(470, 1400),
                snapshot_ms_before_first_change=0,
                sample_gaps_ms=(1400,),
            ),
            self.state,
            self.state,
            settled=(self.state,),
        )
        stall = next(item for item in findings if item.code == "main-loop-stall")

        self.assertEqual(stall.evidence["excess_ms"], [930])
        self.assertEqual(stall.evidence["baseline_ms"], 470)

    def test_the_harnesss_own_waiting_is_not_missing_feedback(self) -> None:
        self.assertNotIn("missing-waiting-feedback", self._codes())

    def test_an_app_that_really_keeps_us_waiting_is_still_reported(self) -> None:
        # 850 ms of deliberate sleep plus 1915 ms of round-trips leaves the app
        # itself accountable for a full second here.
        codes = self._codes(observation_ms=3800)

        self.assertIn("missing-waiting-feedback", codes)

    def test_an_explicit_wait_still_demands_a_visible_status(self) -> None:
        codes = self._codes(kind="wait", expect_status=True, observation_ms=900)

        self.assertIn("missing-waiting-feedback", codes)

    def test_feedback_first_seen_after_our_own_blind_time_is_not_late(self) -> None:
        # The change was noticed 500 ms in, but 470 ms of that was one snapshot
        # during which we structurally could not have seen anything.
        codes = self._codes(first_change_ms=500, snapshot_ms_before_first_change=470)

        self.assertNotIn("slow-visible-feedback", codes)

    def test_feedback_the_app_really_delayed_is_still_reported(self) -> None:
        codes = self._codes(first_change_ms=1400, snapshot_ms_before_first_change=470)

        self.assertIn("slow-visible-feedback", codes)

    def test_a_change_seen_immediately_after_dispatch_is_never_late(self) -> None:
        codes = self._codes(first_change_ms=8, snapshot_ms_before_first_change=0)

        self.assertNotIn("slow-visible-feedback", codes)


class UxOracleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.engine = OracleEngine()

    def test_click_without_handler_is_distinct_from_occlusion(self) -> None:
        before = snapshot([element(1, "Retry")])
        after = snapshot([element(2, "Retry")], state_id="after")

        no_handler = self.engine.analyze(
            ActionEvidence.activate("Retry", dispatch="ax", effect="suspected_noop"),
            before,
            after,
        )
        occluded = self.engine.analyze(
            ActionEvidence.activate(
                "Retry", dispatch="px", effect="unverifiable", ax_probe_changed=True
            ),
            before,
            after,
        )

        self.assertIn("suspected-no-handler", {finding.code for finding in no_handler})
        self.assertIn("suspected-occlusion", {finding.code for finding in occluded})

    def test_actionable_element_outside_window_is_reported(self) -> None:
        state = snapshot([element(1, "Hidden Save", x=1300, y=20)])

        findings = self.engine.inspect_snapshot(state)

        self.assertIn("invisible-actionable", {finding.code for finding in findings})

    def test_a_snapshot_without_screenshot_dimensions_is_judged_by_the_window(
        self,
    ) -> None:
        # The settling probes are fetched without screenshot_out_file, so they
        # carry no screenshot_width/height. Reading that as a 0x0 window turned
        # every visible control into an "outside the visible window" error.
        state = snapshot(
            [element(1, "Add filter", x=260, y=110, visible=True)],
            width=None,
            height=None,
            window=(200, 50, 1200, 800),
        )

        findings = self.engine.inspect_snapshot(state)

        self.assertEqual([finding.code for finding in findings], [])

    def test_the_window_rectangle_is_read_in_screen_coordinates(self) -> None:
        # Element frames are screen coordinates: a child at the top-left window
        # corner reports the window's own origin. Comparing them against the
        # screenshot size would call the right-hand half of the window invisible.
        state = snapshot(
            [
                element(1, "Sidebar", x=200, y=50, visible=True),
                element(2, "Right edge", x=1350, y=60, visible=True),
            ],
            width=1200,
            height=800,
            window=(200, 50, 1200, 800),
        )

        findings = self.engine.inspect_snapshot(state)

        self.assertEqual([finding.code for finding in findings], [])

    def test_an_element_beyond_the_window_rectangle_is_still_reported(self) -> None:
        state = snapshot(
            [element(1, "Hidden Save", x=1500, y=60, visible=True)],
            width=1200,
            height=800,
            window=(200, 50, 1200, 800),
        )

        findings = self.engine.inspect_snapshot(state)

        self.assertEqual(
            [finding.code for finding in findings], ["invisible-actionable"]
        )

    def test_a_snapshot_without_any_geometry_is_not_judged(self) -> None:
        state = snapshot(
            [element(1, "Add filter", x=260, y=110, visible=True)],
            width=None,
            height=None,
            window=None,
        )

        findings = self.engine.inspect_snapshot(state)

        self.assertEqual([finding.code for finding in findings], [])

    def test_layout_shift_during_an_explicit_idle_probe_is_uninvited(self) -> None:
        before = snapshot(
            [element(1, "Search", x=900, y=10), element(2, "Track A", role="row", y=100)]
        )
        after = snapshot(
            [element(3, "Search", x=900, y=10), element(4, "Track A", role="row", y=100)],
            state_id="after",
        )
        settled_late = snapshot(
            [element(5, "Search", x=900, y=28), element(6, "Track A", role="row", y=124)],
            state_id="late",
        )

        findings = self.engine.analyze(
            ActionEvidence(kind="wait", expect_effect="none"),
            before,
            after,
            settled=[after, settled_late],
        )

        layout = [finding for finding in findings if finding.code == "uninvited-layout-shift"]
        self.assertEqual(len(layout), 1)
        self.assertEqual(layout[0].severity, "warning")

    def test_downward_scroll_catches_opposite_motion_and_lost_selection(self) -> None:
        before = snapshot(
            [
                element(1, "Track A", role="row", y=100, selected=True),
                element(2, "Track B", role="row", y=140),
            ]
        )
        after = snapshot(
            [
                element(3, "Track A", role="row", y=180, selected=False),
                element(4, "Track B", role="row", y=220),
            ],
            state_id="after",
        )

        findings = self.engine.analyze(
            ActionEvidence.scroll("down", amount=1, by="page"), before, after
        )
        codes = {finding.code for finding in findings}

        self.assertIn("wrong-scroll-direction", codes)
        self.assertIn("scroll-lost-selection", codes)

    def test_slow_action_without_status_is_a_warning_not_a_hard_verdict(self) -> None:
        before = snapshot([element(1, "Refresh")])
        after = snapshot([element(2, "Refresh")], state_id="after")

        findings = self.engine.analyze(
            ActionEvidence.activate(
                "Refresh",
                effect="confirmed",
                elapsed_ms=1400,
                observation_ms=1400,
                first_change_ms=None,
            ),
            before,
            after,
        )
        waiting = [finding for finding in findings if finding.code == "missing-waiting-feedback"]

        self.assertEqual(len(waiting), 1)
        self.assertEqual(waiting[0].severity, "warning")
        self.assertFalse(waiting[0].blocks_gate)

    def test_offline_transition_keeps_local_music_reachable(self) -> None:
        before = snapshot([element(1, "Music"), element(2, "Podcasts")])
        after = snapshot([element(3, "Podcasts")], state_id="after")

        findings = self.engine.analyze(
            ActionEvidence.connectivity("offline"), before, after
        )

        self.assertIn("offline-broke-local-music", {finding.code for finding in findings})

    def test_offline_continuity_uses_the_settled_projected_state(self) -> None:
        before = snapshot([element(1, "Music"), element(2, "Podcasts")])
        immediate = snapshot(
            [element(3, "Music"), element(4, "Podcasts")], state_id="immediate"
        )
        projected = snapshot([element(5, "Podcasts")], state_id="projected")

        findings = self.engine.analyze(
            ActionEvidence.connectivity("offline"),
            before,
            immediate,
            settled=[immediate, projected],
        )

        self.assertIn("offline-broke-local-music", {finding.code for finding in findings})

    def test_progress_surface_prevents_missing_waiting_false_positive(self) -> None:
        before = snapshot([element(1, "Refresh")])
        after = snapshot(
            [element(2, "Refresh"), element(3, "Scanning library", role="progress bar")],
            state_id="after",
        )

        findings = self.engine.analyze(
            ActionEvidence.activate(
                "Refresh", effect="confirmed", elapsed_ms=1400, first_change_ms=None
            ),
            before,
            after,
        )

        self.assertNotIn("missing-waiting-feedback", {finding.code for finding in findings})

    def test_expected_resize_does_not_use_the_settled_layout_shift_oracle(self) -> None:
        before = snapshot([element(1, "Search", x=900, y=10)])
        after = snapshot([element(2, "Search", x=500, y=10)], state_id="after", width=720)

        findings = self.engine.analyze(
            ActionEvidence(kind="resize"), before, after, settled=[]
        )

        self.assertNotIn("uninvited-layout-shift", {finding.code for finding in findings})


class DeterministicExplorerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.mission = load_mission(
            EXPLORE_ROOT / "missions" / "first-time-exploration.json"
        )
        self.observation = {
            "schema_version": 1,
            "state_id": "state-1",
            "actionable_labels": [
                "Music",
                "Queue",
                "Search all fields",
                "Delete Library",
            ],
            "state_signature": "alpha",
        }

    def test_same_seed_and_history_produce_the_same_action(self) -> None:
        left = DeterministicExplorer(self.mission, seed=42)
        right = DeterministicExplorer(self.mission, seed=42)

        self.assertEqual(left.propose(self.observation), right.propose(self.observation))

    def test_explorer_never_activates_destructive_or_forbidden_targets(self) -> None:
        explorer = DeterministicExplorer(self.mission, seed=7)
        actions = []
        observation = dict(self.observation)
        for index in range(12):
            observation["state_id"] = f"state-{index}"
            observation["state_signature"] = f"state-{index % 3}"
            actions.append(explorer.propose(observation))

        activated = {
            action.get("target", {}).get("label")
            for action in actions
            if action.get("kind") == "activate"
        }
        self.assertNotIn("Delete Library", activated)
        self.assertTrue({"Music", "Queue"} & activated)

    def test_pointer_mission_uses_real_pixel_dispatch(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "pointer-layout-reachability.json"
        )
        explorer = DeterministicExplorer(mission, seed=7)
        observation = {
            "schema_version": 1,
            "state_id": "state-1",
            "actionable_labels": ["Retry"],
            "state_signature": "alpha",
        }

        action = explorer.propose(observation)

        self.assertEqual(action["kind"], "activate")
        self.assertEqual(action["dispatch"], "px")

    def test_builtin_mission_schedules_every_declared_workload_before_finish(self) -> None:
        explorer = DeterministicExplorer(self.mission, seed=7)
        actions = []
        for index in range(40):
            action = explorer.propose(
                {
                    "schema_version": 1,
                    "state_id": f"state-{index}",
                    "actionable_labels": ["Music", "Queue"],
                    "state_signature": f"signature-{index}",
                }
            )
            actions.append(action)
            if action["kind"] == "finish":
                break

        completed = [
            action["workload_index"]
            for action in actions
            if action["kind"] == "complete-workload"
        ]
        self.assertEqual(completed, list(range(len(self.mission.workloads))))
        self.assertIn("restart", {action["kind"] for action in actions})
        self.assertEqual(actions[-1]["kind"], "finish")

    def test_offline_mission_requires_a_reasoning_agent_and_audited_checkpoints(self) -> None:
        mission = load_mission(EXPLORE_ROOT / "missions" / "offline-recovery.json")
        self.assertIn("complete-workload", mission.capabilities)
        self.assertEqual(
            mission.workloads[0]["source_tokens"],
            {
                "Podcasts": "PODCAST_ONLY_NEEDLE",
                "YouTube": "YOUTUBE_ONLY_NEEDLE",
                "Radio": "RADIO_ONLY_NEEDLE",
            },
        )


class ExternalAgentTests(unittest.TestCase):
    def test_jsonl_agent_receives_a_bounded_task_and_returns_one_action(self) -> None:
        program = (
            "import json,sys; request=json.loads(sys.stdin.readline()); "
            "print(json.dumps({'schema_version':1,'state_id':"
            "request['observation']['state_id'],'kind':'finish','reason':'done'}), flush=True)"
        )
        with ExternalAgent([sys.executable, "-c", program], timeout_seconds=1) as agent:
            action = agent.propose(
                {"id": "bounded", "goal": "Explore", "budgets": {"actions": 2}},
                {"state_id": "state-9", "actionable_labels": []},
                [],
            )

        self.assertEqual(action["state_id"], "state-9")
        self.assertEqual(action["kind"], "finish")

    def test_external_agent_receives_the_disposable_home(self) -> None:
        program = (
            "import json,os,sys; request=json.loads(sys.stdin.readline()); "
            "print(json.dumps({'state_id':request['observation']['state_id'],"
            "'home':os.environ.get('HOME')}), flush=True)"
        )
        with tempfile.TemporaryDirectory() as directory:
            private_home = pathlib.Path(directory)
            with ExternalAgent(
                [sys.executable, "-c", program],
                timeout_seconds=1,
                private_home=private_home,
            ) as agent:
                action = agent.propose(
                    {"id": "bounded"}, {"state_id": "state-9"}, []
                )

        self.assertEqual(action["home"], str(private_home))
        self.assertNotEqual(action["home"], str(pathlib.Path.home()))

    def test_malformed_or_silent_agent_fails_closed(self) -> None:
        malformed = "import sys; sys.stdin.readline(); print('not-json', flush=True)"
        with ExternalAgent([sys.executable, "-c", malformed], timeout_seconds=1) as agent:
            with self.assertRaisesRegex(AgentError, "invalid JSON"):
                agent.propose({"id": "bounded"}, {"state_id": "s"}, [])

        silent = "import sys,time; sys.stdin.readline(); time.sleep(1)"
        with ExternalAgent([sys.executable, "-c", silent], timeout_seconds=0.05) as agent:
            with self.assertRaisesRegex(AgentError, "timed out"):
                agent.propose({"id": "bounded"}, {"state_id": "s"}, [])


class FakeTransport:
    def __init__(self, snapshots):
        self.snapshots = list(snapshots)
        self.calls = []

    def call(self, tool, payload):
        self.calls.append((tool, dict(payload)))
        if tool == "get_window_state":
            return self.snapshots.pop(0)
        return {"effect": "confirmed", "verified": True}

    def resize_window(self, window_id, width, height):
        self.calls.append(("resize_window", {"window_id": window_id, "width": width, "height": height}))
        return {"effect": "unverifiable"}

    def set_connectivity(self, state):
        self.calls.append(("set_connectivity", {"state": state}))
        return {"effect": "confirmed"}


class CuaExecutorTests(unittest.TestCase):
    def test_action_is_bracketed_and_uses_token_from_fresh_pre_action_snapshot(self) -> None:
        initial = raw_snapshot([element(3, "Music")])
        fresh = raw_snapshot([element(9, "Music")])
        after = raw_snapshot([element(12, "Music", selected=True)])
        transport = FakeTransport([initial, fresh, after])
        executor = CuaExecutor(
            transport, pid=44, window_id=77, session="test", settle_delays=()
        )
        observation = executor.observe()
        action = ActionGateway(
            load_mission(EXPLORE_ROOT / "missions" / "first-time-exploration.json")
        ).accept(
            {
                "schema_version": 1,
                "state_id": observation["state_id"],
                "kind": "activate",
                "target": {"label": "Music"},
            },
            observation,
        )

        result = executor.execute(action)

        self.assertEqual(
            [tool for tool, _payload in transport.calls],
            ["get_window_state", "get_window_state", "click", "get_window_state"],
        )
        self.assertEqual(transport.calls[2][1]["element_token"], "s00000001:9")
        self.assertNotIn("element_index", transport.calls[2][1])
        self.assertEqual(result.before.state_id, "state-2")
        self.assertEqual(result.after.state_id, "state-3")

    def test_pointer_dispatch_uses_visible_frame_center(self) -> None:
        snapshots = [
            raw_snapshot([element(4, "Retry", x=20, y=30, w=120, h=40)]),
            raw_snapshot([element(5, "Retry", x=20, y=30, w=120, h=40)]),
            raw_snapshot([
                        element(6, "Retry", x=20, y=30, w=120, h=40, selected=True)
                    ]),
        ]
        transport = FakeTransport(snapshots)
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="test",
            settle_delays=(),
            # These fixtures state frames in window coordinates already, so the
            # window sits at the origin; a pixel click needs it either way.
            window_origin=WindowGeometry(0, 0, 800, 600),
        )
        action = ActionEvidence.activate("Retry", dispatch="px")

        executor.execute_evidence(action)

        click_payload = next(payload for tool, payload in transport.calls if tool == "click")
        self.assertEqual((click_payload["x"], click_payload["y"]), (80.0, 50.0))

    def test_unchanged_semantic_activation_is_classified_as_missing_handler(self) -> None:
        unchanged = raw_snapshot([element(4, "Retry")])
        transport = FakeTransport([unchanged, unchanged])
        executor = CuaExecutor(
            transport, pid=44, window_id=77, session="test", settle_delays=()
        )

        result = executor.execute_evidence(ActionEvidence.activate("Retry"))

        self.assertIn("suspected-no-handler", {finding.code for finding in result.findings})

    def test_pointer_noop_is_probed_semantically_to_distinguish_occlusion(self) -> None:
        before = raw_snapshot([element(4, "Retry", x=20, y=30, w=120, h=40)])
        after_pointer = raw_snapshot([element(5, "Retry", x=20, y=30, w=120, h=40)])
        after_ax = raw_snapshot([
                    element(6, "Retry", x=20, y=30, w=120, h=40, selected=True)
                ])
        transport = FakeTransport([before, after_pointer, after_ax])
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="test",
            settle_delays=(),
            window_origin=WindowGeometry(0, 0, 800, 600),
        )

        result = executor.execute_evidence(
            ActionEvidence.activate("Retry", dispatch="px")
        )

        self.assertEqual(
            [tool for tool, _payload in transport.calls],
            ["get_window_state", "click", "get_window_state", "click", "get_window_state"],
        )
        self.assertIn("suspected-occlusion", {finding.code for finding in result.findings})

    def test_planned_settle_delay_is_not_reported_as_a_main_loop_stall(self) -> None:
        unchanged = raw_snapshot([element(4, "Retry")])
        transport = FakeTransport([unchanged, unchanged, unchanged])
        executor = CuaExecutor(
            transport, pid=44, window_id=77, session="test", settle_delays=(0.26,)
        )

        result = executor.execute_evidence(
            ActionEvidence.activate("Retry", expect_effect="idempotent")
        )

        codes = {finding.code for finding in result.findings}
        self.assertNotIn("main-loop-stall", codes)
        self.assertNotIn("missing-waiting-feedback", codes)
        self.assertLess(result.evidence.elapsed_ms, 250)


class ReplayAndReportTests(unittest.TestCase):
    def test_confirmation_requires_the_same_finding_in_two_fresh_profiles(self) -> None:
        finding = {
            "code": "suspected-no-handler",
            "severity": "error",
            "evidence": {"target": "Retry"},
        }

        self.assertEqual(confirm_findings([[finding], []]), [])
        confirmed = confirm_findings([[finding], [finding]])
        self.assertEqual([item["code"] for item in confirmed], ["suspected-no-handler"])
        self.assertEqual(confirmed[0]["confirmations"], 2)

    def test_delta_minimizer_keeps_only_actions_required_to_reproduce(self) -> None:
        actions = [
            {"kind": "activate", "target": "Music"},
            {"kind": "scroll", "direction": "down"},
            {"kind": "trigger-layout-bug"},
            {"kind": "scroll", "direction": "up"},
        ]

        minimized = minimize_actions(
            actions,
            reproduces=lambda candidate: any(
                action["kind"] == "trigger-layout-bug" for action in candidate
            ),
        )

        self.assertEqual(minimized, [{"kind": "trigger-layout-bug"}])

    def test_report_writes_json_markdown_and_jsonl_without_real_paths(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory)
            report = RunReport(
                output,
                mission_id="first-time-exploration",
                profile="mixed-128",
                seed=42,
                commit="abc123",
            )
            report.add_step(
                action={"kind": "activate", "target": {"label": "Retry"}},
                before_state="state-1",
                after_state="state-2",
                findings=[
                    {
                        "code": "suspected-no-handler",
                        "severity": "error",
                        "confidence": 0.9,
                        "summary": "Retry did not react.",
                        "evidence": {"target": "Retry"},
                        "blocks_gate": True,
                    }
                ],
            )
            report.write()

            summary = json.loads((output / "summary.json").read_text())
            markdown = (output / "report.md").read_text()
            trajectory = (output / "trajectory.jsonl").read_text()

        self.assertEqual(summary["mission_id"], "first-time-exploration")
        self.assertEqual(summary["finding_counts"]["error"], 1)
        self.assertIn("suspected-no-handler", markdown)
        self.assertIn('"before_state":"state-1"', trajectory)
        self.assertNotIn(str(pathlib.Path.home()), json.dumps(summary) + markdown + trajectory)


if __name__ == "__main__":
    unittest.main()
