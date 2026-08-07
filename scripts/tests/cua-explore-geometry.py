#!/usr/bin/env python3
"""The harness reads element geometry itself; the matching must refuse to guess."""

from __future__ import annotations

import pathlib
import sys
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
sys.path.insert(0, str(EXPLORE_ROOT))

from atspi_geometry import (  # noqa: E402
    GeometryError,
    GeometryNode,
    choose_frame,
    geometry_calibration,
    normalize_to_frame,
    resolve_driver_geometry,
)
from hover_geometry import WindowGeometry  # noqa: E402
from driver import CuaExecutor  # noqa: E402
from oracles import OracleEngine, normalize_snapshot  # noqa: E402


ORIGIN = WindowGeometry(200, 50, 1200, 800)


def node(role, label, x, y, w, h):
    return GeometryNode(role=role, label=label, x=x, y=y, width=w, height=h)


def driver(index, role, label, w, h, depth=1):
    """What cua-driver reports: real role/label/size, useless x and y."""
    return {
        "element_index": index,
        "role": role,
        "label": label,
        "depth": depth,
        "frame": {"x": 200, "y": 50, "w": w, "h": h},
    }


# Measured shape: the frame node sits at (-5, -5) in WINDOW coordinates - the
# offset of the WINDOW origin - but is the same 1200x800 rectangle that
# list_windows reports.
FRAME_NODE = node("frame", "Reprise", -5, -5, 1200, 800)
NODES = [
    FRAME_NODE,
    node("push button", "Shuffle", -5, 41, 34, 34),
    node("push button", "Main menu", 1105, 41, 34, 34),
    node("row", "Track 000001", 0, 120, 900, 28),
]
ELEMENTS = [
    driver(0, "frame", "Reprise", 1200, 800, depth=0),
    driver(3, "push button", "Shuffle", 34, 34),
    driver(5, "push button", "Main menu", 34, 34),
    driver(9, "row", "Track 000001", 900, 28),
]


class NormalisationTests(unittest.TestCase):
    def test_the_frame_node_becomes_the_window_origin(self) -> None:
        normalized = normalize_to_frame(NODES)

        self.assertEqual((normalized[0].x, normalized[0].y), (0.0, 0.0))

    def test_the_shadow_border_is_taken_off_every_child(self) -> None:
        # Shuffle reports WINDOW (-5, 41); relative to the frame node that is
        # (0, 46), which is what AT-SPI also reports as PARENT coordinates.
        normalized = normalize_to_frame(NODES)

        self.assertEqual((normalized[1].x, normalized[1].y), (0.0, 46.0))

    def test_sizes_are_never_touched(self) -> None:
        normalized = normalize_to_frame(NODES)

        self.assertEqual(
            [item.width for item in normalized], [1200.0, 34.0, 34.0, 900.0]
        )

    def test_a_walk_without_a_frame_node_is_refused(self) -> None:
        with self.assertRaisesRegex(GeometryError, "frame"):
            normalize_to_frame([node("push button", "Shuffle", 0, 0, 34, 34)])


