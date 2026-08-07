#!/usr/bin/env python3
"""Standalone hover measuring rig: does a pointer move reach the real app?

This answers one question and nothing else: when the pointer is placed on a
known control, do the pixels of that control change? It measures the same move
twice - once through cua-driver's move_cursor, once through a real X11 warp -
so the two can be compared directly instead of argued about.
"""

from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
from dataclasses import dataclass, asdict
from typing import Any, Callable, Mapping, Sequence

from driver import DriverError
from hover_geometry import (
    WindowGeometry,
    element_center,
    frame_values,
    park_point,
    to_screenshot_point,
    to_screenshot_rect,
)
from atspi_geometry import GeometryError, resolve_driver_geometry
from hover_oracle import HOVER_CURSOR_EXCLUSION_PX, HOVER_MIN_CHANNEL_DELTA
from pngdiff import UnmeasurableImage, UnsupportedImage, read_rgb, rect_change_ratio


@dataclass(frozen=True)
class ProbeResult:
    method: str
    requested: tuple[float, float] | None
    driver_cursor: tuple[float, float] | None
    x11_cursor: tuple[float, float] | None
    changed_ratio: float | None
    changed_ratio_excluding_cursor: float | None
    rect: tuple[float, float, float, float] | None
    driver_frame: tuple[float, float, float, float] | None = None
    measured_frame: tuple[float, float, float, float] | None = None
    note: str = ""


def _point(value: Any) -> tuple[float, float] | None:
    if isinstance(value, Mapping):
        x, y = value.get("x"), value.get("y")
        if isinstance(x, (int, float)) and isinstance(y, (int, float)):
            return float(x), float(y)
    return None


def xdotool_move(x: float, y: float) -> None:
    subprocess.run(
        ["xdotool", "mousemove", "--sync", str(round(x)), str(round(y))],
        check=True,
        capture_output=True,
        timeout=10,
    )


def xdotool_cursor() -> tuple[float, float] | None:
    completed = subprocess.run(
        ["xdotool", "getmouselocation", "--shell"],
        check=False,
        capture_output=True,
        text=True,
        timeout=10,
    )
    if completed.returncode != 0:
        return None
    values: dict[str, float] = {}
    for line in completed.stdout.splitlines():
        key, _, raw = line.partition("=")
        try:
            values[key.strip().casefold()] = float(raw)
        except ValueError:
            continue
    if "x" in values and "y" in values:
        return values["x"], values["y"]
    return None


def default_x11_move() -> Callable[[float, float], None] | None:
    return xdotool_move if shutil.which("xdotool") else None


def default_x11_cursor() -> Callable[[], tuple[float, float] | None] | None:
    return xdotool_cursor if shutil.which("xdotool") else None


def _find_element(raw: Mapping[str, Any], label: str) -> Mapping[str, Any]:
    structured = raw.get("structuredContent")
    elements = (
        structured.get("elements", [])
        if isinstance(structured, dict)
        else raw.get("elements", [])
    )
    for item in elements if isinstance(elements, list) else []:
        if isinstance(item, dict) and item.get("label") == label:
            return item
    raise DriverError(f"hover probe target is not on screen: {label}")


def measure_cursor_in_screenshot(
    *,
    snapshot: Callable[[str], Any],
    move: Callable[[float, float], None],
    origin: WindowGeometry,
) -> dict[str, Any]:
    """Does the pointer appear in the window screenshot at all?

    Three shots at the park point - parked, pointer moved away, parked again.
    A drawn cursor disappears and comes back; a moving interface does not come
    back to the same pixels. The park point is the window corner plus a small
    margin, so nothing interactive should live there.

    The answer decides whether the hover oracle needs its cursor exclusion box.
    Under Xvfb the X11 cursor is not composited into the capture, and a blanket
    48 px box then blinds the very icon buttons the hover rule is about.
    """
    park = park_point(origin)
    away = (origin.x + origin.width / 2, origin.y + origin.height / 2)
    size = float(HOVER_CURSOR_EXCLUSION_PX)
    left, top = to_screenshot_point(park, origin)
    rect = (left - size / 2, top - size / 2, size, size)

    move(*park)
    parked = snapshot("cursor-probe-parked")
    move(*away)
    moved = snapshot("cursor-probe-away")
    move(*park)
    returned = snapshot("cursor-probe-returned")

    note = ""
    moved_ratio = returned_ratio = 0.0
    try:
        first = read_rgb(parked)
        second = read_rgb(moved)
        third = read_rgb(returned)
        moved_ratio = rect_change_ratio(
            first, second, rect, channel_delta=HOVER_MIN_CHANNEL_DELTA
        ).ratio
        returned_ratio = rect_change_ratio(
            first, third, rect, channel_delta=HOVER_MIN_CHANNEL_DELTA
        ).ratio
    except (OSError, UnsupportedImage, UnmeasurableImage) as error:
        note = f"cursor probe could not compare pixels: {error}"
    # The pointer left and came back: the region must change, then match again.
    visible = not note and moved_ratio > 0.0 and returned_ratio == 0.0
    return {
        "cursor_in_screenshot": visible,
        "probe_point": [float(park[0]), float(park[1])],
        "rect": [float(value) for value in rect],
        "ratio_moved_away": moved_ratio,
        "ratio_returned": returned_ratio,
        "method": "park-away-park",
        "note": note,
    }


