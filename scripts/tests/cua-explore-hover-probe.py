#!/usr/bin/env python3
"""Tests for the standalone hover measuring rig (move_cursor vs a real warp)."""

from __future__ import annotations

import pathlib
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
TEST_ROOT = REPO_ROOT / "scripts" / "tests"
sys.path.insert(0, str(EXPLORE_ROOT))
sys.path.insert(0, str(TEST_ROOT))

from cua_explore_png import write_png  # noqa: E402
from atspi_geometry import GeometryNode  # noqa: E402
from hover_geometry import WindowGeometry  # noqa: E402
from hover_oracle import analyze_hover  # noqa: E402
from hover_probe import (  # noqa: E402
    measure_cursor_in_screenshot,
    probe_hover,
    render_probe_table,
)


ORIGIN = WindowGeometry(200, 50, 400, 300)
# Screen frame (260, 110, 160, 120) -> screenshot rect (60, 60, 160, 120).
TARGET_FRAME = {"x": 260, "y": 110, "w": 160, "h": 120}


class ProbeTransport:
    """Writes a screenshot whose hovered region changes only when told to."""

    def __init__(self, *, changes_for=("x11-warp",)):
        self.changes_for = set(changes_for)
        self.calls = []
        self.hovered = None

    def call(self, tool, payload):
        self.calls.append((tool, dict(payload)))
        if tool == "move_cursor":
            # Only a pointer actually inside the button counts as hovering.
            inside = (
                TARGET_FRAME["x"] <= payload.get("x", -1) <= TARGET_FRAME["x"] + TARGET_FRAME["w"]
                and TARGET_FRAME["y"] <= payload.get("y", -1) <= TARGET_FRAME["y"] + TARGET_FRAME["h"]
            )
            self.hovered = payload.get("probe_method") if inside else None
            return {"ok": True}
        if tool == "get_cursor_position":
            return {"x": 290, "y": 170}
        if tool == "get_window_state":
            output = payload.get("screenshot_out_file")
            if output:
                self._write(pathlib.Path(output))
            return {
                "screenshot_width": 400,
                "screenshot_height": 300,
                "structuredContent": {
                    "elements": [
                        {
                            "element_index": 0,
                            "label": "Reprise",
                            "role": "frame",
                            "depth": 0,
                            "parent_index": None,
                            "frame": {"x": 200, "y": 50, "w": 400, "h": 300},
                            "enabled": True,
                        },
                        {
                            "element_index": 7,
                            "label": "Add filter",
                            "role": "button",
                            "depth": 3,
                            "parent_index": 0,
                            "frame": {"x": 200, "y": 50, "w": 160, "h": 120},
                            "enabled": True,
                        },
                    ]
                },
            }
        return {"effect": "confirmed"}

    def _write(self, path):
        rows = [[(30, 30, 30)] * 400 for _ in range(300)]
        if self.hovered in self.changes_for:
            for y in range(60, 180):
                for x in range(60, 220):
                    rows[y][x] = (220, 220, 220)
        write_png(path, 400, 300, rows)

    def resize_window(self, *args):
        raise AssertionError("the probe must not resize")

    def set_connectivity(self, state):
        raise AssertionError("the probe must not touch connectivity")

    def wmctrl_geometry(self, window_id):
        raise AssertionError("the probe receives its origin")


def run(transport, **overrides):
    with tempfile.TemporaryDirectory() as directory:
        arguments = {
            "pid": 44,
            "window_id": 77,
            "session": "probe",
            "origin": ORIGIN,
            "label": "Add filter",
            "evidence_dir": pathlib.Path(directory),
            "x11_move": lambda x, y: transport.call(
                "move_cursor", {"probe_method": "x11-warp", "x": x, "y": y}
            ),
            "x11_cursor": lambda: (290.0, 170.0),
            "geometry_provider": lambda: [
                GeometryNode("frame", "Reprise", -5, -5, 400, 300),
                GeometryNode("push button", "Add filter", 55, 55, 160, 120),
            ],
        }
        arguments.update(overrides)
        return probe_hover(transport, **arguments)


class CursorVisibilityTests(unittest.TestCase):
    """Decide the cursor exclusion box by measurement, not by assumption."""

    def _measure(self, *, cursor_drawn):
        """Three shots: parked, moved away, parked again."""
        state = {"at": None}
        shots = []

        def move(x, y):
            state["at"] = (x, y)

        def snapshot(stem):
            path = pathlib.Path(self.root) / f"{stem}.png"
            rows = [[(30, 30, 30)] * 400 for _ in range(300)]
            if cursor_drawn and state["at"] == self.park:
                for y in range(2, 14):
                    for x in range(2, 14):
                        rows[y][x] = (240, 240, 240)
            write_png(path, 400, 300, rows)
            shots.append(stem)
            return path

        record = measure_cursor_in_screenshot(
            snapshot=snapshot, move=move, origin=ORIGIN
        )
        return record, shots

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = self._tmp.name
        self.park = (ORIGIN.x + 2, ORIGIN.y + 2)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_a_cursor_that_is_captured_is_detected(self) -> None:
        record, _shots = self._measure(cursor_drawn=True)

        self.assertTrue(record["cursor_in_screenshot"])

    def test_a_cursor_that_never_reaches_the_image_is_detected_too(self) -> None:
        record, _shots = self._measure(cursor_drawn=False)

        self.assertFalse(record["cursor_in_screenshot"])
        self.assertEqual(record["ratio_moved_away"], 0.0)

    def test_the_measurement_is_retained_for_the_evidence(self) -> None:
        record, shots = self._measure(cursor_drawn=True)

        self.assertEqual(len(shots), 3)
        self.assertEqual(record["probe_point"], [202.0, 52.0])
        self.assertIn("rect", record)
        self.assertEqual(record["method"], "park-away-park")

    def test_a_permanent_change_is_not_mistaken_for_a_cursor(self) -> None:
        # A region that differs in every shot is a moving UI, not the pointer:
        # the cursor must come back when the pointer comes back.
        counter = {"n": 0}

        def move(x, y):
            return None

        def snapshot(stem):
            counter["n"] += 1
            path = pathlib.Path(self.root) / f"{stem}.png"
            rows = [[(30 + counter["n"] * 40, 30, 30)] * 400 for _ in range(300)]
            write_png(path, 400, 300, rows)
            return path

        record = measure_cursor_in_screenshot(
            snapshot=snapshot, move=move, origin=ORIGIN
        )

        self.assertFalse(record["cursor_in_screenshot"])