class PerElementResolutionTests(unittest.TestCase):
    """The driver reports a filtered subset, so match element by element."""

    def test_two_different_buttons_get_two_different_points(self) -> None:
        result = resolve_driver_geometry(ELEMENTS, NODES, ORIGIN)

        self.assertEqual(result.frames[3], (200.0, 96.0, 34.0, 34.0))
        self.assertEqual(result.frames[5], (1310.0, 96.0, 34.0, 34.0))

    def test_the_window_origin_from_list_windows_is_added(self) -> None:
        result = resolve_driver_geometry(ELEMENTS, NODES, ORIGIN)

        # Row at WINDOW (0, 120); frame at (-5, -5) -> normalised (5, 125).
        self.assertEqual(result.frames[9], (205.0, 175.0, 900.0, 28.0))

    def test_a_walk_with_far_more_nodes_still_resolves(self) -> None:
        # Measured: driver 180 nodes, walk 485. The trees never match in size.
        extra = [node("label", f"filler {i}", 10, 10 + i, 5, 5) for i in range(40)]

        result = resolve_driver_geometry(ELEMENTS, [*NODES, *extra], ORIGIN)

        self.assertEqual(result.resolved, 4)
        self.assertEqual(result.unmatched, 0)

    def test_one_unmatched_element_does_not_cost_the_others_their_geometry(
        self,
    ) -> None:
        elements = [*ELEMENTS, driver(11, "push button", "Ghost", 20, 20)]

        result = resolve_driver_geometry(elements, NODES, ORIGIN)

        self.assertEqual(result.unmatched, 1)
        self.assertEqual(result.resolved, 4)
        self.assertNotIn(11, result.frames)
        self.assertIn(3, result.frames)

    def test_indistinguishable_twins_stay_unresolved(self) -> None:
        nodes = [
            FRAME_NODE,
            node("push button", "", 10, 10, 34, 34),
            node("push button", "", 90, 10, 34, 34),
        ]
        elements = [
            driver(0, "frame", "Reprise", 1200, 800, depth=0),
            driver(3, "push button", "", 34, 34),
        ]

        result = resolve_driver_geometry(elements, nodes, ORIGIN)

        self.assertEqual(result.ambiguous, 1)
        self.assertNotIn(3, result.frames)

    def test_virtualised_rows_are_counted_apart_from_real_misses(self) -> None:
        # The driver documents that it only reports a frame "when AT-SPI
        # reports usable bounds"; degenerate rows must not look like a miss.
        elements = [*ELEMENTS, driver(13, "row", "Track 000999", 900, 1)]

        result = resolve_driver_geometry(elements, NODES, ORIGIN)

        self.assertEqual(result.degenerate, 1)
        self.assertEqual(result.unmatched, 0)

    def test_a_node_outside_the_window_is_counted_and_dropped(self) -> None:
        nodes = list(NODES)
        nodes[3] = node("row", "Track 000001", 4000, 120, 900, 28)

        result = resolve_driver_geometry(ELEMENTS, nodes, ORIGIN)

        self.assertEqual(result.out_of_window, 1)
        self.assertNotIn(9, result.frames)
        self.assertEqual(result.resolved, 3)

    def test_a_role_alias_still_matches(self) -> None:
        nodes = list(NODES)
        nodes[3] = node("table row", "Track 000001", 0, 120, 900, 28)

        self.assertIn(9, resolve_driver_geometry(ELEMENTS, nodes, ORIGIN).frames)

    def test_the_record_carries_the_quota_for_the_evidence(self) -> None:
        record = resolve_driver_geometry(ELEMENTS, NODES, ORIGIN).as_record()

        self.assertEqual(record["driver_elements"], 4)
        self.assertEqual(record["resolved"], 4)
        self.assertEqual(record["resolved_ratio"], 1.0)
        for key in ("unmatched", "ambiguous", "out_of_window", "degenerate"):
            self.assertIn(key, record)


class UnresolvedDiagnosticsTests(unittest.TestCase):
    """Measure why an element stayed unresolved before changing the method."""

    def test_every_reason_lists_its_elements_with_the_key(self) -> None:
        elements = [
            *ELEMENTS,
            driver(11, "push button", "Ghost", 20, 20),
            driver(13, "row", "Track 000999", 900, 1),
        ]

        record = resolve_driver_geometry(elements, NODES, ORIGIN).as_record()

        ghost = record["unresolved"]["unmatched"][0]
        self.assertEqual(ghost["element_index"], 11)
        self.assertEqual(ghost["role"], "push button")
        self.assertEqual(ghost["label"], "Ghost")
        self.assertEqual([ghost["width"], ghost["height"]], [20, 20])
        self.assertEqual(ghost["candidates"], 0)
        self.assertEqual(
            record["unresolved"]["degenerate"][0]["element_index"], 13
        )

    def test_an_ambiguous_group_reports_how_many_candidates_the_walk_offers(
        self,
    ) -> None:
        nodes = [
            FRAME_NODE,
            node("row", "", 0, 10, 900, 28),
            node("row", "", 0, 40, 900, 28),
            node("row", "", 0, 70, 900, 28),
        ]
        elements = [
            driver(0, "frame", "Reprise", 1200, 800, depth=0),
            driver(7, "row", "", 900, 28),
        ]

        record = resolve_driver_geometry(elements, nodes, ORIGIN).as_record()

        entry = record["unresolved"]["ambiguous"][0]
        self.assertEqual(entry["element_index"], 7)
        self.assertEqual(entry["candidates"], 3)

    def test_the_list_is_capped_so_the_evidence_stays_readable(self) -> None:
        elements = [
            driver(0, "frame", "Reprise", 1200, 800, depth=0),
            *[driver(100 + i, "label", f"miss {i}", 40, 12) for i in range(60)],
        ]

        record = resolve_driver_geometry(elements, NODES, ORIGIN).as_record()

        self.assertEqual(record["unmatched"], 60)
        self.assertEqual(len(record["unresolved"]["unmatched"]), 40)


