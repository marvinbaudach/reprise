#!/usr/bin/env python3
"""Drive the explorer from a recorded, real driver snapshot.

Hand-written fixtures disagreed with the driver three times running - roles,
element container, flags - and every time the suite stayed green while the real
run did nothing. These tests start from `fixtures/hover-sweep-observe.json`,
which is verbatim cua-driver output, and they go through the public `propose`
entry point so no warm-up gate can be bypassed the way a private call does.
"""

from __future__ import annotations

import json
import pathlib
import sys
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
FIXTURES = pathlib.Path(__file__).resolve().parent / "fixtures"
FIXTURE = FIXTURES / "hover-sweep-observe.json"
TOAST_FIXTURE = FIXTURES / "layout-shift-toast.json"
sys.path.insert(0, str(EXPLORE_ROOT))

from driver import CuaExecutor  # noqa: E402
from explorer import DeterministicExplorer  # noqa: E402
from protocol import ActionGateway, ContractError, load_mission  # noqa: E402
from oracles import ActionEvidence, OracleEngine, normalize_snapshot  # noqa: E402
from ui_vocabulary import canonical_role, hover_strictness  # noqa: E402


def recorded_raw() -> dict:
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


class RecordedTransport:
    """Answers get_window_state with the recorded snapshot, unchanged."""

    def __init__(self) -> None:
        self.raw = recorded_raw()

    def call(self, tool, payload):
        if tool == "get_window_state":
            return self.raw
        return {"effect": "confirmed", "verified": True}

    def resize_window(self, *args):
        return {}

    def set_connectivity(self, state):
        return {}

    def wmctrl_geometry(self, window_id):
        return None


def recorded_observation() -> dict:
    """The observation the explorer really sees, built by the real driver code."""
    executor = CuaExecutor(
        RecordedTransport(), pid=1, window_id=2, session="recorded", settle_delays=()
    )
    return executor.observe()


class RecordedSnapshotShapeTests(unittest.TestCase):
    """Pin the shape of real driver output, so a fixture cannot drift from it."""

    def setUp(self) -> None:
        self.raw = recorded_raw()
        self.elements = self.raw["elements"]

    def test_the_driver_puts_elements_at_the_top_level(self) -> None:
        self.assertNotIn("structuredContent", self.raw)
        self.assertEqual(len(self.elements), 180)

    def test_the_recorded_roles_are_the_ones_the_driver_really_sends(self) -> None:
        roles = {str(item.get("role")) for item in self.elements}

        self.assertIn("button", roles)
        self.assertIn("toggle button", roles)
        self.assertIn("grid cell", roles)
        self.assertIn("tree grid", roles)

    def test_no_element_carries_an_actions_list(self) -> None:
        # So "actionable" can only come from the role vocabulary.
        self.assertFalse(any("actions" in item for item in self.elements))

    def test_the_recorded_snapshot_has_hover_targets_at_all(self) -> None:
        eligible = [
            item
            for item in self.elements
            if hover_strictness(canonical_role(str(item.get("role", "")))) != "skip"
            and item.get("label")
            and item.get("enabled") is not False
        ]

        self.assertGreater(len(eligible), 20)


class RecordedObservationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.observation = recorded_observation()

    def test_the_observation_marks_buttons_as_actionable(self) -> None:
        actionable = [
            item for item in self.observation["elements"] if item.get("actionable")
        ]

        self.assertGreater(len(actionable), 20)

    def test_the_observation_keeps_the_measured_geometry_flags(self) -> None:
        untrusted = [
            item
            for item in self.observation["elements"]
            if item.get("geometry_trusted") is False
        ]

        # Two of the four carry no label and the observation drops those.
        self.assertEqual(len(untrusted), 2)


