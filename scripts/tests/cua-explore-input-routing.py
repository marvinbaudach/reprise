#!/usr/bin/env python3
"""How the executor routes clicks, typing and key presses to cua-driver 0.33."""

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

from actions import PressAction, TypeAction  # noqa: E402
from driver import CliTransport, CuaExecutor, DriverError  # noqa: E402
from driver_transport import response_dispatched  # noqa: E402
from hover_geometry import WindowGeometry  # noqa: E402
from oracles import ActionEvidence  # noqa: E402


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


class InputRoutingTests(unittest.TestCase):
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

    def test_a_pixel_click_is_sent_in_the_pixels_of_the_shrunken_screenshot(
        self,
    ) -> None:
        # cua-driver 0.33 shrinks the screenshot of a large window (frame_scale
        # 0.8475 at 1600x1000) and reads click x/y in its pixels. Window pixels
        # sent unscaled landed 1/0.8475 too far out, outside the window for
        # anything near the right edge.
        for scale, expected in ((None, (1374.0, 348.0)), (0.5, (687.0, 174.0))):
            with self.subTest(scale=scale):
                raw = json.loads(json.dumps(self.raw))
                if scale is not None:
                    raw["frame_scale"] = scale
                transport = self.transport(
                    [
                        completed(json.dumps(raw)),
                        completed('{"effect":"unverifiable","route":"global_input"}'),
                        completed(json.dumps(raw)),
                    ]
                )
                executor = CuaExecutor(
                    transport,
                    pid=44,
                    window_id=77,
                    session="contract",
                    window_origin=WindowGeometry(0, 0, 1600, 1000),
                    settle_delays=(),
                )

                executor.execute_evidence(
                    ActionEvidence.activate(
                        "☆", dispatch="px", expect_effect="idempotent"
                    )
                )

                payload = self.click_payloads(transport)[0]
                self.assertEqual((payload["x"], payload["y"]), expected)

    def test_an_unusable_frame_scale_fails_before_any_click(self) -> None:
        raw = json.loads(json.dumps(self.raw))
        raw["frame_scale"] = 0
        transport = self.transport([completed(json.dumps(raw))])
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            window_origin=WindowGeometry(0, 0, 1600, 1000),
            settle_delays=(),
        )

        with self.assertRaisesRegex(DriverError, "frame_scale"):
            executor.execute_evidence(
                ActionEvidence.activate("☆", dispatch="px", expect_effect="idempotent")
            )

        self.assertEqual(self.click_payloads(transport), [])

    def test_an_ax_click_on_a_target_with_no_action_goes_straight_to_foreground(
        self,
    ) -> None:
        # No AT-SPI action means the driver falls to the pointer, which only
        # works in foreground on Xvfb; the background attempt would spend 5 s.
        for actions, expected in (
            (["listitem.scroll-to"], "foreground"),
            (["click"], None),
        ):
            with self.subTest(actions=actions):
                raw = json.loads(json.dumps(self.raw))
                for item in raw["elements"]:
                    if item.get("label") == "☆":
                        item["actions"] = list(actions)
                transport = self.transport(
                    [
                        completed(json.dumps(raw)),
                        completed('{"effect":"unverifiable","route":"x"}'),
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
                self.assertEqual(payload.get("delivery_mode"), expected)
                self.assertIn("element_token", payload)

    def test_an_ax_click_on_a_target_with_no_action_is_aimed_at_the_pointer(
        self,
    ) -> None:
        # A list row or column header offers no AT-SPI click, and cua-driver
        # 0.33 refuses to aim at it by element. A user clicks it, so the
        # executor does the same: by pixel, once, with no accessibility probe.
        raw = json.loads(json.dumps(self.raw))
        for item in raw["elements"]:
            if item.get("label") == "\u2606":
                item["actions"] = ["listitem.scroll-to"]
        transport = self.transport(
            [
                completed(json.dumps(raw)),
                completed('{"effect":"unverifiable","route":"global_input"}'),
                completed(json.dumps(raw)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            window_origin=WindowGeometry(0, 0, 1600, 1000),
            settle_delays=(),
        )

        result = executor.execute_evidence(
            ActionEvidence.activate("\u2606", dispatch="ax", expect_effect="required")
        )

        payloads = self.click_payloads(transport)
        self.assertEqual(len(payloads), 1)
        self.assertEqual((payloads[0]["x"], payloads[0]["y"]), (1374.0, 348.0))
        self.assertEqual(payloads[0]["delivery_mode"], "foreground")
        self.assertNotIn("element_token", payloads[0])
        self.assertEqual(result.evidence.dispatch, "px")
        self.assertEqual(result.action_response["dispatch_rerouted"]["to"], "px")
        self.assertNotIn(
            "no-accessible-action", {finding.code for finding in result.findings}
        )

    def test_an_undelivered_click_on_a_target_with_no_action_blocks_nothing(
        self,
    ) -> None:
        # Without a window origin the click cannot be aimed by pixel. The
        # driver refuses it by element; that is a harness delivery limit, so
        # it is a low-confidence harness note and never an app error.
        refusal = json.dumps(
            {
                "code": "element_bounds_unavailable",
                "effect": "none",
                "reason": "point_owned_by_another_element",
            }
        )
        raw = json.loads(json.dumps(self.raw))
        for item in raw["elements"]:
            if item.get("label") == "\u2606":
                item["actions"] = ["listitem.scroll-to"]
        transport = self.transport(
            [
                completed(json.dumps(raw)),
                completed(refusal, returncode=1),
                completed(json.dumps(raw)),
            ]
        )
        executor = CuaExecutor(
            transport, pid=44, window_id=77, session="contract", settle_delays=()
        )

        result = executor.execute_evidence(
            ActionEvidence.activate("\u2606", dispatch="ax", expect_effect="required")
        )

        by_code = {finding.code: finding for finding in result.findings}
        self.assertNotIn("no-accessible-action", by_code)
        note = by_code["driver-action-undelivered"]
        self.assertFalse(note.blocks_gate)
        self.assertLessEqual(note.confidence, 0.3)
        self.assertFalse(any(finding.blocks_gate for finding in result.findings))

    def test_typing_into_a_non_entry_focuses_by_click_instead_of_grab_focus(
        self,
    ) -> None:
        # "Search all fields" is a toggle button until it is opened. Aimed at it,
        # type_text leaves AT-SPI EditableText for key events and calls
        # Component.GrabFocus, which GTK4 answers with NotSupported.
        transport = self.transport(
            [
                completed(json.dumps(self.raw)),
                completed('{"effect":"unverifiable","route":"global_input"}'),
                completed('{"effect":"unverifiable","route":"global_input"}'),
                completed(json.dumps(self.raw)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            fixture_tokens={"trusted": "fixture text"},
            window_origin=WindowGeometry(0, 0, 1600, 1000),
            settle_delays=(),
        )

        result = executor.execute(TypeAction("state-1", "☆", "ax", "trusted"))

        tools = [command[1] for command in transport.commands]
        self.assertEqual(
            tools, ["get_window_state", "click", "type_text", "get_window_state"]
        )
        typed = json.loads(transport.commands[2][2])
        self.assertEqual(typed["text"], "fixture text")
        self.assertEqual(typed["delivery_mode"], "foreground")
        for forbidden in ("element_token", "element_index"):
            self.assertNotIn(forbidden, typed)
        self.assertIn("focus_click", result.action_response)

    def test_a_refused_ax_probe_is_inconclusive_and_does_not_end_the_run(self) -> None:
        # After a pointer click with no visible effect the executor re-clicks the
        # same target through accessibility. cua-driver 0.33.3 refuses that on a
        # row with no AT-SPI click (exit 1, element_bounds_unavailable); the
        # refusal says nothing about the product, so no finding and no abort.
        refusal = json.dumps(
            {
                "code": "element_bounds_unavailable",
                "effect": "none",
                "element_index": 46,
                "owner": "generic (unnamed)",
                "reason": "point_owned_by_another_element",
            }
        )
        transport = self.transport(
            [
                completed(json.dumps(self.raw)),
                completed('{"effect":"unverifiable","route":"global_input"}'),
                completed(json.dumps(self.raw)),
                completed(refusal, returncode=1),
                completed(json.dumps(self.raw)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            window_origin=WindowGeometry(0, 0, 1600, 1000),
            settle_delays=(),
        )

        result = executor.execute_evidence(
            ActionEvidence.activate("☆", dispatch="px", expect_effect="required")
        )

        self.assertFalse(response_dispatched(result.action_response["ax_probe"]))
        self.assertFalse(result.evidence.ax_probe_changed)
        self.assertNotIn(
            "suspected-occlusion", {finding.code for finding in result.findings}
        )
        self.assertEqual(transport.transport_faults, 1)

    def test_a_targeted_key_press_focuses_by_click_and_never_asks_for_grab_focus(
        self,
    ) -> None:
        # press_key with an element address makes the driver call AT-SPI
        # Component.GrabFocus, which GTK4 accessibles answer with NotSupported
        # (issue 1092: the search box, and a plain Gtk.Button as well). The key
        # goes to the focused window instead, after a click that focuses it.
        transport = self.transport(
            [
                completed(json.dumps(self.raw)),
                completed('{"effect":"unverifiable","route":"global_input"}'),
                completed('{"effect":"unverifiable","route":"global_input"}'),
                completed(json.dumps(self.raw)),
            ]
        )
        executor = CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="contract",
            window_origin=WindowGeometry(0, 0, 1600, 1000),
            settle_delays=(),
        )

        result = executor.execute(PressAction("state-1", "escape", "☆"))

        tools = [command[1] for command in transport.commands]
        self.assertEqual(
            tools, ["get_window_state", "click", "press_key", "get_window_state"]
        )
        focus = json.loads(transport.commands[1][2])
        press = json.loads(transport.commands[2][2])
        self.assertEqual(focus["delivery_mode"], "foreground")
        self.assertIn("x", focus)
        self.assertNotIn("element_token", focus)
        self.assertEqual(press["key"], "escape")
        self.assertEqual(press["delivery_mode"], "foreground")
        for forbidden in ("element_token", "element_index"):
            self.assertNotIn(forbidden, press)
        self.assertIn("focus_click", result.action_response)


if __name__ == "__main__":
    unittest.main(verbosity=2)