class OrderedGroupTests(unittest.TestCase):
    """Same key, same count on both sides: pair them in walk order."""

    NODES = [
        FRAME_NODE,
        node("row", "", 0, 10, 900, 28),
        node("row", "", 0, 40, 900, 28),
    ]
    ELEMENTS = [
        driver(0, "frame", "Reprise", 1200, 800, depth=0),
        driver(7, "row", "", 900, 28),
        driver(8, "row", "", 900, 28),
    ]

    def test_equal_counts_pair_in_order(self) -> None:
        result = resolve_driver_geometry(self.ELEMENTS, self.NODES, ORIGIN)

        self.assertEqual(result.frames[7], (205.0, 65.0, 900.0, 28.0))
        self.assertEqual(result.frames[8], (205.0, 95.0, 900.0, 28.0))

    def test_ordered_pairs_are_counted_apart_from_unique_matches(self) -> None:
        record = resolve_driver_geometry(self.ELEMENTS, self.NODES, ORIGIN).as_record()

        self.assertEqual(record["resolved_unique"], 1)
        self.assertEqual(record["resolved_ordered"], 2)
        self.assertEqual(record["resolved"], 3)

    def test_unequal_counts_leave_the_whole_group_unresolved(self) -> None:
        nodes = [*self.NODES, node("row", "", 0, 70, 900, 28)]

        result = resolve_driver_geometry(self.ELEMENTS, nodes, ORIGIN)

        self.assertEqual(result.ambiguous, 2)
        self.assertNotIn(7, result.frames)
        self.assertNotIn(8, result.frames)

    def test_a_single_driver_element_against_several_nodes_stays_unresolved(
        self,
    ) -> None:
        result = resolve_driver_geometry(self.ELEMENTS[:2], self.NODES, ORIGIN)

        self.assertEqual(result.ambiguous, 1)
        self.assertNotIn(7, result.frames)


class RealTreeShapeTests(unittest.TestCase):
    """The measured shape: driver says 'window', Atspi says 'frame'."""

    DRIVER = [
        {
            "element_index": 0,
            "role": "window",
            "label": "Reprise",
            "depth": 0,
            "frame": {"x": 200, "y": 50, "w": 1200, "h": 800},
        },
        {
            "element_index": 1,
            "role": "panel",
            "label": "",
            "depth": 1,
            "frame": {"x": 200, "y": 50, "w": 1190, "h": 790},
        },
        {
            "element_index": 2,
            "role": "push button",
            "label": "Shuffle",
            "depth": 3,
            "frame": {"x": 200, "y": 50, "w": 34, "h": 34},
        },
        {
            "element_index": 3,
            "role": "push button",
            "label": "Main menu",
            "depth": 3,
            "frame": {"x": 200, "y": 50, "w": 34, "h": 34},
        },
    ]
    WALK = [
        node("frame", "Reprise", -5, -5, 1200, 800),
        node("panel", "", -5, -5, 1190, 790),
        node("push button", "Shuffle", -5, 41, 34, 34),
        node("push button", "Main menu", 1105, 41, 34, 34),
    ]

    def test_the_two_root_spellings_are_the_same_node(self) -> None:
        frames = resolve_driver_geometry(self.DRIVER, self.WALK, ORIGIN).frames

        self.assertEqual(frames[0], (200.0, 50.0, 1200.0, 800.0))

    def test_shuffle_and_main_menu_stop_sharing_a_point(self) -> None:
        frames = resolve_driver_geometry(self.DRIVER, self.WALK, ORIGIN).frames

        self.assertEqual(frames[2], (200.0, 96.0, 34.0, 34.0))
        self.assertEqual(frames[3], (1310.0, 96.0, 34.0, 34.0))
        self.assertNotEqual(frames[2][:2], frames[3][:2])


