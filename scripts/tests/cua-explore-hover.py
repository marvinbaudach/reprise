#!/usr/bin/env python3
"""Display-free tests for hover dispatch, measurement, and workload contracts."""

from __future__ import annotations

import dataclasses
import json
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
from actions import HoverAction  # noqa: E402
from driver import CuaExecutor, DriverError, hover_preflight  # noqa: E402
from explorer import (  # noqa: E402
    CURRENT_VIEW_SECTION,
    DeterministicExplorer,
    MAX_HOVER_TARGETS_PER_SECTION,
)
from hover_geometry import (  # noqa: E402
    WindowGeometry,
    element_center,
    park_point,
    resolve_window_origin,
    to_screenshot_point,
    to_screenshot_rect,
)
from hover_compare import compare_hover_summaries  # noqa: E402
from hover_oracle import analyze_hover  # noqa: E402
from pngdiff import UnsupportedImage, UnmeasurableImage, read_rgb, rect_change_ratio  # noqa: E402
from protocol import ActionGateway, ContractError, load_mission  # noqa: E402
from runner import prepare_hover, write_gtk_animation_settings  # noqa: E402
from workload_audit import ActionTrace, audit_action_workload  # noqa: E402


class PngDiffTests(unittest.TestCase):
    def test_reads_an_eight_bit_rgb_png_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "rgb.png"
            write_png(path, 2, 1, [[(1, 2, 3), (250, 240, 230)]])

            image = read_rgb(path)

        self.assertEqual((image.width, image.height), (2, 1))
        self.assertEqual(image.pixels, ((1, 2, 3), (250, 240, 230)))

    def test_reads_an_eight_bit_rgba_png_and_ignores_alpha(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "rgba.png"
            write_png(path, 1, 1, [[(7, 8, 9, 0)]], color_type=6)

            image = read_rgb(path)

        self.assertEqual(image.pixels, ((7, 8, 9),))

    def test_rejects_sixteen_bit_images_as_unsupported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "sixteen.png"
            write_png(path, 1, 1, [[(1, 2, 3)]], bit_depth=16)

            with self.assertRaises(UnsupportedImage):
                read_rgb(path)

    def test_rejects_interlaced_images_as_unsupported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "interlaced.png"
            write_png(path, 1, 1, [[(1, 2, 3)]], interlace=1)

            with self.assertRaises(UnsupportedImage):
                read_rgb(path)

    def test_rect_change_ratio_ignores_pixels_below_the_channel_delta(self) -> None:
        before = self._image([[(10, 10, 10), (10, 10, 10)]])
        after = self._image([[(15, 10, 10), (16, 10, 10)]])

        stats = rect_change_ratio(
            before, after, (0, 0, 2, 1), channel_delta=6
        )

        self.assertEqual(stats.changed_pixels, 1)
        self.assertEqual(stats.total_pixels, 2)
        self.assertEqual(stats.ratio, 0.5)

    def test_rect_change_ratio_excludes_the_cursor_box(self) -> None:
        before = self._image([[(0, 0, 0)] * 4 for _ in range(4)])
        after = self._image([[(20, 20, 20)] * 4 for _ in range(4)])

        stats = rect_change_ratio(
            before,
            after,
            (0, 0, 4, 4),
            channel_delta=6,
            exclude=(1, 1, 2, 2),
        )

        self.assertEqual(stats.changed_pixels, 12)
        self.assertEqual(stats.total_pixels, 12)

    def test_rect_outside_the_image_is_reported_as_unmeasurable(self) -> None:
        image = self._image([[(0, 0, 0)]])

        with self.assertRaises(UnmeasurableImage):
            rect_change_ratio(image, image, (5, 5, 2, 2), channel_delta=6)

    def _image(self, pixels):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "image.png"
            write_png(path, len(pixels[0]), len(pixels), pixels)
            return read_rgb(path)


class HoverOracleTests(unittest.TestCase):
    def test_button_without_any_change_is_an_error_finding(self) -> None:
        findings = self._analyze("button", 27, 27)

        self.assertEqual(findings[0].code, "hover-affordance-missing")
        self.assertEqual(findings[0].severity, "error")
        self.assertFalse(findings[0].blocks_gate)

    def test_button_with_an_eight_percent_white_wash_is_accepted(self) -> None:
        self.assertEqual(self._analyze("button", 27, 45), ())

    def test_link_without_change_is_an_error_finding(self) -> None:
        findings = self._analyze("link", 27, 27)

        self.assertEqual(findings[0].code, "hover-affordance-missing")
        self.assertEqual(findings[0].severity, "error")

    def test_list_row_without_change_is_only_a_warning(self) -> None:
        findings = self._analyze("row", 27, 27)

        self.assertEqual(findings[0].code, "hover-affordance-weak")
        self.assertEqual(findings[0].severity, "warning")

    def test_disabled_or_invisible_element_is_skipped(self) -> None:
        for field in ({"enabled": False}, {"visible": False}):
            with self.subTest(field=field):
                findings = self._analyze("button", 27, 27, **field)
                self.assertEqual(findings[0].code, "hover-skipped")
                self.assertEqual(findings[0].severity, "info")

    def test_tiny_rect_is_unmeasurable_not_missing(self) -> None:
        findings = self._analyze("button", 27, 27, frame=(10, 10, 5, 5))

        self.assertEqual(findings[0].code, "hover-unmeasurable")

    def test_cursor_box_covering_most_of_the_rect_is_unmeasurable(self) -> None:
        findings = self._analyze("button", 27, 27, frame=(10, 10, 60, 20))

        self.assertEqual(findings[0].code, "hover-unmeasurable")

    def _analyze(
        self,
        role: str,
        before_value: int,
        after_value: int,
        *,
        frame=(10, 10, 100, 100),
        enabled: bool = True,
        visible: bool = True,
    ):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            before = root / "before.png"
            after = root / "after.png"
            write_png(before, 120, 120, [[(before_value,) * 3] * 120 for _ in range(120)])
            write_png(after, 120, 120, [[(after_value,) * 3] * 120 for _ in range(120)])
            x, y, width, height = frame
            return analyze_hover(
                before,
                after,
                {
                    "label": "Target",
                    "role": role,
                    "enabled": enabled,
                    "visible": visible,
                    "frame": {"x": x, "y": y, "width": width, "height": height},
                },
                origin=WindowGeometry(0, 0, 120, 120),
            )


class HoverMeasurementRegionTests(unittest.TestCase):
    """The compared pixels must be the element, not a region one offset away."""

    ORIGIN = WindowGeometry(200, 50, 400, 300)
    FRAME = {"x": 260, "y": 110, "width": 160, "height": 120}
    # Translated: (60, 60, 160, 120) inside the 400x300 window screenshot.

    def _pngs(self, root, changed_box):
        left, top, width, height = changed_box
        before = root / "before.png"
        after = root / "after.png"
        rows = [[(30, 30, 30)] * 400 for _ in range(300)]
        write_png(before, 400, 300, rows)
        changed = [list(row) for row in rows]
        for y in range(top, top + height):
            for x in range(left, left + width):
                changed[y][x] = (220, 220, 220)
        write_png(after, 400, 300, changed)
        return before, after

    def _analyze(self, changed_box):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            before, after = self._pngs(root, changed_box)
            return analyze_hover(
                before,
                after,
                {
                    "label": "Target",
                    "role": "button",
                    "enabled": True,
                    "visible": True,
                    "frame": self.FRAME,
                },
                origin=self.ORIGIN,
            )

    def test_a_change_inside_the_translated_rectangle_counts_as_hover(self) -> None:
        findings = self._analyze((60, 60, 160, 120))

        self.assertEqual([finding.code for finding in findings], [])

    def test_a_change_at_the_untranslated_position_does_not_count(self) -> None:
        # Exactly the bug: comparing screen coordinates against the window crop
        # measured a region one window origin away from the element.
        findings = self._analyze((260, 110, 140, 120))

        self.assertEqual(
            [finding.code for finding in findings], ["hover-affordance-missing"]
        )

    def test_a_rectangle_beyond_the_screenshot_is_only_unmeasurable_when_it_is(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            before, after = self._pngs(root, (60, 60, 160, 120))
            findings = analyze_hover(
                before,
                after,
                {
                    "label": "Right edge",
                    "role": "button",
                    "enabled": True,
                    "visible": True,
                    # Screen x 480 sits inside a window that starts at 200 and
                    # is 400 wide; only the naive reading pushes it off-image.
                    "frame": {"x": 480, "y": 110, "width": 100, "height": 100},
                },
                origin=self.ORIGIN,
            )

        self.assertNotIn(
            "hover-unmeasurable", {finding.code for finding in findings}
        )


class GeometryTransport:
    def __init__(self, response, fallback=None):
        self.response = response
        self.fallback = fallback
        self.calls = []

    def call(self, tool, payload):
        self.calls.append((tool, dict(payload)))
        return self.response

    def wmctrl_geometry(self, window_id):
        self.calls.append(("wmctrl_geometry", window_id))
        if isinstance(self.fallback, Exception):
            raise self.fallback
        return self.fallback


class HoverGeometryTests(unittest.TestCase):
    def test_window_origin_prefers_the_list_windows_record(self) -> None:
        transport = GeometryTransport(
            {
                "windows": [
                    {"window_id": 77, "x": 30, "y": 40, "width": 800, "height": 600}
                ]
            },
            fallback=AssertionError("fallback should not be used"),
        )

        geometry = resolve_window_origin(transport, pid=44, window_id=77)

        self.assertEqual(geometry, WindowGeometry(30, 40, 800, 600))
        self.assertNotIn("wmctrl_geometry", [call[0] for call in transport.calls])

    def test_window_origin_falls_back_to_wmctrl_geometry(self) -> None:
        expected = WindowGeometry(11, 22, 900, 700)
        transport = GeometryTransport({"windows": []}, fallback=expected)

        self.assertEqual(
            resolve_window_origin(transport, pid=44, window_id=77), expected
        )

    def test_window_origin_failure_is_a_driver_error_not_a_silent_zero(self) -> None:
        transport = GeometryTransport(
            {"windows": []}, fallback=DriverError("wmctrl did not find the window")
        )

        with self.assertRaises(DriverError):
            resolve_window_origin(transport, pid=44, window_id=77)

    def test_the_pointer_target_is_the_element_centre_in_screen_coordinates(
        self,
    ) -> None:
        # Element frames are already screen coordinates: the root element and a
        # child at the window's top-left corner report the same x/y. Adding the
        # window origin again put the pointer one window offset away.
        point = element_center({"x": 60, "y": 80, "width": 100, "height": 100})

        self.assertEqual(point, (110.0, 130.0))

    def test_the_comparison_rectangle_drops_the_window_origin(self) -> None:
        # The screenshot is the window crop, anchored at (0, 0).
        rect = to_screenshot_rect(
            {"x": 60, "y": 80, "width": 100, "height": 100},
            WindowGeometry(30, 40, 800, 600),
        )

        self.assertEqual(rect, (30.0, 40.0, 100.0, 100.0))

    def test_a_screen_point_translates_into_the_same_screenshot_space(self) -> None:
        point = to_screenshot_point((110.0, 130.0), WindowGeometry(30, 40, 800, 600))

        self.assertEqual(point, (80.0, 90.0))

    def test_an_incomplete_frame_is_a_driver_error_not_a_silent_zero(self) -> None:
        for frame in ({"x": 1, "y": 2, "width": 3}, {"x": 1, "y": 2, "w": 0, "h": 9}):
            with self.subTest(frame=frame):
                with self.assertRaises(DriverError):
                    element_center(frame)

    def test_park_point_sits_inside_the_window_but_outside_any_element(self) -> None:
        geometry = WindowGeometry(30, 40, 800, 600)

        self.assertEqual(park_point(geometry), (32.0, 42.0))


class HoverProtocolTests(unittest.TestCase):
    def setUp(self) -> None:
        self.observation = {
            "schema_version": 1,
            "state_id": "state-1",
            "actionable_labels": ["Target", "Delete Target"],
        }

    def test_hover_action_is_rejected_when_the_mission_lacks_the_capability(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "first-time-exploration.json"
        )

        with self.assertRaisesRegex(ContractError, "action kind is not allowed"):
            ActionGateway(mission).accept(self._action("Target"), self.observation)

    def test_hover_action_requires_an_actionable_target(self) -> None:
        gateway = ActionGateway(self._hover_mission())

        with self.assertRaisesRegex(ContractError, "not actionable"):
            gateway.accept(self._action("Missing"), self.observation)

    def test_hover_action_rejects_unknown_fields(self) -> None:
        gateway = ActionGateway(self._hover_mission())

        with self.assertRaisesRegex(ContractError, "unknown action field"):
            gateway.accept({**self._action("Target"), "dispatch": "px"}, self.observation)

    def test_hover_action_respects_the_forbidden_target_words(self) -> None:
        gateway = ActionGateway(self._hover_mission())

        with self.assertRaisesRegex(ContractError, "forbidden target"):
            gateway.accept(self._action("Delete Target"), self.observation)

    def _action(self, label: str):
        return {
            "schema_version": 1,
            "state_id": "state-1",
            "kind": "hover",
            "target": {"label": label},
        }

    def _hover_mission(self):
        raw = json.loads(
            (EXPLORE_ROOT / "missions" / "first-time-exploration.json").read_text()
        )
        raw["capabilities"].append("hover")
        raw["oracles"].append("hover-affordance")
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "mission.json"
            path.write_text(json.dumps(raw), encoding="utf-8")
            return load_mission(path)


class RecordingHoverTransport:
    def __init__(self, pixel_values=(27, 27, 27, 27), fail_tool=None):
        self.pixel_values = list(pixel_values)
        self.fail_tool = fail_tool
        self.calls = []

    def call(self, tool, payload):
        self.calls.append((tool, dict(payload)))
        if tool == self.fail_tool:
            raise DriverError("driver stopped")
        if tool == "get_window_state":
            output = payload.get("screenshot_out_file")
            if output:
                value = self.pixel_values.pop(0)
                write_png(
                    pathlib.Path(output),
                    140,
                    140,
                    [[(value, value, value)] * 140 for _ in range(140)],
                )
            return {
                "screenshot_width": 140,
                "screenshot_height": 140,
                "structuredContent": {
                    "elements": [
                        {
                            "element_index": 7,
                            "label": "Target",
                            "role": "button",
                            "actions": ["click"],
                            "enabled": True,
                            "visible": True,
                            "frame": {"x": 60, "y": 80, "w": 100, "h": 100},
                        }
                    ]
                },
            }
        if tool == "get_cursor_position":
            return {"x": 80, "y": 90}
        return {"effect": "confirmed", "verified": True}

    def resize_window(self, _window_id, _width, _height):
        return {"effect": "confirmed"}

    def set_connectivity(self, _state):
        return {"effect": "confirmed"}


class HoverDriverTests(unittest.TestCase):
    def test_execute_hover_disables_the_agent_cursor_once_per_executor(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = RecordingHoverTransport()
            executor = self._executor(transport, pathlib.Path(directory))

            executor.execute(HoverAction("state-1", "Target"))
            executor.execute(HoverAction("state-2", "Target"))

        disables = [call for call in transport.calls if call[0] == "set_agent_cursor_enabled"]
        self.assertEqual(len(disables), 1)
        self.assertFalse(disables[0][1]["enabled"])

    def test_execute_hover_parks_the_pointer_before_the_baseline_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = RecordingHoverTransport()
            self._executor(transport, pathlib.Path(directory)).execute(
                HoverAction("state-1", "Target")
            )

        tools = [tool for tool, _payload in transport.calls]
        first_park = tools.index("move_cursor")
        baseline = tools.index("get_window_state")
        self.assertLess(first_park, baseline)
        self.assertEqual(transport.calls[first_park][1]["scope"], "desktop")

    def test_execute_hover_returns_the_pointer_to_the_park_point(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = RecordingHoverTransport()
            self._executor(transport, pathlib.Path(directory)).execute(
                HoverAction("state-1", "Target")
            )

        moves = [payload for tool, payload in transport.calls if tool == "move_cursor"]
        self.assertEqual((moves[0]["x"], moves[0]["y"]), (32.0, 42.0))
        self.assertEqual(moves[-1], moves[0])

    def test_execute_hover_aims_at_the_element_centre_without_re_adding_the_origin(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = RecordingHoverTransport()
            self._executor(transport, pathlib.Path(directory)).execute(
                HoverAction("state-1", "Target")
            )

        moves = [payload for tool, payload in transport.calls if tool == "move_cursor"]
        # Window at (30, 40); element frame (60, 80, 100, 100) in screen
        # coordinates, so the centre is (110, 130) - not (140, 170).
        self.assertEqual((moves[1]["x"], moves[1]["y"]), (110.0, 130.0))

    def test_execute_hover_records_a_finding_when_nothing_changed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = RecordingHoverTransport()
            result = self._executor(transport, pathlib.Path(directory)).execute(
                HoverAction("state-1", "Target")
            )

        self.assertIn("hover-affordance-missing", {item.code for item in result.findings})

    def test_hover_preflight_fails_loudly_when_the_driver_stops_answering(self) -> None:
        transport = RecordingHoverTransport(fail_tool="get_cursor_position")

        with self.assertRaisesRegex(
            DriverError, "hover dispatch is unsafe on this driver build"
        ):
            hover_preflight(
                transport,
                pid=44,
                window_id=77,
                session="test",
                origin=WindowGeometry(30, 40, 800, 600),
            )

    def _executor(self, transport, evidence_dir):
        return CuaExecutor(
            transport,
            pid=44,
            window_id=77,
            session="test",
            evidence_dir=evidence_dir,
            settle_delays=(),
            hover_geometry=WindowGeometry(30, 40, 800, 600),
        )


class HoverRunnerTests(unittest.TestCase):
    def test_runner_writes_a_gtk_settings_file_when_animations_are_disabled(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)

            path = write_gtk_animation_settings(root, "off")

            self.assertEqual(
                path.read_text(encoding="utf-8"),
                "[Settings]\ngtk-enable-animations=0\n",
            )

    def test_runner_runs_the_hover_preflight_before_the_action_loop(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            transport = RecordingHoverTransport()

            geometry, cursor = prepare_hover(
                transport,
                pid=44,
                window_id=77,
                session="test",
                evidence_dir=pathlib.Path(directory),
                window={"width": 800, "height": 600},
                origin_override=(30, 40),
            )

            self.assertEqual(geometry, WindowGeometry(30, 40, 800, 600))
            # The cursor measurement runs once per launch, before any hover.
            self.assertIn("cursor_in_screenshot", cursor)
            # The cursor probe runs first; the preflight is the tail.
            self.assertEqual(
                [tool for tool, _payload in transport.calls[-3:]],
                ["move_cursor", "get_cursor_position", "get_window_state"],
            )
            self.assertTrue((pathlib.Path(directory) / "hover-preflight.json").is_file())


class HoverCompareTests(unittest.TestCase):
    def test_hover_compare_lists_elements_that_only_hover_with_animations(self) -> None:
        disabled = self._summary(["hover-affordance-missing"])
        enabled = self._summary([])

        findings = compare_hover_summaries(disabled, enabled)

        self.assertEqual(
            findings,
            [
                {
                    "code": "hover-animation-only",
                    "section": "Music",
                    "label": "Play",
                    "role": "button",
                }
            ],
        )

    def _summary(self, codes):
        return {
            "workload_audits": [
                {
                    "kind": "hover-sweep",
                    "hover_findings": [
                        {
                            "section": "Music",
                            "label": "Play",
                            "role": "button",
                            "codes": codes,
                        }
                    ],
                }
            ]
        }


class HoverSweepProtocolTests(unittest.TestCase):
    def test_hover_sweep_workload_rejects_an_empty_section_list(self) -> None:
        raw = self._mission_raw()
        raw["workloads"][0]["sections"] = []

        with self.assertRaisesRegex(ContractError, "sections must be a non-empty list"):
            self._load(raw)

    def test_hover_sweep_workload_rejects_unknown_fields(self) -> None:
        raw = self._mission_raw()
        raw["workloads"][0]["surprise"] = True

        with self.assertRaisesRegex(ContractError, "unknown hover-sweep workload field"):
            self._load(raw)

    def _mission_raw(self):
        return json.loads(
            (EXPLORE_ROOT / "missions" / "hover-affordance-sweep.json").read_text(
                encoding="utf-8"
            )
        )

    def _load(self, raw):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "mission.json"
            path.write_text(json.dumps(raw), encoding="utf-8")
            return load_mission(path)


class HoverSweepExplorerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.mission = load_mission(
            EXPLORE_ROOT / "missions" / "hover-affordance-sweep.json"
        )
        self.explorer = DeterministicExplorer(self.mission, 1)

    def test_hover_sweep_explorer_activates_every_section_before_hovering_it(self) -> None:
        actions = self._drive(
            [self._element("First"), self._element("Second", x=40)]
        )
        sections = self.mission.workloads[0]["sections"]

        for section in sections:
            activate = next(
                index
                for index, action in enumerate(actions)
                if action["kind"] == "activate"
                and action["target"]["label"] == section
            )
            hovers = [
                index
                for index, action in enumerate(actions)
                if action["kind"] == "hover"
                and action.get("section_for_test") == section
            ]
            self.assertTrue(hovers)
            self.assertLess(activate, min(hovers))

    def test_hover_sweep_explorer_hovers_only_actionable_enabled_visible_elements(self) -> None:
        actions = self._drive(
            [
                self._element("Good"),
                self._element("Disabled", enabled=False),
                self._element("Hidden", visible=False),
                self._element("Passive", actionable=False),
                self._element("Row", role="row"),
            ]
        )

        labels = {action["target"]["label"] for action in actions if action["kind"] == "hover"}
        # Rows answer softly; disabled, hidden and passive elements are skipped.
        self.assertEqual(labels, {"Good", "Row"})

    def test_hover_sweep_explorer_caps_targets_per_section(self) -> None:
        elements = [self._element(f"Button {index}", y=index) for index in range(40)]

        actions = self._drive(elements)

        limit = self.explorer.hover_budget_per_section(
            len(self.mission.workloads[0]["sections"]) + 1
        )
        self.assertLessEqual(limit, MAX_HOVER_TARGETS_PER_SECTION)
        for entry in self.explorer.hover_coverage:
            self.assertLessEqual(entry["hovered"], limit)
        self.assertEqual(self.explorer.hover_coverage[0]["hovered"], limit)

    def test_hover_sweep_explorer_accepts_the_drivers_own_role_spelling(self) -> None:
        # Measured: cua-driver reports "push button" where the mission lists
        # "button". Matching the mission list literally produced 0 hovers.
        actions = self._drive(
            [
                self._element("Shuffle", role="push button"),
                self._element("Play (Space)", role="push button", y=20),
            ]
        )

        labels = {
            action["target"]["label"] for action in actions if action["kind"] == "hover"
        }
        self.assertEqual(labels, {"Shuffle", "Play (Space)"})

    def test_hover_sweep_explorer_skips_elements_without_measured_geometry(self) -> None:
        actions = self._drive(
            [
                self._element("Solid", role="push button"),
                self._element("Placeholder", role="push button", y=20, geometry=False),
            ]
        )

        labels = {
            action["target"]["label"] for action in actions if action["kind"] == "hover"
        }
        self.assertEqual(labels, {"Solid"})

    def test_hover_sweep_explorer_records_what_it_covered_and_what_it_left(
        self,
    ) -> None:
        elements = [
            self._element(f"Button {index}", role="push button", y=index)
            for index in range(40)
        ]
        elements.append(
            self._element("Placeholder", role="push button", y=99, geometry=False)
        )

        self._drive(elements)

        first = self.explorer.hover_coverage[0]
        self.assertEqual(first["section"], CURRENT_VIEW_SECTION)
        limit = self.explorer.hover_budget_per_section(
            len(self.mission.workloads[0]["sections"]) + 1
        )
        self.assertEqual(first["candidates"], 40)
        self.assertEqual(first["hovered"], limit)
        self.assertEqual(first["skipped_budget"], 40 - limit)
        self.assertEqual(first["skipped_without_geometry"], 1)

    def test_a_small_budget_lowers_the_cap_instead_of_overrunning(self) -> None:
        mission = dataclasses.replace(
            self.mission,
            budgets=dataclasses.replace(self.mission.budgets, actions=30),
        )
        explorer = DeterministicExplorer(mission, 1)
        sections = len(mission.workloads[0]["sections"])

        self.assertLess(explorer.hover_budget_per_section(sections), 28)
        self.assertGreaterEqual(explorer.hover_budget_per_section(sections), 1)

    def test_the_coverage_reaches_the_evidence(self) -> None:
        from report import RunReport

        elements = [
            self._element(f"Button {index}", role="push button", y=index)
            for index in range(40)
        ]
        self._drive(elements)

        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory)
            report = RunReport(
                output,
                mission_id="hover-affordance-sweep",
                profile="mixed-sources-128",
                seed=1,
                commit="abc",
            )
            report.set_hover_coverage(self.explorer.hover_coverage)
            summary = report.write()

            # Entry view plus seven sections, each capped by the budget.
            limit = self.explorer.hover_budget_per_section(
                len(self.mission.workloads[0]["sections"]) + 1
            )
            self.assertEqual(summary["hover_candidates"], 40 * 8)
            self.assertEqual(summary["hover_reached"], limit * 8)
            self.assertIn(
                "Hover coverage", (output / "report.md").read_text(encoding="utf-8")
            )

    def _drive(self, elements):
        """Go through propose(), the way the runner does.

        Calling the private workload method directly used to hide the warm-up
        gate and let these tests pass while a real run never reached a hover.
        """
        sections = list(self.mission.workloads[0]["sections"])
        actions = []
        current_section = None
        observation = {
            "state_id": "s-0",
            "state_signature": "sig-0",
            "elements": elements,
            "actionable_labels": [item["label"] for item in elements] + sections,
        }
        for index in range(self.mission.budgets.actions):
            action = self.explorer.propose(observation)
            self.assertIsNotNone(action)
            if action["kind"] == "activate" and action["target"]["label"] in sections:
                current_section = action["target"]["label"]
            if action["kind"] == "hover":
                action = {**action, "section_for_test": current_section}
            actions.append(action)
            observation = {
                **observation,
                "state_id": f"s-{index + 1}",
                "state_signature": f"sig-{index + 1}",
            }
            if action["kind"] == "finish":
                break
        return actions

    def _element(
        self,
        label,
        *,
        role="button",
        x=10,
        y=10,
        enabled=True,
        visible=True,
        actionable=True,
        geometry=True,
    ):
        return {
            "label": label,
            "role": role,
            "enabled": enabled,
            "visible": visible,
            "actionable": actionable,
            "geometry_trusted": geometry,
            "frame": {"x": x, "y": y, "width": 20, "height": 20},
        }


class HoverSweepAuditTests(unittest.TestCase):
    def test_hover_sweep_audit_requires_one_measured_hover_per_section(self) -> None:
        workload = self._workload(minimum=1)
        traces = [
            self._activate("Music"),
            self._hover("Play", ("hover-unmeasurable",)),
        ]

        audit = audit_action_workload(0, workload, traces)

        self.assertFalse(audit["complete"])
        self.assertEqual(audit["hovered_per_section"]["Music"], 1)
        self.assertEqual(audit["measured_per_section"]["Music"], 0)

    def test_hover_sweep_audit_names_element_and_section_for_every_finding(self) -> None:
        workload = self._workload(minimum=1)
        traces = [
            self._activate("Music"),
            self._hover("Play", ("hover-affordance-missing",)),
        ]

        audit = audit_action_workload(0, workload, traces)

        self.assertTrue(audit["complete"])
        self.assertEqual(
            audit["hover_findings"],
            [
                {
                    "section": "Music",
                    "label": "Play",
                    "role": "button",
                    "codes": ["hover-affordance-missing"],
                }
            ],
        )

    def _workload(self, minimum):
        return {
            "kind": "hover-sweep",
            "sections": ["Music"],
            "min_targets_per_section": minimum,
            "roles": ["button"],
        }

    def _activate(self, section):
        return ActionTrace(
            action={"kind": "activate", "target_label": section},
            state_changed=True,
        )

    def _hover(self, label, codes):
        return ActionTrace(
            action={"kind": "hover", "target_label": label},
            after_roles=((label, "button"),),
            finding_codes=codes,
        )


class HoverSweepMissionTests(unittest.TestCase):
    def test_hover_sweep_mission_validates_and_is_listed_by_the_runner(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "hover-affordance-sweep.json"
        )

        self.assertEqual(mission.mission_id, "hover-affordance-sweep")
        self.assertEqual(mission.agent, "optional")


if __name__ == "__main__":
    unittest.main()
