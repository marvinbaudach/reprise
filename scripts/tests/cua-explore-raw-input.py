#!/usr/bin/env python3
"""Raw input on a private Xvfb: foreground up front, bounded escapes, no abort."""

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

from driver import CliTransport, DriverError  # noqa: E402
from driver_transport import response_dispatched  # noqa: E402


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


class RawInputDeliveryTests(unittest.TestCase):
    """A private Xvfb has no input hotplug, so raw input must not try background.

    Measured on cua-driver 0.33.3 (issue 1092): a background pixel click that
    misses an AT-SPI action, a background scroll, text without an element and
    any key without an element all wait about 5 s for a uinput slave device
    that Xvfb never attaches, then fail with background_pointer_failed or
    "virtual master keyboard delivery failed". Foreground delivery uses XTest.
    """

    BACKGROUND_POINTER_FAILED = json.dumps(
        {
            "code": "background_pointer_failed",
            "effect": "none",
            "hint": "Click by element_token (AT-SPI action) or retry with "
            'delivery_mode:"foreground".',
            "path": "mpx_pointer",
            "reason": "timed out waiting for X input slave device "
            "'CUA v1 uinput pointer'",
        }
    )
    KEYBOARD_FAILED = (
        "virtual master keyboard delivery failed: timed out waiting for X "
        "input slave device 'CUA v1 uinput pointer'"
    )
    DELIVERED = '{"effect":"unverifiable","route":"global_input"}'

    def sent(self, tool: str, payload: dict) -> dict:
        with tempfile.TemporaryDirectory() as directory:
            transport = CommandScriptTransport(
                [completed(self.DELIVERED)], evidence_dir=pathlib.Path(directory)
            )
            transport.call(tool, payload)
            self.assertEqual(len(transport.commands), 1)
            return json.loads(transport.commands[0][2])

    def test_raw_input_asks_for_foreground_on_the_first_attempt(self) -> None:
        for tool, payload in (
            ("click", {"x": 10, "y": 20}),
            ("scroll", {"direction": "down"}),
            ("type_text", {"text": "t"}),
            ("press_key", {"key": "escape"}),
            ("hotkey", {"keys": ["ctrl", "f"]}),
        ):
            with self.subTest(tool=tool):
                self.assertEqual(
                    self.sent(tool, payload)["delivery_mode"], "foreground"
                )

    def test_element_addressed_input_keeps_the_atspi_route(self) -> None:
        for tool, payload in (
            ("click", {"element_token": "s00000001:3"}),
            ("click", {"element_index": 3, "snapshot_id": "s00000001"}),
            ("type_text", {"element_token": "s00000001:3", "text": "t"}),
            ("scroll", {"element_token": "s00000001:3", "direction": "down"}),
        ):
            with self.subTest(tool=tool, payload=payload):
                self.assertNotIn("delivery_mode", self.sent(tool, payload))

    def test_an_explicit_delivery_mode_is_never_overridden(self) -> None:
        sent = self.sent("click", {"x": 1, "y": 2, "delivery_mode": "background"})
        self.assertEqual(sent["delivery_mode"], "background")

    def test_tools_without_delivery_mode_never_receive_it(self) -> None:
        self.assertNotIn(
            "delivery_mode",
            self.sent("move_cursor", {"scope": "desktop", "x": 1, "y": 2}),
        )

    def test_background_pointer_failed_takes_the_bounded_foreground_escape(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = CommandScriptTransport(
                [
                    # Measured: the code object arrives with exit status 1.
                    completed(self.BACKGROUND_POINTER_FAILED, returncode=1),
                    completed(self.DELIVERED),
                ],
                evidence_dir=pathlib.Path(directory),
            )

            response = transport.call("click", {"element_token": "s00000001:3"})

            self.assertEqual(
                response["delivery_escalation"],
                {
                    "code": "background_pointer_failed",
                    "from": "background",
                    "to": "foreground",
                },
            )
            self.assertEqual(
                json.loads(transport.commands[1][2])["delivery_mode"], "foreground"
            )
            self.assertEqual(transport.transport_faults, 1)

    def test_a_snapshot_gets_a_walk_budget_that_does_not_truncate_the_tree(
        self,
    ) -> None:
        raw = FIXTURE.read_text(encoding="utf-8")
        for payload, expected in (({}, 10_000), ({"timeout_ms": 2500}, 2500)):
            with self.subTest(payload=payload), tempfile.TemporaryDirectory() as directory:
                transport = CommandScriptTransport(
                    [completed(raw)], evidence_dir=pathlib.Path(directory)
                )
                transport.call("get_window_state", payload)
                sent = json.loads(transport.commands[0][2])
                self.assertEqual(sent["timeout_ms"], expected)

    def test_a_refusal_that_proves_nothing_arrived_is_undelivered_not_fatal(
        self,
    ) -> None:
        refusal = json.dumps(
            {
                "code": "element_bounds_unavailable",
                "effect": "none",
                "element_index": 46,
                "owner": "generic (unnamed)",
                "reason": "point_owned_by_another_element",
            }
        )
        with tempfile.TemporaryDirectory() as directory:
            transport = CommandScriptTransport(
                [completed(refusal, returncode=1)],
                evidence_dir=pathlib.Path(directory),
            )

            response = transport.call("click", {"element_token": "s00000001:46"})

            self.assertFalse(response_dispatched(response))
            self.assertEqual(transport.transport_faults, 1)
            self.assertEqual(len(transport.commands), 1)

    def test_other_refusals_with_exit_one_still_end_the_call(self) -> None:
        refusal = json.dumps({"code": "stale_element", "effect": "none"})
        with tempfile.TemporaryDirectory() as directory:
            transport = CommandScriptTransport(
                [completed(refusal, returncode=1)],
                evidence_dir=pathlib.Path(directory),
            )

            with self.assertRaisesRegex(DriverError, "stale_element"):
                transport.call("click", {"element_token": "s00000001:46"})

    def test_a_foreground_pointer_failure_still_ends_the_call(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = CommandScriptTransport(
                [completed(self.BACKGROUND_POINTER_FAILED, returncode=1)],
                evidence_dir=pathlib.Path(directory),
            )

            with self.assertRaisesRegex(DriverError, "background_pointer_failed"):
                transport.call("click", {"x": 1, "y": 2})

            self.assertEqual(len(transport.commands), 1)

    def test_the_plain_text_keyboard_failure_takes_the_same_escape(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = CommandScriptTransport(
                [
                    completed(self.KEYBOARD_FAILED, returncode=1),
                    completed(self.DELIVERED),
                ],
                evidence_dir=pathlib.Path(directory),
            )

            response = transport.call(
                "type_text", {"element_token": "s00000001:3", "text": "t"}
            )

            self.assertEqual(response["delivery_escalation"]["to"], "foreground")
            self.assertEqual(
                json.loads(transport.commands[1][2])["delivery_mode"], "foreground"
            )

    def test_a_foreground_keyboard_failure_still_ends_the_call(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = CommandScriptTransport(
                [completed(self.KEYBOARD_FAILED, returncode=1)],
                evidence_dir=pathlib.Path(directory),
            )

            with self.assertRaisesRegex(DriverError, "virtual master keyboard"):
                transport.call("press_key", {"key": "escape"})

            self.assertEqual(len(transport.commands), 1)


if __name__ == "__main__":
    unittest.main(verbosity=2)
