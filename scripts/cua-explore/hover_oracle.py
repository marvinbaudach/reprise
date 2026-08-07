#!/usr/bin/env python3
"""Classify visible pointer-hover feedback from retained screenshots."""

from __future__ import annotations

import pathlib
from typing import Any, Mapping

from oracles import Finding
from pngdiff import UnsupportedImage, UnmeasurableImage, read_rgb, rect_change_ratio
from ui_vocabulary import hover_strictness


HOVER_SETTLE_MS = 250
HOVER_MIN_CHANNEL_DELTA = 6
HOVER_MIN_CHANGED_RATIO = 0.02
HOVER_CURSOR_EXCLUSION_PX = 48
HOVER_PARK_MARGIN_PX = 2
HOVER_MIN_RECT_PX = 6


def _info(code: str, summary: str, evidence: Mapping[str, Any]) -> Finding:
    return Finding(code, "info", 1.0, summary, evidence)


def _intersection_area(
    left: tuple[float, float, float, float],
    right: tuple[float, float, float, float],
) -> float:
    left_x, left_y, left_width, left_height = left
    right_x, right_y, right_width, right_height = right
    width = max(
        0.0,
        min(left_x + left_width, right_x + right_width) - max(left_x, right_x),
    )
    height = max(
        0.0,
        min(left_y + left_height, right_y + right_height) - max(left_y, right_y),
    )
    return width * height


def analyze_hover(
    before_path: pathlib.Path | str,
    after_path: pathlib.Path | str,
    element: Mapping[str, Any],
    *,
    origin: Any,
    exclude_cursor: bool = True,
) -> tuple[Finding, ...]:
    """Return a finding only when hover is absent, weak, skipped, or unmeasurable."""
    label = str(element.get("label") or "")
    role = str(element.get("role") or "unknown")
    evidence = {"label": label, "role": role}
    if element.get("enabled") is False or element.get("visible") is False:
        return (
            _info(
                "hover-skipped",
                "The hover target is disabled or invisible.",
                evidence,
            ),
        )
    strictness = hover_strictness(role)
    if strictness == "skip":
        return (
            _info(
                "hover-skipped",
                "The element role has no hover acceptance contract.",
                evidence,
            ),
        )
    # Both the rectangle and the cursor box live in screenshot coordinates,
    # translated from the element's screen coordinates at exactly one place.
    from driver import DriverError
    from hover_geometry import element_center, to_screenshot_point, to_screenshot_rect

    try:
        rect = to_screenshot_rect(element.get("frame") or {}, origin)
        pointer = to_screenshot_point(element_center(element.get("frame") or {}), origin)
    except DriverError:
        return (
            _info(
                "hover-unmeasurable",
                "The hover target has no complete rectangle.",
                {**evidence, "reason": "missing-rectangle"},
            ),
        )
    x, y, width, height = rect
    if width < HOVER_MIN_RECT_PX or height < HOVER_MIN_RECT_PX:
        return (
            _info(
                "hover-unmeasurable",
                "The hover target is too small for a reliable pixel comparison.",
                {**evidence, "reason": "tiny-rectangle"},
            ),
        )
    cursor_size = float(HOVER_CURSOR_EXCLUSION_PX)
    cursor_box = (
        (
            pointer[0] - cursor_size / 2,
            pointer[1] - cursor_size / 2,
            cursor_size,
            cursor_size,
        )
        if exclude_cursor
        else None
    )
    # Without a cursor in the capture the box is pure loss: it blinds every
    # icon button smaller than 48 px, which is exactly what BTN-1 is about.
    if cursor_box is not None and _intersection_area(rect, cursor_box) > width * height * 0.5:
        return (
            _info(
                "hover-unmeasurable",
                "Cursor exclusion covers most of the hover target.",
                {**evidence, "reason": "cursor-exclusion"},
            ),
        )
    try:
        stats = rect_change_ratio(
            read_rgb(before_path),
            read_rgb(after_path),
            rect,
            channel_delta=HOVER_MIN_CHANNEL_DELTA,
            exclude=cursor_box,
        )
    except (OSError, UnsupportedImage, UnmeasurableImage) as error:
        return (
            _info(
                "hover-unmeasurable",
                "The retained screenshots cannot support this hover comparison.",
                {**evidence, "reason": str(error)},
            ),
        )
    if stats.ratio >= HOVER_MIN_CHANGED_RATIO:
        return ()
    code = "hover-affordance-missing" if strictness == "strict" else "hover-affordance-weak"
    severity = "error" if strictness == "strict" else "warning"
    return (
        Finding(
            code,
            severity,
            0.95,
            f"'{label or role}' showed no measurable hover state change.",
            {
                **evidence,
                "changed_pixels": stats.changed_pixels,
                "total_pixels": stats.total_pixels,
                "changed_ratio": stats.ratio,
                "max_channel_delta": stats.max_delta,
            },
        ),
    )