class RecordedHoverSweepTests(unittest.TestCase):
    """The mission's whole purpose: from this snapshot, hover something."""

    def setUp(self) -> None:
        self.mission = load_mission(
            EXPLORE_ROOT / "missions" / "hover-affordance-sweep.json"
        )
        self.observation = recorded_observation()

    def _drive(self, limit=None):
        """Same loop the runner runs, gateway included.

        Leaving the gateway out is what let three green suites hide a run that
        died on its very first workload action.
        """
        explorer = DeterministicExplorer(self.mission, 1)
        gateway = ActionGateway(self.mission)
        actions = []
        for _index in range(limit or self.mission.budgets.actions):
            action = explorer.propose(self.observation)
            self.assertIsNotNone(action)
            try:
                accepted = gateway.accept(action, self.observation)
                # The runner confirms a checkpoint once its audit passed; the
                # audit itself is covered by the workload-evidence tests.
                if action["kind"] == "complete-workload":
                    gateway.confirm_workload(int(action["workload_index"]))
            except ContractError as error:
                self.fail(
                    f"the gateway rejected proposal {len(actions) + 1} "
                    f"({action.get('kind')} "
                    f"{action.get('target', {}).get('label', '')!r}): {error}"
                )
            actions.append(action)
            if action["kind"] == "finish":
                break
        return explorer, actions

    def test_the_explorer_proposes_hover_actions_from_a_real_snapshot(self) -> None:
        _explorer, actions = self._drive()
        kinds = [action["kind"] for action in actions]

        self.assertIn("hover", kinds, f"no hover proposed; kinds were {set(kinds)}")

    def test_the_sweep_reaches_more_than_a_handful_of_targets(self) -> None:
        _explorer, actions = self._drive()
        hovers = [action for action in actions if action["kind"] == "hover"]

        self.assertGreaterEqual(len(hovers), 31)

    def test_the_sweep_records_what_it_covered(self) -> None:
        explorer, _actions = self._drive()

        self.assertTrue(explorer.hover_coverage)
        self.assertGreater(
            sum(int(item["candidates"]) for item in explorer.hover_coverage), 0
        )

    def test_the_run_uses_its_budget_instead_of_stopping_after_eleven_steps(
        self,
    ) -> None:
        _explorer, actions = self._drive()

        self.assertGreater(len(actions), 11)

    def test_the_whole_budget_goes_to_the_one_reachable_section(self) -> None:
        # Seven of the eight sections have no accessible handle. Splitting the
        # budget eight ways capped the entry view at 25 of its 31 distinct
        # targets, and the player bar - sorted last, because it sits at the
        # bottom - was exactly what fell off the end.
        explorer, _actions = self._drive()
        entry = explorer.hover_coverage[0]

        self.assertEqual(entry["candidates"], 31)
        self.assertEqual(entry["hovered"], 31)
        self.assertEqual(entry["skipped_budget"], 0)

    def test_the_player_bar_controls_are_reached(self) -> None:
        _explorer, actions = self._drive()
        hovered = {
            action["target"]["label"]
            for action in actions
            if action["kind"] == "hover"
        }

        for label in ("Play (Space)", "Shuffle", "Volume"):
            self.assertIn(label, hovered)

    def test_the_sweep_stays_inside_the_missions_action_budget(self) -> None:
        _explorer, actions = self._drive()

        self.assertLessEqual(len(actions), self.mission.budgets.actions)

    def test_the_coverage_states_how_the_budget_was_divided(self) -> None:
        explorer, _actions = self._drive()
        entry = explorer.hover_coverage[0]

        self.assertEqual(entry["planned_sections"], 1)
        self.assertGreaterEqual(entry["limit_per_section"], 31)

    def test_every_hover_target_is_a_label_the_snapshot_offers(self) -> None:
        _explorer, actions = self._drive()
        labels = {
            str(item.get("label"))
            for item in self.observation["elements"]
            if item.get("label")
        }

        for action in actions:
            if action["kind"] == "hover":
                self.assertIn(action["target"]["label"], labels)


