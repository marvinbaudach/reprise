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


def desktop_point(
    frame: Mapping[str, Any], geometry: WindowGeometry
) -> tuple[float, float]:
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
    return geometry.x + x + width / 2, geometry.y + y + height / 2


def park_point(geometry: WindowGeometry) -> tuple[float, float]:
    margin = float(HOVER_PARK_MARGIN_PX)
    return geometry.x + margin, geometry.y + margin
