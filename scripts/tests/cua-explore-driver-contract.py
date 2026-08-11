#!/usr/bin/env python3
"""Production-path regressions for the cua-driver snapshot contract."""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
FIXTURE = (
    REPO_ROOT
    / "scripts"
    / "tests"
    / "fixtures"
    / "night-2026-08-10-ambiguous-cells.json"
)
sys.path.insert(0, str(EXPLORE_ROOT))

from actions import ActivateAction, PressAction, ScrollAction, TypeAction  # noqa: E402
from driver import CliTransport, CuaExecutor, DriverError  # noqa: E402
from driver_transport import SUCCESS_CONTRACT, response_dispatched  # noqa: E402
from hover_geometry import WindowGeometry  # noqa: E402
from oracles import ActionEvidence  # noqa: E402


REFUSAL_TEXT = (
    '{"status": "refused", "refusal": {"code": "snapshot_id_required", '
    '"message": "click: bare element_index is not accepted in Cua Driver 0.17; '
    'pass element_token or snapshot_id with element_index"}}'
)

# Recorded from cua-driver 0.19.3 on 2026-08-10: private Xvfb, openbox,
# at-spi-bus-launcher, `cua-driver serve --no-overlay`, a zenity dialog as the
# target. Every string below is that driver's own stdout for the tool named in
# the key, copied verbatim including its session ids.
MEASURED_SUCCESS = {
    "click": '{"delivery": {"mode": "background"}, "effect": "unverifiable", '
    '"route": "accessibility"}',
    # No delivery-confirmed type_text was captured: against a zenity entry the
    # driver reported delivery_failed every single time. This is that measured
    # answer - it satisfies the contract (it carries `effect`) and is proven
    # undelivered further down, which is exactly the fourth shell.
    "type_text": '{"delivery": {"mode": "foreground"}, "effect": '
    '"unverifiable", "escalation": {"reason": "delivery_failed", "target": '
    '"foreground"}, "route": "accessibility"}',
    "press_key": '{"delivery": {"mode": "foreground"}, "effect": '
    '"unverifiable", "route": "synthetic_events"}',
    "hotkey": '{"delivery": {"mode": "foreground"}, "effect": "unverifiable", '
    '"route": "synthetic_events"}',
    "scroll": '{"delivery": {"mode": "foreground"}, "effect": "unverifiable", '
    '"route": "global_input"}',
    "move_cursor": '{"delivery": {"mode": "not_applicable"}, "effect": '
    '"unverifiable", "route": "global_input"}',
    # get_window_state answers with the recorded fixture next to this table.
    "get_cursor_position": '{"source": "x11", "x": 250, "y": 250}',
    "get_screen_size": '{"height": 1000, "scale_factor": 1.0, "width": 1600}',
    "list_windows": '{"windows": [{"app_name": "zenity", "bounds": {"height": '
    '260, "width": 310, "x": 645, "y": 370}, "height": 260, "is_on_screen": '
    'true, "pid": 69213, "title": "Contract Probe", "width": 310, '
    '"window_id": 8388613, "x": 645, "y": 370, "z_index": 0}]}',
    "set_agent_cursor_enabled": '{"enabled": true, "session": '
    '"contract-probe-69086"}',
}

# The shells that are not successes. Each was recorded from a real tool in the
# same sessions, and none of them is tool-specific: the driver reuses them, so
# every tool the harness calls has to reject every one of them. All three exit
# 0, and only the first carries `status`/`refusal` - which is why the absence
# of a known error marker cannot mean success.
MEASURED_ERROR_SHELLS = {
    # click, bare element_index
    "refusal": REFUSAL_TEXT,
    # press_key and hotkey against an unfocused window
    "code_object": '{"code": "background_unavailable", "detail": "the '
    'requested target has no focus-free input backend; the remaining '
    'XTest/X11 route can only deliver to the globally focused widget", '
    '"escalation": {"reason": "background input is unavailable on this '
    'surface; retry this action with delivery_mode:\\"foreground\\".", '
    '"recommended": "foreground"}, "suggestion": "Retry this action with '
    'delivery_mode:\\"foreground\\"."}',
    # get_cursor_position and move_cursor on a session-scoped call
    "escalation_required": '{"capture_scope": "auto", "code": '
    '"desktop_escalation_required", "desktop_unlocked": false, '
    '"effective_scope": "window", "escalation_detail": null, '
    '"escalation_reason": null, "session": "contract-probe-69086"}',
}
BACKGROUND_UNAVAILABLE = MEASURED_ERROR_SHELLS["code_object"]