def _rect(frame: Mapping[str, Any]) -> tuple[float, float, float, float] | None:
    try:
        x, y, width, height = frame_values(frame)
    except DriverError:
        return None
    return x, y, width, height


def _measured_frame(
    raw: Mapping[str, Any],
    element: Mapping[str, Any],
    origin: Any,
    geometry_provider: Callable[[], Any] | None,
) -> tuple[Mapping[str, Any], str]:
    """Prefer our own measurement, and say plainly when we fall back."""
    if geometry_provider is None:
        return element.get("frame") or {}, ""
    structured = raw.get("structuredContent")
    container = structured if isinstance(structured, dict) else raw
    elements = container.get("elements")
    if not isinstance(elements, list):
        return element.get("frame") or {}, "snapshot carries no element list"
    try:
        resolution = resolve_driver_geometry(elements, geometry_provider(), origin)
    except GeometryError as error:
        return element.get("frame") or {}, f"measured geometry refused: {error}"
    rect = resolution.frames.get(int(element.get("element_index", -1)))
    if rect is None:
        record = resolution.as_record()
        return (
            element.get("frame") or {},
            "this element has no unambiguous measured position "
            f"(resolved {record['resolved']}/{record['driver_elements']}, "
            f"unmatched {record['unmatched']}, ambiguous {record['ambiguous']})",
        )
    return {"x": rect[0], "y": rect[1], "w": rect[2], "h": rect[3]}, ""


