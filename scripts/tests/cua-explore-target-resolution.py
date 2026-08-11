#!/usr/bin/env python3
"""Regression tests for measured action names and fresh target resolution."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


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
from driver import CliTransport, CuaExecutor, DriverError  # noqa: E402
from driver_faults import MAX_RETAINED_FAULT_LINES  # noqa: E402
from explorer import DeterministicExplorer  # noqa: E402
from oracles import ActionEvidence, OracleEngine, normalize_snapshot  # noqa: E402
from protocol import ActionGateway, load_mission  # noqa: E402
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


class RecordedTransport:
    def __init__(self, raw: dict) -> None:
        self.raw = raw
        self.calls: list[tuple[str, dict]] = []

    def call(self, tool: str, payload: dict) -> dict:
        self.calls.append((tool, payload))
        if tool == "get_window_state":
            return self.raw
        return {"effect": "confirmed", "verified": True}

    def resize_window(self, *args):
        return {}

    def set_connectivity(self, state):
        return {}

    def wmctrl_geometry(self, window_id):
        return None


def completed(stdout: str, *, returncode: int = 0, stderr: str = ""):
    return subprocess.CompletedProcess([], returncode, stdout, stderr)


class ScriptedCliTransport(CliTransport):
    def __init__(self, responses, *, evidence_dir: pathlib.Path) -> None:
        super().__init__(evidence_dir=evidence_dir)
        self.responses = list(responses)
        self.run_count = 0

    def _run(self, command):
        self.run_count += 1
        response = self.responses.pop(0)
        if isinstance(response, BaseException):
            raise response
        return response


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

    def test_absent_and_malformed_action_lists_classify_as_empty(self) -> None:
        # `.get("actions", ())` hands None straight through when the key exists
        # with a null value; iterating it raised TypeError and ended the run.
        for value in (None, 7, True, object()):
            with self.subTest(value=value):
                self.assertEqual(invocable_actions(value), ())
                self.assertEqual(unknown_action_names(value), ())

    def test_a_lone_string_counts_as_one_action_name(self) -> None:
        self.assertEqual(invocable_actions("click"), ("click",))
        self.assertEqual(invocable_actions("listitem.scroll-to"), ())

    def test_classification_reads_the_same_in_both_spellings(self) -> None:
        # normalize_snapshot lowercases the names, atspi_geometry hands them
        # through verbatim, and driver._target judges the verbatim ones.
        self.assertTrue(is_structural_action("ListItem.Scroll-To"))
        self.assertEqual(invocable_actions((" ListItem.scroll-to ",)), ())
        self.assertEqual(invocable_actions(("Click",)), ("click",))
        self.assertEqual(unknown_action_names(("Click",)), ())


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


class DeterministicTargetTests(unittest.TestCase):
    def setUp(self) -> None:
        self.raw = load_fixture(AMBIGUOUS_CELLS)
        self.transport = RecordedTransport(self.raw)
        self.executor = CuaExecutor(
            self.transport,
            pid=1,
            window_id=2,
            session="recorded",
            settle_delays=(),
        )

    def test_structural_cell_collision_uses_the_existing_role_fallback(self) -> None:
        chosen = self.executor._target(self.raw, "Fixture Album 34")

        self.assertEqual(chosen["element_index"], 118)
        self.assertEqual(self.executor._ambiguity_notes, {})

    def test_invocable_name_collision_chooses_reading_order_and_fires_once(self) -> None:
        chosen = self.executor._target(self.raw, "☆")

        self.assertEqual(chosen["element_index"], 35)
        first = self.executor.execute_evidence(
            ActionEvidence.activate("☆", expect_effect="idempotent")
        )
        second = self.executor.execute_evidence(
            ActionEvidence.activate("☆", expect_effect="idempotent")
        )
        first_findings = [
            item for item in first.findings if item.code == "ambiguous-accessible-name"
        ]
        second_findings = [
            item for item in second.findings if item.code == "ambiguous-accessible-name"
        ]

        self.assertEqual(len(first_findings), 1)
        self.assertEqual(second_findings, [])
        evidence = first_findings[0].evidence
        self.assertEqual(evidence["count"], 21)
        self.assertEqual(evidence["chosen"], chosen["frame"])
        self.assertLessEqual(len(evidence["alternatives"]), 8)

    def test_null_actions_do_not_end_the_run(self) -> None:
        raw = load_fixture(AMBIGUOUS_CELLS)
        for item in raw["elements"]:
            if item.get("label") == "☆":
                item["actions"] = None
        executor = CuaExecutor(
            RecordedTransport(raw), pid=1, window_id=2, session="null", settle_delays=()
        )

        chosen = executor._target(raw, "☆")

        self.assertEqual(chosen["label"], "☆")
        self.assertIs(executor.target_carries_action(raw, "☆"), False)

    def test_recorded_explorer_reaches_ten_actions_without_target_failure(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "first-time-exploration.json"
        )
        explorer = DeterministicExplorer(mission, seed=11)
        gateway = ActionGateway(mission)
        observation = self.executor.observe()

        for _step in range(10):
            proposal = explorer.propose(observation)
            accepted = gateway.accept(proposal, observation)
            result = self.executor.execute(accepted)
            observation = self.executor.observation_from_snapshot(result.after)

        clicks = [call for call in self.transport.calls if call[0] == "click"]
        self.assertEqual(len(clicks), 10)


class CliTransportRetryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.evidence_dir = pathlib.Path(self.temporary.name)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def fault_records(self) -> list[dict]:
        path = self.evidence_dir / "driver-faults.jsonl"
        return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]

    def test_read_recovers_from_non_json_text_and_retains_the_payload(self) -> None:
        transport = ScriptedCliTransport(
            [completed("not-json"), completed('{"elements": []}')],
            evidence_dir=self.evidence_dir,
        )

        response = transport.call("get_window_state", {})

        self.assertEqual(response, {"elements": []})
        self.assertEqual(transport.transport_faults, 1)
        self.assertEqual(self.fault_records()[0]["stdout_head"], "not-json")
        self.assertEqual(
            [finding.code for finding in transport.take_findings()],
            ["driver-transport-fault"],
        )
        self.assertEqual(transport.take_findings(), [])

    def test_three_non_json_reads_name_the_last_line_after_retaining_every_attempt(
        self,
    ) -> None:
        transport = ScriptedCliTransport(
            [completed("one"), completed("two"), completed("three")],
            evidence_dir=self.evidence_dir,
        )

        with self.assertRaisesRegex(DriverError, "returned non-JSON text: three"):
            transport.call("get_window_state", {})

        self.assertEqual(transport.run_count, 3)
        self.assertEqual(
            [record["stdout_head"] for record in self.fault_records()],
            ["one", "two", "three"],
        )

    def test_empty_window_state_exhausts_readiness_ladder_and_retains_every_attempt(
        self,
    ) -> None:
        transport = ScriptedCliTransport(
            [completed("")] * 4,
            evidence_dir=self.evidence_dir,
        )

        with mock.patch("driver_transport.time.sleep") as sleep:
            with self.assertRaisesRegex(DriverError, "returned an empty response"):
                transport.call("get_window_state", {})

        self.assertEqual(transport.run_count, 4)
        self.assertEqual(transport.transport_faults, 4)
        self.assertEqual(
            [record["attempt"] for record in self.fault_records()],
            [1, 2, 3, 4],
        )
        self.assertEqual(
            [record["stdout_head"] for record in self.fault_records()],
            ["", "", "", ""],
        )
        self.assertEqual(
            [call.args[0] for call in sleep.call_args_list],
            [1.0, 2.0, 3.0],
        )

    def test_input_action_is_never_retried(self) -> None:
        transport = ScriptedCliTransport(
            [completed("broken"), completed('{"effect": "confirmed"}')],
            evidence_dir=self.evidence_dir,
        )

        with self.assertRaisesRegex(DriverError, "returned non-JSON text: broken"):
            transport.call("click", {})

        self.assertEqual(transport.run_count, 1)
        self.assertEqual(transport.transport_faults, 1)

    def test_nonzero_exit_is_not_retried(self) -> None:
        transport = ScriptedCliTransport(
            [completed("", returncode=2, stderr="driver failed")],
            evidence_dir=self.evidence_dir,
        )

        with self.assertRaisesRegex(DriverError, "driver failed"):
            transport.call("get_window_state", {})

        self.assertEqual(transport.run_count, 1)
        self.assertEqual(transport.transport_faults, 1)
        self.assertEqual(self.fault_records()[0]["returncode"], 2)

    def test_read_timeout_retries_and_is_retained(self) -> None:
        timeout = subprocess.TimeoutExpired(["cua-driver"], 30, output="partial")
        screen = '{"width": 1440, "height": 900, "scale_factor": 1.0}'
        transport = ScriptedCliTransport(
            [timeout, completed(screen)],
            evidence_dir=self.evidence_dir,
        )

        self.assertEqual(transport.call("get_screen_size", {}), json.loads(screen))
        self.assertEqual(self.fault_records()[0]["stdout_head"], "partial")

    def test_fault_log_stops_at_the_line_cap_and_says_so(self) -> None:
        # Per-field truncation bounds a line, not the file; a permanently broken
        # driver writes one record per call for a whole run.
        overshoot = 5
        attempts = MAX_RETAINED_FAULT_LINES + overshoot
        transport = ScriptedCliTransport(
            [completed("junk")] * attempts, evidence_dir=self.evidence_dir
        )

        for _attempt in range(attempts):
            with self.assertRaises(DriverError):
                transport.call("click", {})

        records = self.fault_records()
        self.assertEqual(len(records), MAX_RETAINED_FAULT_LINES + 1)
        self.assertEqual(records[-1]["truncated"], True)
        self.assertEqual(records[-1]["retained"], MAX_RETAINED_FAULT_LINES)
        self.assertEqual(transport.transport_faults, attempts)

    def test_a_confirmation_glyph_is_never_a_verified_outcome(self) -> None:
        transport = ScriptedCliTransport(
            [completed("✅ action completed")], evidence_dir=self.evidence_dir
        )

        self.assertEqual(
            transport.call("click", {}),
            {"effect": "unverifiable", "verified": False},
        )
        self.assertEqual(transport.transport_faults, 1)
        self.assertEqual(
            self.fault_records()[0]["stdout_head"], "✅ action completed"
        )

    def test_a_confirmation_glyph_cannot_stand_in_for_a_snapshot(self) -> None:
        transport = ScriptedCliTransport(
            [completed("✅ done")] * 3, evidence_dir=self.evidence_dir
        )

        with self.assertRaisesRegex(DriverError, "returned non-JSON text: ✅ done"):
            transport.call("get_window_state", {})

        self.assertEqual(transport.run_count, 3)
        self.assertEqual(transport.transport_faults, 3)


if __name__ == "__main__":
    unittest.main(verbosity=2)