class CalibrationTests(unittest.TestCase):
    """The shadow normalisation must be a recorded measurement, not a belief."""

    def test_the_measurement_is_reported_for_the_evidence(self) -> None:
        record = geometry_calibration(NODES, ORIGIN)

        self.assertEqual(record["frame_window_rect"], [-5.0, -5.0, 1200.0, 800.0])
        self.assertEqual(record["window_rect"], [200.0, 50.0, 1200.0, 800.0])
        self.assertEqual(record["window_origin_offset"], [5.0, 5.0])
        self.assertTrue(record["size_matches_list_windows"])

    def test_a_frame_that_does_not_match_the_window_size_is_refused(self) -> None:
        # If the frame node and the list_windows entry are different
        # rectangles, anchoring one on the other is meaningless.
        nodes = list(NODES)
        nodes[0] = node("frame", "Reprise", -5, -5, 640, 480)

        with self.assertRaisesRegex(GeometryError, "frame size"):
            resolve_driver_geometry(
                [driver(0, "frame", "Reprise", 640, 480, depth=0), *ELEMENTS[1:]],
                nodes,
                ORIGIN,
            )

    def test_a_frame_of_the_window_size_calibrates_to_no_shadow(self) -> None:
        nodes = [
            node("frame", "Reprise", 0, 0, 1200, 800),
            node("push button", "Shuffle", 0, 46, 34, 34),
        ]
        record = geometry_calibration(nodes, ORIGIN)

        self.assertEqual(record["window_origin_offset"], [0.0, 0.0])
        self.assertTrue(record["size_matches_list_windows"])


class RefusalDetailTests(unittest.TestCase):
    """The one remaining all-or-nothing refusal must stay diagnosable."""

    def test_the_anchor_refusal_names_both_node_counts(self) -> None:
        nodes = list(NODES)
        nodes[0] = node("frame", "Reprise", -5, -5, 640, 480)
        elements = [driver(0, "frame", "Reprise", 640, 480, depth=0), *ELEMENTS[1:]]

        with self.assertRaisesRegex(GeometryError, r"driver 4, walk 4"):
            resolve_driver_geometry(elements, nodes, ORIGIN)

    def test_a_size_mismatch_no_longer_costs_the_whole_snapshot(self) -> None:
        wrong = list(NODES)
        wrong[1] = node("push button", "Shuffle", -5, 41, 48, 48)

        result = resolve_driver_geometry(ELEMENTS, wrong, ORIGIN)

        self.assertEqual(result.unmatched, 1)
        self.assertEqual(result.resolved, 3)


class GeometryTrustTests(unittest.TestCase):
    """Position oracles must stay silent when the positions are not proven."""

    def setUp(self) -> None:
        self.engine = OracleEngine()

    def _snapshot(self, *, snapshot_trusted=True, element_trusted=True):
        return normalize_snapshot(
            {
                "geometry_trusted": snapshot_trusted,
                "structuredContent": {
                    "elements": [
                        {
                            "element_index": 0,
                            "role": "frame",
                            "label": "Reprise",
                            "depth": 0,
                            "parent_index": None,
                            "frame": {"x": 200, "y": 50, "w": 1200, "h": 800},
                            "enabled": True,
                        },
                        {
                            "element_index": 4,
                            "role": "push button",
                            "label": "Ghost",
                            "depth": 3,
                            "parent_index": 0,
                            # Far outside the window: only trusted geometry may
                            # turn this into a finding.
                            "frame": {"x": 9000, "y": 9000, "w": 34, "h": 34},
                            "enabled": True,
                            "geometry_trusted": element_trusted,
                        },
                    ]
                },
            },
            state_id="s",
            captured_ms=0,
        )

    def test_trusted_geometry_still_reports_an_element_outside_the_window(self) -> None:
        findings = self.engine.inspect_snapshot(self._snapshot())

        self.assertEqual(
            [item.code for item in findings], ["invisible-actionable"]
        )

    def test_an_untrusted_snapshot_produces_no_position_findings(self) -> None:
        findings = self.engine.inspect_snapshot(self._snapshot(snapshot_trusted=False))

        self.assertEqual([item.code for item in findings], [])

    def test_an_untrusted_element_is_skipped_on_its_own(self) -> None:
        findings = self.engine.inspect_snapshot(self._snapshot(element_trusted=False))

        self.assertEqual([item.code for item in findings], [])

    def test_the_default_stays_trusting_so_fixtures_keep_their_meaning(self) -> None:
        state = normalize_snapshot(
            {"structuredContent": {"elements": []}}, state_id="s", captured_ms=0
        )

        self.assertTrue(state.geometry_trusted)


