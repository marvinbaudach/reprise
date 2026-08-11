#!/usr/bin/env python3
"""Display-free contract and end-to-end tests for the bundled reasoning agent."""

from __future__ import annotations

import copy
import json
import os
import pathlib
import random
import subprocess
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
TEST_ROOT = REPO_ROOT / "scripts" / "tests"
sys.path.insert(0, str(EXPLORE_ROOT))
sys.path.insert(0, str(TEST_ROOT))

from agents.budget import BudgetTooSmall, mandatory_step_count, plan_budget  # noqa: E402
from agents.agent_core import AgentSession  # noqa: E402
from agents.assertions import assertion_codes, batch_selection_count  # noqa: E402
from agents.plans import _activate, _search_type  # noqa: E402
from agents.steps import step_is_satisfied, step_to_action  # noqa: E402
from agent_adapter import ExternalAgent, MAX_RESPONSE_BYTES  # noqa: E402
from agents.plans import PLANNERS, build_phases  # noqa: E402
from cua_explore_fake_world import FakeWorld, drive  # noqa: E402
from driver import CuaExecutor  # noqa: E402
from explorer import DeterministicExplorer  # noqa: E402
from oracles import normalize_snapshot  # noqa: E402
from protocol import ActionGateway, load_mission  # noqa: E402
from runner import retain_agent_notes  # noqa: E402
from workload_audit import audit_action_workload  # noqa: E402


