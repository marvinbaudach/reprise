#!/usr/bin/env python3
"""Verify desktop-pointer dispatch before a mission spends its action budget."""

from __future__ import annotations

import subprocess
from typing import Any, Mapping

from driver_transport import DriverError, Transport
from pointer_dispatch import desktop_pointer_payload


HOVER_PREFLIGHT_TOLERANCE_PX = 3.0


def hover_preflight(
    transport: Transport,
    *,
    pid: int,
    window_id: int,
    session: str,
    origin: Any,
) -> dict[str, Any]:
    """Verify desktop-pointer dispatch before spending a mission action budget."""
    try:
        # Measured on cua-driver 0.19.3 under X11: a session-bound cursor read is
        # confined to window scope and answers desktop_escalation_required, while
        # the session-free form reads the real X11 pointer exactly. Escalating the
        # session is not an option: escalation is permanent and disables the
        # session-bound get_window_state route used by every mission snapshot.
        before = _desktop_cursor_position(transport.call("get_cursor_position", {}))
        candidates = (
            (origin.x + origin.width * 0.25, origin.y + origin.height * 0.25),
            (origin.x + origin.width * 0.75, origin.y + origin.height * 0.75),
        )
        target = max(candidates, key=lambda point: _distance_squared(before, point))
        transport.call("move_cursor", desktop_pointer_payload(*target))
        after = _desktop_cursor_position(transport.call("get_cursor_position", {}))
    except (DriverError, OSError, subprocess.SubprocessError) as error:
        raise DriverError("hover dispatch is unsafe on this driver build") from error
    moved = not _points_close(before, after)
    reached = _points_close(target, after)
    verified = moved and reached
    return {
        "before": {"source": "x11", "x": before[0], "y": before[1]},
        "target": {"x": target[0], "y": target[1]},
        "after": {"source": "x11", "x": after[0], "y": after[1]},
        "tolerance_px": HOVER_PREFLIGHT_TOLERANCE_PX,
        "moved_from_before": moved,
        "reached_target": reached,
        "verified": verified,
        "verdict": "pointer-reached-target" if verified else "pointer-did-not-reach-target",
    }


def _desktop_cursor_position(response: Mapping[str, Any]) -> tuple[float, float]:
    if response.get("source") != "x11":
        raise DriverError("session-free cursor read did not report the X11 pointer")
    x, y = response.get("x"), response.get("y")
    if not all(isinstance(value, (int, float)) and not isinstance(value, bool) for value in (x, y)):
        raise DriverError("session-free cursor read has no numeric position")
    return float(x), float(y)


def _distance_squared(left: tuple[float, float], right: tuple[float, float]) -> float:
    return (left[0] - right[0]) ** 2 + (left[1] - right[1]) ** 2


def _points_close(left: tuple[float, float], right: tuple[float, float]) -> bool:
    return all(
        abs(left_value - right_value) <= HOVER_PREFLIGHT_TOLERANCE_PX
        for left_value, right_value in zip(left, right)
    )