def completed(stdout: str, *, returncode: int = 0, stderr: str = ""):
    return subprocess.CompletedProcess([], returncode, stdout, stderr)


class CommandScriptTransport(CliTransport):
    """Run the real CLI transport parser against scripted process results."""

    def __init__(self, responses, *, evidence_dir: pathlib.Path) -> None:
        super().__init__(evidence_dir=evidence_dir)
        self.responses = list(responses)
        self.commands: list[list[str]] = []

    def _run(self, command):
        self.commands.append(list(command))
        return self.responses.pop(0)


class SuccessContractTests(unittest.TestCase):
    """Every tool the harness calls, told apart from the driver's error shells.

    The old rule listed the failures it knew and passed everything else. Three
    shells were known, a fourth existed, and a whole night of hover evidence
    was recorded from an error object nobody counted.
    """

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.evidence_dir = pathlib.Path(self.temporary.name)
        self.measured = dict(MEASURED_SUCCESS)
        self.measured["get_window_state"] = FIXTURE.read_text(encoding="utf-8")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def transport(self, *stdout: str) -> CommandScriptTransport:
        return CommandScriptTransport(
            [completed(item) for item in stdout], evidence_dir=self.evidence_dir
        )

    def test_the_table_covers_every_tool_the_harness_calls(self) -> None:
        called = set()
        for path in sorted(EXPLORE_ROOT.glob("*.py")):
            source = path.read_text(encoding="utf-8")
            for chunk in source.split(".call(")[1:]:
                name = chunk.strip().split(",")[0].strip().strip("\"'")
                if name.isidentifier() and not name.startswith("self"):
                    called.add(name)
        self.assertIn("click", called)
        self.assertIn("get_window_state", called)
        self.assertTrue(called <= set(SUCCESS_CONTRACT), called - set(SUCCESS_CONTRACT))
        self.assertEqual(set(self.measured), set(SUCCESS_CONTRACT))

    def test_a_measured_success_passes_for_every_tool(self) -> None:
        for tool, stdout in self.measured.items():
            with self.subTest(tool=tool):
                transport = self.transport(stdout)

                self.assertEqual(transport.call(tool, {}), json.loads(stdout))

    def test_no_error_shell_passes_for_any_tool(self) -> None:
        for tool in SUCCESS_CONTRACT:
            for shell, stdout in MEASURED_ERROR_SHELLS.items():
                with self.subTest(tool=tool, shell=shell):
                    transport = self.transport(*[stdout] * 3)

                    with self.assertRaises(DriverError):
                        transport.call(tool, {})

                    self.assertGreaterEqual(transport.transport_faults, 1)

    def test_a_tool_without_a_contract_cannot_pass(self) -> None:
        transport = self.transport('{"anything": true}')

        with self.assertRaisesRegex(DriverError, "no success contract"):
            transport.call("set_value", {})

    def test_a_failed_delivery_is_a_fault_and_never_counts_as_dispatched(self) -> None:
        stdout = MEASURED_SUCCESS["type_text"]
        transport = self.transport(stdout)

        response = transport.call("type_text", {})

        self.assertEqual(response, json.loads(stdout))
        self.assertFalse(response_dispatched(response))
        self.assertEqual(transport.transport_faults, 1)
        record = json.loads(
            (self.evidence_dir / "driver-faults.jsonl").read_text(encoding="utf-8")
        )
        self.assertEqual(record["response"], json.loads(stdout))

    def test_a_delivered_action_of_the_same_shape_still_counts(self) -> None:
        # The delivered and the undelivered answer differ in one key. A rule
        # that reads "unverifiable" as a failure would stop the whole harness:
        # every measured action outcome above says exactly that.
        stdout = MEASURED_SUCCESS["click"]
        transport = self.transport(stdout)

        response = transport.call("click", {})

        self.assertTrue(response_dispatched(response))
        self.assertEqual(transport.transport_faults, 0)