class BudgetTests(unittest.TestCase):
    def test_budget_shortfall_finishes_immediately_with_a_contract_reason(self) -> None:
        mission = self._mission("section-search-isolation")
        mission["budgets"]["actions"] = 5
        session = AgentSession(seed=1)

        action = session.next_action(mission, self._observation(), [])

        self.assertEqual(action["kind"], "finish")
        self.assertTrue(action["reason"].startswith("agent-contract-mismatch: budget"))

    def test_last_action_is_always_finish_under_a_truncated_budget(self) -> None:
        mission = self._mission("section-search-isolation")
        mandatory = sum(mandatory_step_count(item) for item in mission["workloads"])
        for total in range(mandatory + 2, mission["budgets"]["actions"] + 1):
            with self.subTest(total=total):
                candidate = copy.deepcopy(mission)
                candidate["budgets"]["actions"] = total
                loaded = load_mission(
                    EXPLORE_ROOT / "missions" / "section-search-isolation.json"
                )
                world = FakeWorld(profile=loaded.profile, tokens=loaded.fixture_tokens)
                session = AgentSession(seed=1, probe_ratio=0)
                observation = world.observation()
                history = []
                actions = []
                for _index in range(total):
                    action = session.next_action(candidate, observation, history)
                    actions.append(action)
                    if action["kind"] == "finish":
                        break
                    if action["kind"] != "complete-workload":
                        world.apply(action)
                        observation = world.observation()
                    history.append(
                        {
                            "action": action,
                            "finding_codes": world.finding_codes(action),
                            "after_state": observation["state_id"],
                        }
                    )
                self.assertEqual(actions[-1]["kind"], "finish")
                self.assertLessEqual(len(actions), total)

    AGENT_MISSIONS = (
        "first-time-exploration",
        "large-library-stress",
        "offline-recovery",
        "pointer-layout-reachability",
        "section-search-isolation",
    )

    def test_mandatory_steps_match_the_plan_the_agent_actually_executes(self) -> None:
        for name in self.AGENT_MISSIONS:
            mission = self._mission(name)
            for seed in (1, 7, 29):
                with self.subTest(mission=name, seed=seed):
                    phases = build_phases(mission, seed)

                    plan = plan_budget(mission, seed)

                    self.assertEqual(
                        plan.mandatory_per_workload,
                        tuple(len(phase.steps) for phase in phases),
                    )

    def test_each_workload_kind_reports_its_own_plan_length(self) -> None:
        seen = set()
        for name in self.AGENT_MISSIONS:
            mission = self._mission(name)
            for index, workload in enumerate(mission["workloads"]):
                seen.add(workload["kind"])
                with self.subTest(mission=name, kind=workload["kind"]):
                    expected = len(
                        PLANNERS[workload["kind"]](workload, index, random.Random(5)).steps
                    )

                    self.assertEqual(
                        mandatory_step_count(workload, index, seed=5), expected
                    )
        self.assertEqual(seen, set(PLANNERS))

    def test_a_budget_below_the_real_plan_is_refused_up_front(self) -> None:
        for name in self.AGENT_MISSIONS:
            mission = self._mission(name)
            phases = build_phases(mission, 1)
            minimum = sum(len(phase.steps) for phase in phases) + len(phases) + 1
            for total in (minimum - 1, minimum // 2):
                with self.subTest(mission=name, total=total):
                    candidate = copy.deepcopy(mission)
                    candidate["budgets"]["actions"] = total
                    with self.assertRaises(BudgetTooSmall):
                        plan_budget(candidate, 1)

    def test_the_supplied_missions_still_fit_their_declared_budgets(self) -> None:
        for name in self.AGENT_MISSIONS:
            for seed in (1, 7, 29):
                with self.subTest(mission=name, seed=seed):
                    plan_budget(self._mission(name), seed)

    def _mission(self, name):
        mission = load_mission(EXPLORE_ROOT / "missions" / f"{name}.json")
        return {
            "schema_version": 1,
            "id": mission.mission_id,
            "budgets": {
                "actions": mission.budgets.actions,
                "seconds": mission.budgets.seconds,
                "restarts": mission.budgets.restarts,
            },
            "capabilities": sorted(mission.capabilities),
            "fixture_tokens": sorted(mission.fixture_tokens),
            "workloads": list(mission.workloads),
            "forbidden": list(mission.forbidden),
        }

    def _observation(self):
        return {
            "schema_version": 1,
            "state_id": "state-1",
            "actionable_labels": [],
            "elements": [],
        }


class SelectionCountAssertionTests(unittest.TestCase):
    SELECT_ALL = {"kind": "hotkey", "keys": ["ctrl", "a"]}

    def _observation(self, *labels):
        return {
            "schema_version": 1,
            "state_id": "state-9",
            "elements": [
                {"label": label, "role": "status"} for label in labels
            ],
        }

    def _codes(self, observation, selection_count):
        return {
            code
            for code, _summary, _evidence in assertion_codes(
                self.SELECT_ALL, observation, selection_count=selection_count
            )
        }

    def test_the_expected_count_comes_from_the_mission_not_from_the_source(self) -> None:
        observation = self._observation("300 selected")

        self.assertNotIn(
            "agent-missing-selection-count", self._codes(observation, 300)
        )
        self.assertIn("agent-missing-selection-count", self._codes(observation, 512))

    def test_the_stress_mission_still_expects_its_own_512(self) -> None:
        mission = load_mission(EXPLORE_ROOT / "missions" / "large-library-stress.json")

        self.assertEqual(
            batch_selection_count({"workloads": list(mission.workloads)}), 512
        )

    def test_a_row_title_is_not_a_visible_selection_count(self) -> None:
        observation = self._observation("Track 005128", "Track 105129")

        self.assertIn("agent-missing-selection-count", self._codes(observation, 512))

    def test_the_evidence_names_the_expected_count(self) -> None:
        _code, _summary, evidence = assertion_codes(
            self.SELECT_ALL, self._observation("Music"), selection_count=64
        )[0]

        self.assertEqual(evidence["selection_count"], 64)

    def test_a_mission_without_a_batch_workload_asserts_nothing(self) -> None:
        mission = load_mission(EXPLORE_ROOT / "missions" / "first-time-exploration.json")

        self.assertIsNone(batch_selection_count({"workloads": list(mission.workloads)}))
        self.assertEqual(self._codes(self._observation("Music"), None), set())


class SectionSearchPreconditionTests(unittest.TestCase):
    """The night run invented four scope leaks from a value nobody had typed."""

    TYPED = {
        "kind": "type",
        "target": {"label": "Search all fields"},
        "fixture_token": "PODCAST_ONLY_NEEDLE",
    }

    def _observation(self, entry_value, rows=("Row A", "Row B")):
        return {
            "schema_version": 1,
            "state_id": "state-3",
            "elements": [
                {
                    "label": "Search all fields",
                    "role": "search box",
                    "value": entry_value,
                },
                *({"label": row, "role": "row"} for row in rows),
            ],
        }

    def _codes(self, observation, known_token_values):
        return {
            code
            for code, _summary, _evidence in assertion_codes(
                self.TYPED,
                observation,
                "search-Podcasts",
                section_changed=True,
                known_token_values=known_token_values,
            )
        }

    def test_the_previous_sources_value_does_not_satisfy_the_precondition(self) -> None:
        codes = self._codes(
            self._observation("Writable Batch 0042"),
            {"MUSIC_ONLY_NEEDLE": "Writable Batch 0042"},
        )

        self.assertIn("agent-precondition-unmet:search-Podcasts", codes)
        self.assertNotIn("agent-search-scope-leak", codes)

    def test_the_token_that_was_typed_still_produces_the_scope_leak(self) -> None:
        codes = self._codes(
            self._observation("Fixture Podcast Needle"),
            {
                "MUSIC_ONLY_NEEDLE": "Writable Batch 0042",
                "PODCAST_ONLY_NEEDLE": "Fixture Podcast Needle",
            },
        )

        self.assertIn("agent-search-scope-leak", codes)
        self.assertNotIn("agent-precondition-unmet:search-Podcasts", codes)

    def test_a_first_search_without_any_learned_value_is_believed(self) -> None:
        codes = self._codes(self._observation("Fixture Podcast Needle"), {})

        self.assertIn("agent-search-scope-leak", codes)

    def test_a_single_row_under_the_typed_value_asserts_nothing(self) -> None:
        codes = self._codes(
            self._observation(
                "Fixture Podcast Needle", rows=("Fixture Podcast Needle",)
            ),
            {"PODCAST_ONLY_NEEDLE": "Fixture Podcast Needle"},
        )

        self.assertEqual(codes, set())

    def test_the_evidence_names_the_token_and_what_stood_in_the_entry(self) -> None:
        _code, _summary, evidence = assertion_codes(
            self.TYPED,
            self._observation("Writable Batch 0042"),
            "search-Podcasts",
            section_changed=True,
            known_token_values={"MUSIC_ONLY_NEEDLE": "Writable Batch 0042"},
        )[0]

        self.assertEqual(evidence["fixture_token"], "PODCAST_ONLY_NEEDLE")
        self.assertEqual(evidence["entry_values"], ["Writable Batch 0042"])


class AgentAcceptanceTests(unittest.TestCase):
    def test_recorded_actions_choose_dispatch_and_entry_roles_stay_strict(self) -> None:
        closed = self._recorded_observation("postfix-2026-08-10-sidebar-open.json")
        opened = self._recorded_observation("postfix-2026-08-10-search-open.json")
        music, search = _activate("open-Music", "Music"), _activate("open-search", "Search all fields")

        self.assertEqual(step_to_action(music, closed)[0]["dispatch"], "px")
        self.assertEqual(step_to_action(search, closed)[0]["dispatch"], "ax")
        explorer_action = DeterministicExplorer(
            self._mission("first-time-exploration"), 11
        ).propose(closed)
        self.assertEqual((explorer_action["target"], explorer_action["dispatch"]), ({"label": "Music"}, "px"))
        opener, type_step = _search_type("search-Music", "MUSIC_ONLY_NEEDLE")
        self.assertFalse(step_is_satisfied(opener, closed))
        self.assertTrue(step_is_satisfied(opener, opened))
        self.assertIsNone(step_to_action(type_step, closed)[0])
        action = step_to_action(type_step, opened)[0]
        self.assertEqual(action["target"], {"label": "Search all fields"})
        ActionGateway(self._mission("section-search-isolation")).accept(action, opened)

    def test_an_unmeasured_route_is_noted_instead_of_silently_staying_semantic(self) -> None:
        observation = self._without_geometry(
            self._recorded_observation("postfix-2026-08-10-sidebar-open.json")
        )
        session = AgentSession(seed=11, probe_ratio=0)

        action = session.next_action(
            self._agent_mission(self._mission("section-search-isolation")),
            observation,
            [],
        )

        self.assertEqual(action["dispatch"], "ax")
        note = next(
            note
            for note in session.notes
            if note.code.startswith("agent-dispatch-geometry-unmeasured:")
        )
        self.assertEqual(note.evidence["target"], action["target"]["label"])
        self.assertEqual(note.evidence["actions"], [])
        self.assertEqual(note.evidence["dispatch"], "ax")

    def test_the_explorer_records_the_route_it_could_not_prove(self) -> None:
        measured = self._recorded_observation("postfix-2026-08-10-sidebar-open.json")
        explorer = DeterministicExplorer(self._mission("first-time-exploration"), 11)

        action = explorer.propose(self._without_geometry(measured))

        self.assertEqual(
            (action["target"], action["dispatch"]), ({"label": "Music"}, "ax")
        )
        self.assertEqual(
            explorer.dispatch_policy["reason"], "activation-geometry-unmeasurable"
        )
        self.assertEqual(
            [item["target"] for item in explorer.dispatch_policy["targets"]], ["Music"]
        )

    def test_a_measured_route_leaves_the_explorer_policy_empty(self) -> None:
        explorer = DeterministicExplorer(self._mission("first-time-exploration"), 11)

        explorer.propose(
            self._recorded_observation("postfix-2026-08-10-sidebar-open.json")
        )

        self.assertIsNone(explorer.dispatch_policy)

    def test_semantic_retry_switches_the_run_to_pointer_dispatch_after_three(self) -> None:
        mission = self._mission("section-search-isolation")
        closed = self._closed_sources_observation()
        opened = self._recorded_observation("postfix-2026-08-10-search-open.json")
        session = AgentSession(seed=11, probe_ratio=0)
        gateway, history, actions = ActionGateway(mission), [], []
        observation = self._state(closed, 0, "closed")
        for index in range(70):
            action = session.next_action(self._agent_mission(mission), observation, history)
            gateway.accept(action, observation)
            actions.append(action)
            if action["kind"] in {"finish", "complete-workload"}:
                break
            unchanged = (
                action["kind"] == "activate"
                and action.get("target", {}).get("label") == "Search all fields"
                and action.get("dispatch") == "ax"
            )
            is_open = action["kind"] == "type" or (
                action["kind"] == "activate"
                and action.get("target", {}).get("label") == "Search all fields"
                and action.get("dispatch") == "px"
            )
            next_template = opened if is_open else closed
            signature = observation["state_signature"] if unchanged else f"changed-{index}"
            next_observation = self._state(next_template, index + 1, signature)
            history.append({"action": action, "finding_codes": [], "after_state": next_observation["state_id"]})
            observation = next_observation
            search_activations = [
                item for item in actions
                if item["kind"] == "activate"
                and item.get("target", {}).get("label") == "Search all fields"
            ]
            if len(search_activations) >= 7:
                break

        search_routes = [item["dispatch"] for item in search_activations]
        self.assertEqual(
            search_routes[:6], ["ax", "px", "ax", "px", "ax", "px"], actions
        )
        self.assertEqual(search_routes[6], "px")
        self.assertEqual(session.dispatch_policy["effective"], "px")
        self.assertEqual([note.code for note in session.notes].count("semantic-route-unavailable"), 1)
        self.assertEqual([note.code for note in session.notes].count("semantic-activation-ineffective"), 3)

    def test_a_working_second_semantic_activation_keeps_semantic_dispatch(self) -> None:
        mission, closed, opened = (
            self._mission("section-search-isolation"),
            self._closed_sources_observation(),
            self._recorded_observation("postfix-2026-08-10-search-open.json"),
        )
        session, history = AgentSession(seed=11, probe_ratio=0), []
        observation, semantic_attempt = self._state(closed, 0, "closed"), 0
        for index in range(30):
            action = session.next_action(self._agent_mission(mission), observation, history)
            is_search_ax = action["kind"] == "activate" and action.get("target", {}).get("label") == "Search all fields" and action.get("dispatch") == "ax"
            semantic_attempt += int(is_search_ax)
            unchanged = is_search_ax and semantic_attempt == 1
            is_open = action["kind"] == "type" or (action["kind"] == "activate" and action.get("target", {}).get("label") == "Search all fields" and (action.get("dispatch") == "px" or semantic_attempt == 2))
            next_observation = self._state(opened if is_open else closed, index + 1, observation["state_signature"] if unchanged else f"changed-{index}")
            history.append({"action": action, "finding_codes": [], "after_state": next_observation["state_id"]})
            observation = next_observation
            if semantic_attempt == 2:
                session.next_action(self._agent_mission(mission), observation, history)
                break
        self.assertEqual(session.dispatch_policy["effective"], "ax")
        self.assertNotIn("semantic-route-unavailable", {note.code for note in session.notes})

    def test_two_ineffective_routes_do_not_schedule_a_third_attempt(self) -> None:
        observation = self._closed_sources_observation()
        action = {
            "kind": "activate",
            "target": {"label": "Search all fields"},
            "dispatch": "ax",
            "expect_effect": "required",
        }
        session = AgentSession(seed=11, probe_ratio=0)
        session._track_activation_result(observation, observation, action, "open-search")
        retry, context = session._pending_activation_retry
        session._pending_activation_retry = None
        session._activation_retry_inflight = context

        session._track_activation_result(observation, observation, retry, "open-search")

        self.assertIsNone(session._pending_activation_retry)
        self.assertEqual(
            [note.code for note in session.notes],
            ["agent-missing-affordance:open-search"],
        )

    def test_missing_sidebar_is_named_with_the_measured_width(self) -> None:
        mission = self._mission("section-search-isolation")
        observation = self._recorded_observation("night-2026-08-10-music-collapsed.json")
        session, history = AgentSession(seed=11, probe_ratio=0), []
        for _index in range(8):
            action = session.next_action(self._agent_mission(mission), observation, history)
            history.append({"action": action, "finding_codes": [], "after_state": observation["state_id"]})
            if any(note.code == "agent-sidebar-unavailable" for note in session.notes):
                break
        note = next(note for note in session.notes if note.code == "agent-sidebar-unavailable")
        self.assertEqual(note.evidence["window_width"], observation["window"]["width"])

    def test_section_search_mission_satisfies_the_real_audit(self) -> None:
        session, actions, _traces, mission = self._run("section-search-isolation")

        self.assertEqual(actions[-1]["kind"], "finish")
        self.assertTrue(all(audit["complete"] for audit in session.workload_audits.values()))
        self.assertEqual(set(session.workload_audits), {0, 1})

    def test_offline_mission_satisfies_the_real_audit(self) -> None:
        session, actions, traces, mission = self._run("offline-recovery")

        audit = session.workload_audits[0]
        self.assertTrue(audit["complete"])
        offline_at = next(
            index
            for index, trace in enumerate(traces)
            if trace.action.get("kind") == "set-connectivity"
            and trace.action.get("connectivity") == "offline"
        )
        self.assertEqual(traces[offline_at - 1].action["kind"], "activate")
        self.assertIn("refresh", traces[offline_at - 1].action["target_label"].casefold())
        self.assertEqual(actions[-1]["kind"], "finish")

    def test_stress_mission_satisfies_all_four_audits(self) -> None:
        session, actions, _traces, mission = self._run("large-library-stress")

        self.assertEqual(set(session.workload_audits), {0, 1, 2, 3})
        self.assertTrue(all(audit["complete"] for audit in session.workload_audits.values()))
        checkpoint = next(
            index
            for index, action in enumerate(actions)
            if action.get("kind") == "complete-workload"
            and action.get("workload_index") == 0
        )
        first_long_scroll = next(
            index
            for index, action in enumerate(actions)
            if action.get("kind") == "scroll" and action.get("amount", 0) > 1
        )
        self.assertLess(checkpoint, first_long_scroll)

    def test_mission_never_exceeds_its_action_budget(self) -> None:
        for name in self._agent_missions():
            with self.subTest(mission=name):
                _session, actions, _traces, mission = self._run(name)
                self.assertLessEqual(len(actions), mission.budgets.actions)

    def test_same_seed_produces_the_same_action_sequence(self) -> None:
        first = self._run("section-search-isolation", seed=29)[1]
        second = self._run("section-search-isolation", seed=29)[1]

        self.assertEqual(first, second)

    def test_different_seeds_keep_locked_order_but_vary_probes(self) -> None:
        mission = self._mission("section-search-isolation")
        first = build_phases(self._agent_mission(mission), 11)
        second = build_phases(self._agent_mission(mission), 29)

        first_locked = [
            (phase.name, [step.name for step in phase.steps])
            for phase in first
            if phase.order_locked
        ]
        second_locked = [
            (phase.name, [step.name for step in phase.steps])
            for phase in second
            if phase.order_locked
        ]
        self.assertEqual(first_locked, second_locked)
        self.assertNotEqual(
            self._run("section-search-isolation", seed=11)[1][0],
            self._run("section-search-isolation", seed=29)[1][0],
        )

    def test_agent_never_targets_a_label_missing_from_actionable_labels(self) -> None:
        for name in self._agent_missions():
            session, actions, _traces, _mission = self._run(name)
            # Full execution through drive already exercises the real gateway on each target.
            self.assertTrue(actions)
            self.assertFalse(any(note.code.startswith("agent-self-gate-blocked") for note in session.notes))

    def test_agent_never_targets_destructive_or_external_labels(self) -> None:
        _session, actions, _traces, _mission = self._run("large-library-stress")

        labels = [str(action.get("target", {}).get("label", "")).casefold() for action in actions]
        self.assertFalse(any("delete" in label or "open in browser" in label for label in labels))

    def test_agent_only_uses_declared_capabilities_and_fixture_tokens(self) -> None:
        for name in self._agent_missions():
            _session, actions, _traces, mission = self._run(name)
            for action in actions:
                self.assertIn(action["kind"], mission.capabilities)
                if action["kind"] == "type":
                    self.assertIn(action["fixture_token"], mission.fixture_tokens)

    def test_agent_hotkeys_always_carry_a_modifier_and_allowed_keys(self) -> None:
        _session, actions, _traces, _mission = self._run("large-library-stress")

        for action in actions:
            if action["kind"] == "hotkey":
                self.assertIn(action["keys"][0], {"ctrl", "shift", "alt"})
                self.assertIn(len(action["keys"]), {2, 3})

    def test_every_emitted_action_is_accepted_by_the_real_action_gateway(self) -> None:
        # drive() sends every emitted object through ActionGateway before applying it.
        for name in self._agent_missions():
            with self.subTest(mission=name):
                self._run(name)

    def test_missing_section_is_reported_and_never_faked(self) -> None:
        session, actions, _traces, mission = self._run(
            "section-search-isolation", quirks={"no-youtube-section"}
        )

        self.assertEqual(actions[-1]["kind"], "complete-workload")
        self.assertIn("agent-sidebar-unavailable", {note.code for note in session.notes})
        self.assertIn("agent-precondition-unmet:search-YouTube", {note.code for note in session.notes})
        self.assertNotIn("agent-search-scope-leak", {note.code for note in session.notes})
        self.assertFalse(any(action.get("target", {}).get("label") == "YouTube" for action in actions))
        audit = audit_action_workload(0, mission.workloads[0], session.traces, mission.fixture_tokens)
        self.assertFalse(audit["route_results"]["YouTube"])

    def test_scope_leak_is_recorded_as_a_note(self) -> None:
        session, _actions, _traces, _mission = self._run(
            "section-search-isolation", quirks={"search-leaks-music"}
        )

        self.assertIn("agent-search-scope-leak", {note.code for note in session.notes})

    def test_missing_selection_count_is_recorded_as_a_note(self) -> None:
        session, _actions, _traces, _mission = self._run(
            "large-library-stress", quirks={"no-selection-count"}
        )

        self.assertIn("agent-missing-selection-count", {note.code for note in session.notes})

    def test_filter_drop_and_sort_stall_are_named_notes(self) -> None:
        sort_session, _actions, _traces, _mission = self._run(
            "large-library-stress", quirks={"sort-does-not-reorder"}
        )
        filter_session, _actions, _traces, _mission = self._run(
            "large-library-stress", quirks={"chip-dropped-by-search"}
        )

        self.assertIn("agent-sort-without-reorder", {note.code for note in sort_session.notes})
        self.assertIn(
            "agent-filter-dropped-by-search", {note.code for note in filter_session.notes}
        )

    def test_offline_duplicates_and_stuck_status_are_named_notes(self) -> None:
        duplicate, _actions, _traces, _mission = self._run(
            "offline-recovery", quirks={"duplicate-cached-row"}
        )
        stuck, _actions, _traces, _mission = self._run(
            "offline-recovery", quirks={"offline-status-stuck"}
        )

        self.assertIn("agent-duplicate-cached-row", {note.code for note in duplicate.notes})
        self.assertIn("agent-offline-status-stuck", {note.code for note in stuck.notes})

    def test_agent_recovers_via_alternate_route_before_reporting_missing(self) -> None:
        session, actions, _traces, _mission = self._run(
            "large-library-stress", quirks={"context-menu-missing"}
        )

        self.assertTrue(any(action.get("kind") == "press" and action.get("key") == "f10" for action in actions))
        self.assertFalse(any(note.code.startswith("agent-missing-affordance:edit-tags") for note in session.notes))

    def test_agent_answers_every_request_even_after_an_internal_error(self) -> None:
        mission = self._mission("section-search-isolation")
        world = FakeWorld(profile=mission.profile, tokens=mission.fixture_tokens)
        session = AgentSession(seed=1)
        session._initialize = lambda _mission: (_ for _ in ()).throw(RuntimeError("boom"))

        action = session.next_action(self._agent_mission(mission), world.observation(), [])

        self.assertEqual(action["kind"], "finish")
        self.assertTrue(action["reason"].startswith("agent-internal-error: RuntimeError"))

    def test_probes_stop_after_the_soft_time_deadline(self) -> None:
        clock_values = iter((0.0, 900.0, 900.0, 900.0))
        mission = self._mission("section-search-isolation")
        world = FakeWorld(profile=mission.profile, tokens=mission.fixture_tokens)
        session = AgentSession(seed=1, clock=lambda: next(clock_values))

        action = session.next_action(self._agent_mission(mission), world.observation(), [])

        self.assertNotEqual(action["kind"], "wait")

    def test_agent_notes_a_role_mismatch_instead_of_stalling(self) -> None:
        session, actions, _traces, _mission = self._run(
            "large-library-stress", quirks={"rows-report-table-row-role"}
        )

        self.assertTrue(any("role-vocabulary-mismatch" in note.code for note in session.notes))
        self.assertTrue(actions)

    def test_agent_works_without_entry_values(self) -> None:
        session, actions, _traces, _mission = self._run(
            "section-search-isolation", quirks={"entry-has-no-value"}
        )

        self.assertIn("agent-token-value-unknown", {note.code for note in session.notes})
        self.assertTrue(actions)

    def test_mission_fixture_tokens_are_referenced_by_at_least_one_plan(self) -> None:
        for name in self._agent_missions():
            mission = self._mission(name)
            phases = build_phases(self._agent_mission(mission), 1)
            referenced = {
                str(step.fields["fixture_token"])
                for phase in phases
                for step in phase.steps
                if "fixture_token" in step.fields
            }
            referenced.update(
                step.token_hint
                for phase in phases
                for step in phase.steps
                if step.token_hint
            )
            required = set(mission.fixture_tokens) - {"NO_MATCH", "SEARCH_NEEDLE"}
            self.assertTrue(required.issubset(referenced), (name, required - referenced))

    def test_hover_sample_is_skipped_when_the_mission_lacks_the_capability(self) -> None:
        mission = self._mission("section-search-isolation")
        agent_mission = self._agent_mission(mission)
        agent_mission["capabilities"].remove("hover")
        world = FakeWorld(profile=mission.profile, tokens=mission.fixture_tokens)
        session = AgentSession(seed=1, probe_ratio=0)

        action = session.next_action(agent_mission, world.observation(), [])

        self.assertNotEqual(action["kind"], "hover")

    def _run(self, name, *, seed=11, quirks=frozenset()):
        mission = self._mission(name)
        world = FakeWorld(
            profile=mission.profile,
            tokens=mission.fixture_tokens,
            quirks=frozenset(quirks),
        )
        session = AgentSession(seed=seed, probe_ratio=1.0)
        actions, traces = drive(
            session, world, mission, max_actions=mission.budgets.actions
        )
        return session, actions, traces, mission

    def _mission(self, name):
        return load_mission(EXPLORE_ROOT / "missions" / f"{name}.json")

    def _agent_mission(self, mission):
        return {
            "schema_version": 1,
            "id": mission.mission_id,
            "budgets": {
                "actions": mission.budgets.actions,
                "seconds": mission.budgets.seconds,
                "restarts": mission.budgets.restarts,
            },
            "capabilities": sorted(mission.capabilities),
            "fixture_tokens": sorted(mission.fixture_tokens),
            "workloads": list(mission.workloads),
            "forbidden": list(mission.forbidden),
        }

    def _agent_missions(self):
        return (
            "section-search-isolation",
            "offline-recovery",
            "large-library-stress",
        )

    def _recorded_observation(self, name):
        raw = json.loads((TEST_ROOT / "fixtures" / name).read_text(encoding="utf-8"))
        state = normalize_snapshot(raw, state_id="recorded", captured_ms=0)
        observation = CuaExecutor._observation(object.__new__(CuaExecutor), state)
        projected = [item for item in state.elements if item.label]
        for item, element in zip(observation["elements"], projected, strict=True):
            item["actions"] = list(element.actions)
        return observation

    def _closed_sources_observation(self):
        closed = self._recorded_observation("postfix-2026-08-10-sidebar-open.json")
        opened = self._recorded_observation("postfix-2026-08-10-search-open.json")
        toggle = next(item for item in closed["elements"] if item["label"] == "Search all fields")
        opened["elements"] = [
            item for item in opened["elements"] if item["label"] != "Search all fields"
        ] + [toggle]
        opened["actionable_labels"] = sorted({item["label"] for item in opened["elements"] if item["actionable"]})
        return opened

    @staticmethod
    def _without_geometry(observation):
        """The same recorded tree with every frame left unmeasured."""
        return {
            **observation,
            "elements": [
                {**item, "frame": {**item.get("frame", {}), "width": 0, "height": 0}}
                for item in observation["elements"]
            ],
        }

    @staticmethod
    def _state(template, index, signature):
        observation = copy.deepcopy(template)
        observation["state_id"] = f"state-{index}"
        observation["state_signature"] = signature
        return observation


class FakeWorldTests(unittest.TestCase):
    def test_fake_world_matches_the_observation_schema(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "section-search-isolation.json"
        )
        observation = FakeWorld(
            profile=mission.profile, tokens=mission.fixture_tokens
        ).observation()

        self.assertEqual(
            set(observation),
            {
                "schema_version",
                "state_id",
                "state_signature",
                "window",
                "degraded",
                "actionable_labels",
                "elements",
            },
        )
        self.assertEqual(
            set(observation["elements"][0]),
            {
                "key",
                "label",
                "role",
                "enabled",
                "visible",
                "focused",
                "selected",
                "value",
                "actionable",
                "frame",
            },
        )

    def test_fake_world_quirks_change_only_the_intended_predicate(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "section-search-isolation.json"
        )
        normal = FakeWorld(profile=mission.profile, tokens=mission.fixture_tokens)
        quirky = FakeWorld(
            profile=mission.profile,
            tokens=mission.fixture_tokens,
            quirks=frozenset({"no-youtube-section"}),
        )

        self.assertIn("YouTube", normal.observation()["actionable_labels"])
        self.assertNotIn("YouTube", quirky.observation()["actionable_labels"])
        self.assertEqual(normal.section, quirky.section)


class AgentTransportTests(unittest.TestCase):
    def test_agent_process_answers_one_json_object_per_line(self) -> None:
        mission, request = self._request()
        with tempfile.TemporaryDirectory() as directory:
            with ExternalAgent(
                [sys.executable, str(self._agent_path()), "--seed", "11"],
                private_home=pathlib.Path(directory),
            ) as agent:
                action = agent.propose(
                    request["mission"], request["observation"], []
                )

        self.assertIsInstance(action, dict)
        self.assertEqual(action["schema_version"], 1)

    def test_agent_writes_less_than_eight_kilobytes_to_stderr(self) -> None:
        _mission, request = self._request()
        with tempfile.TemporaryDirectory() as directory:
            completed = subprocess.run(
                [sys.executable, str(self._agent_path()), "--seed", "11"],
                input=json.dumps(request) + "\n",
                capture_output=True,
                text=True,
                check=False,
                timeout=5,
                env={"PATH": os.environ.get("PATH", ""), "HOME": directory},
            )

        self.assertEqual(completed.returncode, 0)
        self.assertLess(len(completed.stderr.encode("utf-8")), 8_192)

    def test_agent_response_stays_below_the_bounded_transport_size(self) -> None:
        _mission, request = self._request()
        with tempfile.TemporaryDirectory() as directory:
            completed = subprocess.run(
                [sys.executable, str(self._agent_path()), "--seed", "11"],
                input=json.dumps(request) + "\n",
                capture_output=True,
                text=True,
                check=True,
                timeout=5,
                env={"PATH": os.environ.get("PATH", ""), "HOME": directory},
            )

        self.assertLess(len(completed.stdout.splitlines()[0].encode("utf-8")), MAX_RESPONSE_BYTES)

    def test_runner_retains_agent_notes_as_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            agent_home = root / "profile" / "agent-home"
            agent_home.mkdir(parents=True)
            (agent_home / "agent-notes.jsonl").write_text(
                '{"code":"finding"}\n', encoding="utf-8"
            )

            retain_agent_notes(root / "profile", root / "evidence")

            self.assertEqual(
                (root / "evidence" / "agent" / "agent-notes.jsonl").read_text(
                    encoding="utf-8"
                ),
                '{"code":"finding"}\n',
            )

    def _request(self):
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "section-search-isolation.json"
        )
        world = FakeWorld(profile=mission.profile, tokens=mission.fixture_tokens)
        agent_mission = {
            "schema_version": 1,
            "id": mission.mission_id,
            "budgets": {
                "actions": mission.budgets.actions,
                "seconds": mission.budgets.seconds,
                "restarts": mission.budgets.restarts,
            },
            "capabilities": sorted(mission.capabilities),
            "fixture_tokens": sorted(mission.fixture_tokens),
            "workloads": list(mission.workloads),
            "forbidden": list(mission.forbidden),
        }
        return mission, {
            "schema_version": 1,
            "mission": agent_mission,
            "observation": world.observation(),
            "recent_history": [],
            "instruction": "Return one action",
        }

    def _agent_path(self):
        return EXPLORE_ROOT / "agents" / "reprise_ux_agent.py"

if __name__ == "__main__":
    unittest.main()