class RecordedLayoutShiftTests(unittest.TestCase):
    """The toast finding was the harness reading its own placeholder frame."""

    def setUp(self) -> None:
        raw = json.loads(TOAST_FIXTURE.read_text(encoding="utf-8"))
        self.before = normalize_snapshot(
            raw["before"], state_id="before", captured_ms=0
        )
        self.after = normalize_snapshot(raw["after"], state_id="after", captured_ms=500)
        self.engine = OracleEngine()

    def _codes(self, before=None, after=None):
        findings = self.engine.analyze(
            ActionEvidence(kind="wait", expect_effect="none"),
            before or self.before,
            after or self.after,
            settled=(after or self.after,),
        )
        return [item.code for item in findings]

    def test_the_recording_really_holds_the_placeholder_fallback(self) -> None:
        before = {item.label: item for item in self.before.elements}
        after = {item.label: item for item in self.after.elements}

        self.assertTrue(before["Dismiss"].geometry_trusted)
        self.assertFalse(after["Dismiss"].geometry_trusted)
        # Same rectangle in both real measurements; only the fallback differs.
        self.assertEqual(before["Dismiss"].frame.x, 966.0)
        self.assertEqual(after["Dismiss"].frame.x, 200.0)

    def test_an_unmeasured_position_is_not_a_layout_shift(self) -> None:
        self.assertNotIn("uninvited-layout-shift", self._codes())

    def test_a_real_shift_beside_the_toast_is_still_reported(self) -> None:
        # Move an element that keeps measured geometry on both sides.
        raw = json.loads(TOAST_FIXTURE.read_text(encoding="utf-8"))
        moved = []
        for item in raw["after"]["elements"]:
            item = dict(item)
            if item.get("label") == "Add filter":
                frame = dict(item["frame"])
                frame["y"] = float(frame["y"]) + 90
                item["frame"] = frame
            moved.append(item)
        after = normalize_snapshot(
            {**raw["after"], "elements": moved}, state_id="moved", captured_ms=500
        )

        codes = self._codes(after=after)

        self.assertIn("uninvited-layout-shift", codes)


class RecordedScrollDirectionTests(unittest.TestCase):
    """The same placeholder artefact could invert a scroll verdict."""

    def setUp(self) -> None:
        self.raw = json.loads(TOAST_FIXTURE.read_text(encoding="utf-8"))
        self.engine = OracleEngine()

    def _snapshot(self, elements, state_id):
        return normalize_snapshot(
            {**self.raw["before"], "elements": elements},
            state_id=state_id,
            captured_ms=0,
        )

    def _scrolled(self, *, poison):
        """Real rows answering an upward scroll; optionally one loses geometry.

        Scrolling up moves the content down, so the rows' y grows.
        """
        after = []
        poisoned = False
        for item in self.raw["before"]["elements"]:
            item = dict(item)
            if canonical_role(str(item.get("role", ""))) == "row":
                frame = dict(item["frame"])
                if poison and not poisoned:
                    poisoned = True
                    item["geometry_trusted"] = False
                    frame["x"], frame["y"] = 200, 50
                else:
                    frame["y"] = float(frame["y"]) + 40
                item["frame"] = frame
            after.append(item)
        return after

    def _codes(self, poison):
        findings = self.engine.analyze(
            ActionEvidence.scroll("up", amount=1, by="page"),
            self._snapshot(self.raw["before"]["elements"], "before"),
            self._snapshot(self._scrolled(poison=poison), "after"),
        )
        return [item.code for item in findings]

    def test_a_correct_scroll_stays_uncriticised(self) -> None:
        self.assertNotIn("wrong-scroll-direction", self._codes(poison=False))

    def test_one_unmeasured_row_does_not_invert_the_verdict(self) -> None:
        self.assertNotIn("wrong-scroll-direction", self._codes(poison=True))


if __name__ == "__main__":
    unittest.main(verbosity=1)