class DriverRefusalContractTests(unittest.TestCase):
    def test_background_unavailable_retries_foreground_and_retains_step_evidence(
        self,
    ) -> None:
        raw = json.loads(FIXTURE.read_text(encoding="utf-8"))
        foreground = {
            "delivery": {"mode": "foreground"},
            "effect": "unverifiable",
            "route": "synthetic_events",
        }
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = pathlib.Path(directory)
            transport = CommandScriptTransport(
                [
                    completed(json.dumps(raw)),
                    completed(BACKGROUND_UNAVAILABLE),
                    completed(json.dumps(foreground)),
                    completed(json.dumps(raw)),
                ],
                evidence_dir=evidence_dir,
            )
            executor = CuaExecutor(
                transport,
                pid=44,
                window_id=77,
                session="contract",
                fixture_tokens={"trusted": "fixture text"},
                evidence_dir=evidence_dir,
                settle_delays=(),
            )

            result = executor.execute(
                TypeAction("state-1", "☆", "ax", "trusted")
            )

            self.assertTrue(result.evidence.dispatched)
            self.assertIn(
                "driver-transport-fault",
                {finding.code for finding in result.findings},
            )
            self.assertEqual(transport.transport_faults, 1)
            action_commands = [
                command for command in transport.commands if command[1] == "type_text"
            ]
            self.assertEqual(len(action_commands), 2)
            self.assertNotIn("delivery_mode", json.loads(action_commands[0][2]))
            self.assertEqual(
                json.loads(action_commands[1][2])["delivery_mode"], "foreground"
            )
            retained = json.loads(
                (evidence_dir / "step-0001-result.json").read_text(encoding="utf-8")
            )
            self.assertEqual(
                retained["action_response"]["delivery_escalation"],
                {
                    "code": "background_unavailable",
                    "from": "background",
                    "to": "foreground",
                },
            )
            fault = json.loads(
                (evidence_dir / "driver-faults.jsonl").read_text(encoding="utf-8")
            )
            self.assertEqual(fault["response"], json.loads(BACKGROUND_UNAVAILABLE))

    def test_unknown_error_envelope_still_aborts_without_foreground_retry(self) -> None:
        raw = json.loads(FIXTURE.read_text(encoding="utf-8"))
        unknown = '{"code":"future_driver_error","detail":"not delivered"}'
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = pathlib.Path(directory)
            transport = CommandScriptTransport(
                [completed(json.dumps(raw)), completed(unknown)],
                evidence_dir=evidence_dir,
            )
            executor = CuaExecutor(
                transport,
                pid=44,
                window_id=77,
                session="contract",
                fixture_tokens={"trusted": "fixture text"},
                evidence_dir=evidence_dir,
                settle_delays=(),
            )

            with self.assertRaisesRegex(DriverError, "future_driver_error"):
                executor.execute(TypeAction("state-1", "☆", "ax", "trusted"))

            self.assertEqual(
                [command[1] for command in transport.commands],
                ["get_window_state", "type_text"],
            )
            self.assertEqual(transport.transport_faults, 1)

    def test_background_unavailable_escalates_other_schema_compatible_actions(
        self,
    ) -> None:
        foreground = '{"effect":"unverifiable","route":"synthetic_events"}'
        for tool, payload in (
            ("press_key", {"key": "enter"}),
            ("hotkey", {"keys": ["CTRL", "F"]}),
            # `describe` lists delivery_mode for these two as well, and a
            # pointer action meets the same missing background backend.
            ("click", {"element_token": "fixture-token"}),
            ("scroll", {"element_token": "fixture-token", "direction": "down"}),
        ):
            with self.subTest(tool=tool), tempfile.TemporaryDirectory() as directory:
                transport = CommandScriptTransport(
                    [completed(BACKGROUND_UNAVAILABLE), completed(foreground)],
                    evidence_dir=pathlib.Path(directory),
                )

                response = transport.call(tool, payload)

                self.assertEqual(response["delivery_escalation"]["to"], "foreground")
                self.assertEqual(
                    json.loads(transport.commands[1][2])["delivery_mode"],
                    "foreground",
                )

    def test_background_unavailable_does_not_extend_an_unsupported_schema(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = CommandScriptTransport(
                [completed(BACKGROUND_UNAVAILABLE)],
                evidence_dir=pathlib.Path(directory),
            )

            # `describe move_cursor` lists no delivery_mode, so there is no
            # documented escape to take and the refusal still ends the run.
            with self.assertRaisesRegex(DriverError, "background_unavailable"):
                transport.call("move_cursor", {"scope": "desktop", "x": 1, "y": 2})

            self.assertEqual(len(transport.commands), 1)

    def test_the_delivery_shell_survives_the_escape_without_ending_the_run(
        self,
    ) -> None:
        # Measured on cua-driver 0.19.3: every type_text answers with
        # escalation.reason "delivery_failed", a successful foreground one
        # included - the typed marker was read back out of the accessibility
        # tree. Aborting on it would abort on the driver's normal answer, and
        # first-time-exploration is exactly the mission that lands here.
        with tempfile.TemporaryDirectory() as directory:
            transport = CommandScriptTransport(
                [
                    completed(BACKGROUND_UNAVAILABLE),
                    completed(MEASURED_SUCCESS["type_text"]),
                ],
                evidence_dir=pathlib.Path(directory),
            )

            response = transport.call("type_text", {"text": "fixture text"})

            self.assertEqual(len(transport.commands), 2)
            self.assertEqual(transport.transport_faults, 2)
            self.assertEqual(response["delivery_escalation"]["to"], "foreground")
            self.assertIn("delivery_failure", response["delivery_escalation"])
            # The run continues, but nothing counts as delivered.
            self.assertFalse(response_dispatched(response))

    def test_exit_zero_snapshot_refusal_aborts_the_production_action_path(self) -> None:
        raw = json.loads(FIXTURE.read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = pathlib.Path(directory)
            transport = CommandScriptTransport(
                [
                    completed(json.dumps(raw)),
                    completed(REFUSAL_TEXT, returncode=0),
                    completed(json.dumps(raw)),
                ],
                evidence_dir=evidence_dir,
            )
            executor = CuaExecutor(
                transport,
                pid=44,
                window_id=77,
                session="contract",
                settle_delays=(),
            )

            with self.assertRaisesRegex(DriverError, "snapshot_id_required"):
                executor.execute_evidence(
                    ActionEvidence.activate("☆", expect_effect="idempotent")
                )

            self.assertEqual(
                [command[1] for command in transport.commands],
                ["get_window_state", "click"],
            )
            self.assertEqual(transport.transport_faults, 1)
            fault_path = evidence_dir / "driver-faults.jsonl"
            records = [
                json.loads(line)
                for line in fault_path.read_text(encoding="utf-8").splitlines()
            ]
            self.assertEqual(records[0]["response"], json.loads(REFUSAL_TEXT))

    def test_status_failure_and_refusal_are_independent_transport_errors(self) -> None:
        responses = (
            '{"status":"failed","error":"input was not delivered"}',
            '{"status":"success","refusal":{"code":"policy","message":"no"}}',
        )
        for index, response in enumerate(responses):
            with self.subTest(response=response):
                evidence_dir = pathlib.Path(self._temporary.name) / str(index)
                transport = CommandScriptTransport(
                    [completed(response)], evidence_dir=evidence_dir
                )

                with self.assertRaises(DriverError):
                    transport.call("click", {})

                record = json.loads(
                    (evidence_dir / "driver-faults.jsonl")
                    .read_text(encoding="utf-8")
                    .strip()
                )
                self.assertEqual(record["response"], json.loads(response))

    def setUp(self) -> None:
        self._temporary = tempfile.TemporaryDirectory()

    def tearDown(self) -> None:
        self._temporary.cleanup()


class ElementAddressContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.evidence_dir = pathlib.Path(self.temporary.name)
        self.raw = json.loads(FIXTURE.read_text(encoding="utf-8"))
        self.target = next(
            item for item in self.raw["elements"] if item.get("label") == "☆"
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def transport(self, responses) -> CommandScriptTransport:
        return CommandScriptTransport(responses, evidence_dir=self.evidence_dir)

    @staticmethod
    def click_payloads(transport: CommandScriptTransport) -> list[dict]:
        return [
            json.loads(command[2])
            for command in transport.commands
            if command[1] == "click"
        ]

    def test_direct_evidence_click_sends_the_snapshot_element_token(self) -> None:
        transport = self.transport(
            [
                completed(json.dumps(self.raw)),
                completed('{"effect":"unverifiable","route":"accessibility"}'),
                completed(json.dumps(self.raw)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            settle_delays=(),
        )

        executor.execute_evidence(
            ActionEvidence.activate("☆", expect_effect="idempotent")
        )

        payload = self.click_payloads(transport)[0]
        self.assertEqual(payload["element_token"], self.target["element_token"])
        self.assertNotIn("element_index", payload)

    def test_accepted_action_click_sends_the_snapshot_element_token(self) -> None:
        transport = self.transport(
            [
                completed(json.dumps(self.raw)),
                completed('{"effect":"unverifiable","route":"accessibility"}'),
                completed(json.dumps(self.raw)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            settle_delays=(),
        )

        executor.execute(
            ActivateAction("state-1", "☆", "ax", "idempotent")
        )

        payload = self.click_payloads(transport)[0]
        self.assertEqual(payload["element_token"], self.target["element_token"])
        self.assertNotIn("element_index", payload)

    def test_pointer_noop_probe_uses_the_after_snapshot_element_token(self) -> None:
        # A second get_window_state is a new generation: a new snapshot_id and
        # a token per element that embeds it.
        after = json.loads(json.dumps(self.raw))
        after["snapshot_id"] = "s00000009"
        for item in after["elements"]:
            index = str(item["element_token"]).partition(":")[2]
            item["element_token"] = f"s00000009:{index}"
        after_target = next(
            item for item in after["elements"] if item.get("label") == "☆"
        )
        after_target["element_token"] = "s00000009:35"
        transport = self.transport(
            [
                completed(json.dumps(self.raw)),
                completed('{"effect":"unverifiable","route":"pixel"}'),
                completed(json.dumps(after)),
                completed('{"effect":"unverifiable","route":"accessibility"}'),
                completed(json.dumps(after)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            settle_delays=(),
            window_origin=WindowGeometry(0, 0, 1600, 1000),
        )

        executor.execute_evidence(
            ActionEvidence.activate("☆", dispatch="px", expect_effect="required")
        )

        pointer, semantic_probe = self.click_payloads(transport)
        self.assertIn("x", pointer)
        self.assertEqual(semantic_probe["element_token"], "s00000009:35")
        self.assertNotIn("element_index", semantic_probe)

    def test_missing_token_falls_back_to_index_plus_the_same_snapshot_id(self) -> None:
        raw = json.loads(json.dumps(self.raw))
        raw["snapshot_id"] = "s00000004"
        target = next(item for item in raw["elements"] if item.get("label") == "☆")
        target.pop("element_token")
        transport = self.transport(
            [
                completed(json.dumps(raw)),
                completed('{"effect":"unverifiable","route":"accessibility"}'),
                completed(json.dumps(raw)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            settle_delays=(),
        )

        executor.execute_evidence(
            ActionEvidence.activate("☆", expect_effect="idempotent")
        )

        payload = self.click_payloads(transport)[0]
        self.assertEqual(payload["element_index"], target["element_index"])
        self.assertEqual(payload["snapshot_id"], "s00000004")
        self.assertNotIn("element_token", payload)

    def test_a_token_from_its_own_snapshot_survives_the_freshness_check(self) -> None:
        # The fixture names its snapshot, so both halves of the check run: the
        # token is compared against a present snapshot_id and passes. Weakening
        # the comparison in snapshot_element_address turns this test red.
        self.assertEqual(self.raw["snapshot_id"], "s00000004")
        self.assertTrue(
            self.target["element_token"].startswith(self.raw["snapshot_id"] + ":")
        )
        transport = self.transport(
            [
                completed(json.dumps(self.raw)),
                completed('{"effect":"unverifiable","route":"accessibility"}'),
                completed(json.dumps(self.raw)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            settle_delays=(),
        )

        executor.execute_evidence(
            ActionEvidence.activate("☆", expect_effect="idempotent")
        )

        payload = self.click_payloads(transport)[0]
        self.assertEqual(payload["element_token"], self.target["element_token"])

    def test_a_snapshot_that_does_not_name_itself_fails_before_dispatch(self) -> None:
        # Without a snapshot_id there is nothing to check the token against,
        # and a token from an older generation would go out looking current.
        raw = json.loads(json.dumps(self.raw))
        raw.pop("snapshot_id")
        transport = self.transport([completed(json.dumps(raw))])
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            settle_delays=(),
        )

        with self.assertRaisesRegex(DriverError, "does not name itself"):
            executor.execute_evidence(
                ActionEvidence.activate("☆", expect_effect="idempotent")
            )

        self.assertEqual(self.click_payloads(transport), [])

    def test_mismatched_snapshot_and_element_token_fail_before_dispatch(self) -> None:
        raw = json.loads(json.dumps(self.raw))
        raw["snapshot_id"] = "s00000009"
        transport = self.transport([completed(json.dumps(raw))])
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            settle_delays=(),
        )

        with self.assertRaisesRegex(DriverError, "does not belong"):
            executor.execute_evidence(
                ActionEvidence.activate("☆", expect_effect="idempotent")
            )

        self.assertEqual(self.click_payloads(transport), [])

    def test_other_element_addressed_tools_send_the_snapshot_token(self) -> None:
        cases = (
            (
                "type_text",
                TypeAction("state-1", "☆", "ax", "trusted"),
                {"trusted": "fixture text"},
            ),
            ("press_key", PressAction("state-1", "enter", "☆"), {}),
            (
                "scroll",
                ScrollAction("state-1", "down", 3, "line", "☆"),
                {},
            ),
        )
        for tool, action, fixture_tokens in cases:
            with self.subTest(tool=tool):
                transport = self.transport(
                    [
                        completed(json.dumps(self.raw)),
                        completed('{"effect":"unverifiable"}'),
                        completed(json.dumps(self.raw)),
                    ]
                )
                executor = CuaExecutor(
                    transport,
                    pid=44,
                    window_id=77,
                    session="contract",
                    fixture_tokens=fixture_tokens,
                    settle_delays=(),
                )

                executor.execute(action)

                command = next(item for item in transport.commands if item[1] == tool)
                payload = json.loads(command[2])
                self.assertEqual(
                    payload["element_token"], self.target["element_token"]
                )
                self.assertNotIn("element_index", payload)

    def test_delivered_but_ineffective_click_remains_a_noop_product_finding(self) -> None:
        transport = self.transport(
            [
                completed(json.dumps(self.raw)),
                completed('{"effect":"unverifiable","route":"accessibility"}'),
                completed(json.dumps(self.raw)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            settle_delays=(),
        )

        result = executor.execute_evidence(ActionEvidence.activate("☆"))

        self.assertEqual(result.evidence.effect, "suspected_noop")
        self.assertIn(
            "suspected-no-handler", {finding.code for finding in result.findings}
        )
        self.assertEqual(transport.transport_faults, 0)

    def test_an_undelivered_click_is_never_blamed_on_the_product(self) -> None:
        # Same unchanged snapshots as the test above; only the driver's answer
        # differs. Booking this as delivered is what turned a driver fault into
        # a dead-handler finding against the app.
        transport = self.transport(
            [
                completed(json.dumps(self.raw)),
                completed(MEASURED_SUCCESS["type_text"]),
                completed(json.dumps(self.raw)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            settle_delays=(),
        )

        result = executor.execute_evidence(ActionEvidence.activate("☆"))

        codes = {finding.code for finding in result.findings}
        self.assertFalse(result.evidence.dispatched)
        self.assertNotEqual(result.evidence.effect, "suspected_noop")
        self.assertNotIn("suspected-no-handler", codes)
        self.assertNotIn("click-no-visible-effect", codes)
        self.assertIn("driver-action-undelivered", codes)
        self.assertEqual(transport.transport_faults, 1)


if __name__ == "__main__":
    unittest.main(verbosity=2)