class DriverGeometryTests(unittest.TestCase):
    """The measured geometry must actually reach the snapshot the oracles see."""

    def _executor(self, provider):
        transport = GeometryDriverTransport()
        return (
            transport,
            CuaExecutor(
                transport,
                pid=1,
                window_id=2,
                session="t",
                settle_delays=(),
                geometry_provider=provider,
                window_origin=ORIGIN,
            ),
        )

    def test_the_measured_positions_replace_the_placeholders(self) -> None:
        _transport, executor = self._executor(lambda: list(NODES))

        observation = executor.observe()

        by_label = {item["label"]: item["frame"] for item in observation["elements"]}
        self.assertEqual(by_label["Shuffle"]["x"], 200.0)
        self.assertEqual(by_label["Main menu"]["x"], 1310.0)

    def test_a_failing_walk_marks_the_snapshot_untrusted_and_is_recorded(self) -> None:
        def provider():
            raise GeometryError("the Atspi bindings are unavailable")

        _transport, executor = self._executor(provider)

        observation = executor.observe()

        self.assertFalse(observation["geometry_trusted"])
        self.assertEqual(
            executor.geometry_failures, ["the Atspi bindings are unavailable"]
        )

    def test_an_unresolved_element_loses_only_its_own_geometry(self) -> None:
        transport = GeometryDriverTransport(
            extra={
                "element_index": 11,
                "role": "push button",
                "label": "Ghost",
                "depth": 3,
                "frame": {"x": 200, "y": 50, "w": 20, "h": 20},
            }
        )
        executor = CuaExecutor(
            transport,
            pid=1,
            window_id=2,
            session="t",
            settle_delays=(),
            geometry_provider=lambda: list(NODES),
            window_origin=ORIGIN,
        )

        observation = executor.observe()

        by_label = {item["label"]: item for item in observation["elements"]}
        self.assertTrue(observation["geometry_trusted"])
        self.assertEqual(by_label["Shuffle"]["frame"]["x"], 200.0)
        self.assertFalse(by_label["Ghost"]["geometry_trusted"])
        self.assertTrue(by_label["Shuffle"]["geometry_trusted"])

    def test_the_resolution_quota_reaches_the_evidence(self) -> None:
        _transport, executor = self._executor(lambda: list(NODES))

        executor.observe()

        self.assertEqual(executor.geometry_resolution["resolved"], 4)
        self.assertEqual(executor.geometry_resolution["resolved_ratio"], 1.0)

    def test_the_calibration_measurement_is_retained_for_the_evidence(self) -> None:
        _transport, executor = self._executor(lambda: list(NODES))

        executor.observe()

        self.assertEqual(
            executor.geometry_calibration["window_origin_offset"], [5.0, 5.0]
        )
        self.assertTrue(executor.geometry_calibration["consistent"])

    def test_without_a_provider_the_snapshot_is_left_alone(self) -> None:
        transport = GeometryDriverTransport()
        executor = CuaExecutor(
            transport, pid=1, window_id=2, session="t", settle_delays=()
        )

        observation = executor.observe()

        self.assertTrue(observation["geometry_trusted"])


class GeometryDriverTransport:
    def __init__(self, extra=None):
        self.extra = extra

    def call(self, tool, payload):
        if tool == "get_window_state":
            elements = [dict(item) for item in ELEMENTS]
            if self.extra is not None:
                elements.append(dict(self.extra))
            return {"structuredContent": {"elements": elements}}
        return {"effect": "confirmed"}

    def resize_window(self, *args):
        return {}

    def set_connectivity(self, state):
        return {}

    def wmctrl_geometry(self, window_id):
        return None


if __name__ == "__main__":
    unittest.main(verbosity=1)
