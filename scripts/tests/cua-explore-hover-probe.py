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
from hover_probe import probe_hover, render_probe_table  # noqa: E402


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