def probe_hover(
    transport: Any,
    *,
    pid: int,
    window_id: int,
    session: str,
    origin: WindowGeometry,
    label: str,
    evidence_dir: pathlib.Path,
    x11_move: Callable[[float, float], None] | None = None,
    x11_cursor: Callable[[], tuple[float, float] | None] | None = None,
    geometry_provider: Callable[[], Any] | None = None,
) -> list[ProbeResult]:
    """Place the pointer on one control twice and measure the pixels each time."""
    evidence_dir.mkdir(parents=True, exist_ok=True)
    park = park_point(origin)

    def snapshot(stem: str) -> tuple[Mapping[str, Any], pathlib.Path]:
        path = evidence_dir / f"{stem}.png"
        raw = transport.call(
            "get_window_state",
            {
                "pid": pid,
                "window_id": window_id,
                "session": session,
                "screenshot_out_file": str(path),
            },
        )
        return raw, path

    def driver_move(x: float, y: float) -> None:
        transport.call(
            "move_cursor",
            {
                "pid": pid,
                "window_id": window_id,
                "session": session,
                "scope": "desktop",
                "probe_method": "move_cursor",
                "x": x,
                "y": y,
            },
        )

    routes: list[tuple[str, Callable[[float, float], None] | None]] = [
        ("move_cursor", driver_move),
        ("x11-warp", x11_move),
    ]
    results = []
    for method, move in routes:
        if move is None:
            results.append(
                ProbeResult(
                    method=method,
                    requested=None,
                    driver_cursor=None,
                    x11_cursor=None,
                    changed_ratio=None,
                    changed_ratio_excluding_cursor=None,
                    rect=None,
                    driver_frame=None,
                    measured_frame=None,
                    note="xdotool is not installed, so the control route was skipped",
                )
            )
            continue
        driver_move(*park)
        before_raw, before_path = snapshot(f"hover-probe-{method}-before")
        element = _find_element(before_raw, label)
        driver_frame = _rect(element.get("frame") or {})
        frame, measured_note = _measured_frame(
            before_raw, element, origin, geometry_provider
        )
        measured_frame = _rect(frame)
        target = element_center(frame)
        rect = to_screenshot_rect(frame, origin)
        move(*target)
        _after_raw, after_path = snapshot(f"hover-probe-{method}-after")
        cursor_size = float(HOVER_CURSOR_EXCLUSION_PX)
        pointer = to_screenshot_point(target, origin)
        cursor_box = (
            pointer[0] - cursor_size / 2,
            pointer[1] - cursor_size / 2,
            cursor_size,
            cursor_size,
        )
        note = measured_note
        ratio: float | None = None
        ratio_excluding: float | None = None
        try:
            before_image = read_rgb(before_path)
            after_image = read_rgb(after_path)
            ratio = rect_change_ratio(
                before_image, after_image, rect, channel_delta=HOVER_MIN_CHANNEL_DELTA
            ).ratio
            ratio_excluding = rect_change_ratio(
                before_image,
                after_image,
                rect,
                channel_delta=HOVER_MIN_CHANNEL_DELTA,
                exclude=cursor_box,
            ).ratio
        except (OSError, UnsupportedImage, UnmeasurableImage) as error:
            note = f"{note} pixel comparison failed: {error}".strip()
        results.append(
            ProbeResult(
                method=method,
                requested=target,
                driver_cursor=_point(
                    transport.call(
                        "get_cursor_position",
                        {"pid": pid, "window_id": window_id, "session": session},
                    )
                ),
                x11_cursor=x11_cursor() if x11_cursor is not None else None,
                changed_ratio=ratio,
                changed_ratio_excluding_cursor=ratio_excluding,
                rect=rect,
                driver_frame=driver_frame,
                measured_frame=measured_frame,
                note=note,
            )
        )
    driver_move(*park)
    return results


def render_probe_table(results: Sequence[ProbeResult]) -> str:
    lines = [
        "hover probe: does a pointer move reach the app?",
        "",
        f"{'method':<12} {'driver_frame':<24} {'measured_frame':<24} "
        f"{'requested':<16} {'driver_cursor':<16} {'x11_cursor':<16} "
        f"{'changed_ratio':<14} {'without cursor':<14}",
    ]
    for item in results:
        def show(value):
            if value is None:
                return "-"
            if isinstance(value, tuple) and len(value) == 4:
                return (
                    f"({value[0]:.0f}, {value[1]:.0f}, "
                    f"{value[2]:.0f}x{value[3]:.0f})"
                )
            if isinstance(value, tuple):
                return f"({value[0]:.0f}, {value[1]:.0f})"
            return f"{value:.4f}"

        lines.append(
            f"{item.method:<12} {show(item.driver_frame):<24} "
            f"{show(item.measured_frame):<24} {show(item.requested):<16} "
            f"{show(item.driver_cursor):<16} {show(item.x11_cursor):<16} "
            f"{show(item.changed_ratio):<14} "
            f"{show(item.changed_ratio_excluding_cursor):<14}"
        )
        if item.note:
            lines.append(f"  note: {item.note}")
    lines.extend(
        [
            "",
            "driver_frame is what cua-driver claims, measured_frame is what the",
            "accessibility walk says. If two differently sized controls share a",
            "driver_frame, that column is the placeholder, not a position.",
            "",
            "Read it like this: x11_cursor is the ground truth for where the",
            "pointer actually is. If move_cursor leaves x11_cursor at the park",
            "point, the driver never moved the real pointer. If both routes land",
            "on the target and only the x11 row changes pixels, move_cursor",
            "draws an overlay instead of hovering, and the hover path needs",
            "xdotool. If both rows change pixels, move_cursor is fine.",
        ]
    )
    return "\n".join(lines) + "\n"


def write_probe_evidence(
    results: Sequence[ProbeResult], evidence_dir: pathlib.Path
) -> pathlib.Path:
    path = evidence_dir / "hover-probe.json"
    path.write_text(
        json.dumps([asdict(item) for item in results], indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return path
