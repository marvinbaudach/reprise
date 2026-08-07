#!/usr/bin/env python3
"""Resolve CUA window geometry and convert window-relative hover points."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Iterator, Mapping, Protocol

from driver import DriverError
from hover_oracle import HOVER_PARK_MARGIN_PX


@dataclass(frozen=True)
class WindowGeometry:
    x: float
    y: float
    width: float
    height: float


class GeometryTransport(Protocol):
    def call(self, tool: str, payload: Mapping[str, Any]) -> Mapping[str, Any]: ...
    def wmctrl_geometry(self, window_id: int) -> WindowGeometry: ...


def _objects(value: Any) -> Iterator[Mapping[str, Any]]:
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _objects(child)
    elif isinstance(value, list):
        for child in value:
            yield from _objects(child)


def _number(value: Any) -> float | None:
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return float(value)
    return None


def _geometry(record: Mapping[str, Any]) -> WindowGeometry | None:
    nested = record.get("frame", record.get("geometry", {}))
    sources = [record]
    if isinstance(nested, dict):
        sources.insert(0, nested)
    for source in sources:
        x = _number(source.get("x"))
        y = _number(source.get("y"))
        width = _number(source.get("w", source.get("width")))
        height = _number(source.get("h", source.get("height")))
        if None not in (x, y, width, height) and width > 0 and height > 0:
            return WindowGeometry(x, y, width, height)
    return None


def resolve_window_origin(
    transport: GeometryTransport, *, pid: int, window_id: int
) -> WindowGeometry:
    """Prefer driver window geometry and fail loudly if wmctrl cannot recover it."""
    response = transport.call("list_windows", {"pid": pid})
    for record in _objects(response):
        if record.get("window_id") != window_id:
            continue
        geometry = _geometry(record)
        if geometry is not None:
            return geometry
    try:
        geometry = transport.wmctrl_geometry(window_id)
    except (OSError, ValueError, DriverError) as error:
        raise DriverError("window origin is unavailable from cua-driver and wmctrl") from error
    if not isinstance(geometry, WindowGeometry):
        raise DriverError("window origin is unavailable from cua-driver and wmctrl")
    return geometry


def frame_values(frame: Mapping[str, Any]) -> tuple[float, float, float, float]:
    """The one reader of an element frame, in the driver's screen coordinates."""
    values = (
        _number(frame.get("x")),
        _number(frame.get("y")),
        _number(frame.get("w", frame.get("width"))),
        _number(frame.get("h", frame.get("height"))),
    )
    if None in values:
        raise DriverError("hover target has incomplete geometry")
    x, y, width, height = values
    if width <= 0 or height <= 0:
        raise DriverError("hover target has non-positive geometry")
    return x, y, width, height


def element_center(frame: Mapping[str, Any]) -> tuple[float, float]:
    """The pointer target, in screen coordinates.

    Element frames are already screen coordinates - the root element and a
    child at the window's top-left corner report the same origin - so adding
    the window origin here aimed the pointer one window offset away.
    """
    x, y, width, height = frame_values(frame)
    return x + width / 2, y + height / 2


def to_screenshot_point(
    point: tuple[float, float], origin: WindowGeometry
) -> tuple[float, float]:
    """A screen point expressed relative to the window screenshot."""
    return point[0] - origin.x, point[1] - origin.y


def to_screenshot_rect(
    frame: Mapping[str, Any], origin: WindowGeometry
) -> tuple[float, float, float, float]:
    """An element rectangle expressed relative to the window screenshot.

    The screenshot is the window crop anchored at (0, 0); the frame is in
    screen coordinates. This subtraction is the only bridge between the two.
    """
    x, y, width, height = frame_values(frame)
    left, top = to_screenshot_point((x, y), origin)
    return left, top, width, height


def park_point(geometry: WindowGeometry) -> tuple[float, float]:
    margin = float(HOVER_PARK_MARGIN_PX)
    return geometry.x + margin, geometry.y + margin
