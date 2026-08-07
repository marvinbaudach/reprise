#!/usr/bin/env python3
"""The click probe: does the accessibility action do what a real click does?

Built on the recorded driver snapshot, because hand-written element shapes
disagreed with the driver three times running.
"""

from __future__ import annotations

import json
import pathlib
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
TEST_ROOT = REPO_ROOT / "scripts" / "tests"
FIXTURE = TEST_ROOT / "fixtures" / "hover-sweep-observe.json"
sys.path.insert(0, str(EXPLORE_ROOT))
sys.path.insert(0, str(TEST_ROOT))

from click_probe import (  # noqa: E402
    probe_click,
    render_click_table,
    star_counts,
    write_click_evidence,
)
from cua_explore_png import write_png  # noqa: E402
from hover_geometry import WindowGeometry  # noqa: E402


ORIGIN = WindowGeometry(200, 50, 1200, 800)
STAR_LABEL = "☆"
# The recorded target sits at screen (1366, 295), so its rect in screenshot
# coordinates ends at (1182, 269).
IMAGE_WIDTH = 1186
IMAGE_HEIGHT = 273


def recorded_elements():
    return json.loads(FIXTURE.read_text(encoding="utf-8"))["elements"]


class RecordedStarTransport:
    """Serves the recorded snapshot; a click may flip one empty star."""

    def __init__(self, *, flips=("px",), small_target=False, measured=True):
        self.flips = set(flips)
        self.small_target = small_target
        self.measured = measured
        self.elements = [dict(item) for item in recorded_elements()]
        self.calls = []
        self.pending_route = None
        for item in self.elements:
            if item.get("label") == STAR_LABEL:
                self.target = item
                break
        if self.small_target:
            self.target["frame"] = {**self.target["frame"], "w": 4.0, "h": 4.0}
        if not self.measured:
            self.target["geometry_trusted"] = False
            self.target["frame"] = {**self.target["frame"], "x": 200.0, "y": 50.0}

    def call(self, tool, payload):
        self.calls.append((tool, dict(payload)))
        if tool == "click":
            route = "ax" if "element_index" in payload else "px"
            if route in self.flips:
                self.target["label"] = "★"
            return {"effect": "confirmed", "verified": True}
        if tool == "get_window_state":
            output = payload.get("screenshot_out_file")
            if output:
                shade = 200 if self.target["label"] == "★" else 40
                # Just large enough to hold the target's translated rect;
                # a full 1200x800 capture makes the suite crawl.
                write_png(
                    pathlib.Path(output),
                    IMAGE_WIDTH,
                    IMAGE_HEIGHT,
                    [[(shade, shade, shade)] * IMAGE_WIDTH for _ in range(IMAGE_HEIGHT)],
                )
            return {
                "screenshot_width": IMAGE_WIDTH,
                "screenshot_height": IMAGE_HEIGHT,
                "elements": [dict(item) for item in self.elements],
            }
        return {"effect": "confirmed"}

    def resize_window(self, *args):
        raise AssertionError("the probe must not resize")

    def set_connectivity(self, state):
        raise AssertionError("the probe must not touch connectivity")

    def wmctrl_geometry(self, window_id):
        raise AssertionError("the probe receives its origin")


def run(transport, label=STAR_LABEL):
    with tempfile.TemporaryDirectory() as directory:
        return probe_click(
            transport,
            pid=44,
            window_id=77,
            session="probe",
            origin=ORIGIN,
            label=label,
            evidence_dir=pathlib.Path(directory),
        )


class StarCountTests(unittest.TestCase):
    def test_the_recorded_snapshot_counts_its_star_buttons(self) -> None:
        counts = star_counts(recorded_elements())

        self.assertEqual(counts["filled"], 27)
        self.assertEqual(counts["empty"], 23)

    def test_a_flipped_star_moves_one_from_empty_to_filled(self) -> None:
        elements = [dict(item) for item in recorded_elements()]
        for item in elements:
            if item.get("label") == "☆":
                item["label"] = "★"
                break

        counts = star_counts(elements)

        self.assertEqual(counts["filled"], 28)
        self.assertEqual(counts["empty"], 22)


class ClickProbeTests(unittest.TestCase):
    def test_both_routes_are_probed_accessibility_first(self) -> None:
        results = run(RecordedStarTransport())

        self.assertEqual([item.route for item in results], ["ax", "px"])

    def test_the_accessibility_route_addresses_the_element_index(self) -> None:
        transport = RecordedStarTransport()

        results = run(transport)

        self.assertIn("element_index", results[0].address)
        self.assertNotIn("x", results[0].address)

    def test_the_pixel_route_aims_at_the_measured_centre(self) -> None:
        transport = RecordedStarTransport()
        target = dict(transport.target)

        results = run(transport)

        expected = (
            target["frame"]["x"] + target["frame"]["w"] / 2,
            target["frame"]["y"] + target["frame"]["h"] / 2,
        )
        self.assertEqual((results[1].address["x"], results[1].address["y"]), expected)

    def test_a_route_that_works_only_on_pixels_is_visible_as_such(self) -> None:
        # The case that cannot be told apart today: the accessibility action is
        # not wired, but a real click rates the track.
        results = {item.route: item for item in run(RecordedStarTransport(flips=("px",)))}

        self.assertEqual(results["ax"].stars_before, results["ax"].stars_after)
        self.assertFalse(results["ax"].signature_changed)
        self.assertEqual(results["px"].stars_after["filled"], 28)
        self.assertTrue(results["px"].signature_changed)

    def test_a_control_that_answers_neither_route_shows_neither(self) -> None:
        results = run(RecordedStarTransport(flips=()))

        for item in results:
            self.assertEqual(item.stars_before, item.stars_after)
            self.assertFalse(item.signature_changed)

    def test_a_working_accessibility_action_is_visible_too(self) -> None:
        results = {item.route: item for item in run(RecordedStarTransport(flips=("ax",)))}

        self.assertTrue(results["ax"].signature_changed)
        self.assertEqual(results["ax"].stars_after["filled"], 28)

    def test_pixels_are_refused_when_the_position_was_never_measured(self) -> None:
        transport = RecordedStarTransport(measured=False)

        results = {item.route: item for item in run(transport)}

        self.assertFalse(results["px"].dispatched)
        self.assertIn("not measured", results["px"].note)
        clicks = [payload for tool, payload in transport.calls if tool == "click"]
        self.assertTrue(all("element_index" in payload for payload in clicks))

    def test_a_target_too_small_for_pixels_says_so(self) -> None:
        results = {item.route: item for item in run(RecordedStarTransport(small_target=True))}

        self.assertIn("too small", results["px"].note)

    def test_an_unknown_label_fails_loudly(self) -> None:
        with self.assertRaisesRegex(Exception, "Nope"):
            run(RecordedStarTransport(), label="Nope")

    def test_the_changed_pixels_are_reported(self) -> None:
        results = {item.route: item for item in run(RecordedStarTransport(flips=("px",)))}

        self.assertEqual(results["ax"].changed_ratio, 0.0)
        self.assertGreater(results["px"].changed_ratio, 0.9)

    def test_the_table_and_the_evidence_name_the_verdict_inputs(self) -> None:
        results = run(RecordedStarTransport())
        text = render_click_table(results)

        for phrase in ("route", "signature", "stars", "changed_ratio"):
            self.assertIn(phrase, text)
        with tempfile.TemporaryDirectory() as directory:
            path = write_click_evidence(results, pathlib.Path(directory))
            payload = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual([item["route"] for item in payload], ["ax", "px"])


if __name__ == "__main__":
    unittest.main(verbosity=1)
