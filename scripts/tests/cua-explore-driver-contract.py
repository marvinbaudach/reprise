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
from hover_geometry import WindowGeometry  # noqa: E402
from oracles import ActionEvidence  # noqa: E402


REFUSAL_TEXT = (
    '{"status": "refused", "refusal": {"code": "snapshot_id_required", '
    '"message": "click: bare element_index is not accepted in Cua Driver 0.17; '
    'pass element_token or snapshot_id with element_index"}}'
)


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


class DriverRefusalContractTests(unittest.TestCase):
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
        after = json.loads(json.dumps(self.raw))
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


if __name__ == "__main__":
    unittest.main(verbosity=2)