class ConditionalExclusionTests(unittest.TestCase):
    """A small icon button must stay measurable when no cursor is drawn."""

    ORIGIN_ = ORIGIN
    SMALL = {"x": 260, "y": 110, "w": 36, "h": 34}

    def _analyze(self, exclude_cursor):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            before, after = root / "b.png", root / "a.png"
            rows = [[(30, 30, 30)] * 400 for _ in range(300)]
            write_png(before, 400, 300, rows)
            changed = [list(r) for r in rows]
            for y in range(60, 94):
                for x in range(60, 96):
                    changed[y][x] = (220, 220, 220)
            write_png(after, 400, 300, changed)
            return analyze_hover(
                before,
                after,
                {
                    "label": "Back to previous view",
                    "role": "button",
                    "enabled": True,
                    "visible": True,
                    "frame": self.SMALL,
                },
                origin=self.ORIGIN_,
                exclude_cursor=exclude_cursor,
            )

    def test_the_box_still_protects_when_a_cursor_is_drawn(self) -> None:
        codes = [item.code for item in self._analyze(True)]

        self.assertEqual(codes, ["hover-unmeasurable"])

    def test_without_a_drawn_cursor_the_small_button_is_judged(self) -> None:
        findings = self._analyze(False)

        self.assertEqual([item.code for item in findings], [])


class HoverProbeTests(unittest.TestCase):
    def test_both_dispatch_routes_are_measured(self) -> None:
        results = run(ProbeTransport())

        self.assertEqual([item.method for item in results], ["move_cursor", "x11-warp"])

    def test_the_requested_point_is_the_element_centre_in_screen_coordinates(
        self,
    ) -> None:
        results = run(ProbeTransport())

        for item in results:
            self.assertEqual(item.requested, (340.0, 170.0))

    def test_the_measured_region_is_the_translated_rectangle(self) -> None:
        # Only the x11 warp changes pixels here, and only inside the element.
        results = {item.method: item for item in run(ProbeTransport())}

        self.assertEqual(results["move_cursor"].changed_ratio, 0.0)
        self.assertGreater(results["x11-warp"].changed_ratio, 0.9)

    def test_a_driver_that_does_move_the_pointer_is_visible_too(self) -> None:
        results = {
            item.method: item
            for item in run(ProbeTransport(changes_for=("move_cursor", "x11-warp")))
        }

        self.assertGreater(results["move_cursor"].changed_ratio, 0.9)

    def test_both_cursor_readbacks_are_retained(self) -> None:
        results = run(ProbeTransport())

        for item in results:
            self.assertEqual(item.driver_cursor, (290.0, 170.0))
            self.assertEqual(item.x11_cursor, (290.0, 170.0))

    def test_a_missing_x11_tool_is_reported_rather_than_fatal(self) -> None:
        results = {
            item.method: item
            for item in run(ProbeTransport(), x11_move=None, x11_cursor=None)
        }

        self.assertIsNone(results["x11-warp"].changed_ratio)
        self.assertIn("xdotool", results["x11-warp"].note)

    def test_an_unknown_label_fails_loudly(self) -> None:
        with self.assertRaisesRegex(Exception, "Nope"):
            run(ProbeTransport(), label="Nope")

    def test_the_driver_and_measured_positions_are_reported_side_by_side(
        self,
    ) -> None:
        results = run(ProbeTransport())

        for item in results:
            # The driver puts every element on the window origin; the walk
            # puts this button where it really is.
            self.assertEqual(item.driver_frame, (200.0, 50.0, 160.0, 120.0))
            self.assertEqual(item.measured_frame, (260.0, 110.0, 160.0, 120.0))

    def test_the_table_shows_both_positions(self) -> None:
        text = render_probe_table(run(ProbeTransport()))

        for phrase in ("driver_frame", "measured_frame"):
            self.assertIn(phrase, text)

    def test_the_table_names_the_verdict_inputs(self) -> None:
        text = render_probe_table(run(ProbeTransport()))

        for phrase in ("move_cursor", "x11-warp", "changed_ratio", "x11_cursor"):
            self.assertIn(phrase, text)


if __name__ == "__main__":
    unittest.main(verbosity=1)
