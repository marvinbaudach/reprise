#!/usr/bin/env python3
"""Apply and verify the window size a mission declares.

One concern only: resize once, measure what the window manager actually
granted, and hand back both numbers. It knows neither the runner nor the
report - the caller decides what a mismatch means. Launching the app and
waiting for its accessibility tree lives in `launch.py`.
"""

from __future__ import annotations

from typing import Any, Mapping, Protocol


# Window managers round; a couple of pixels of drift is not a failed resize.
SIZE_TOLERANCE_PX = 2


class WindowTransport(Protocol):
    def resize_window(
        self, window_id: int, width: int, height: int
    ) -> Mapping[str, Any]: ...

    def wmctrl_geometry(self, window_id: int) -> Any: ...


def apply_window_size(
    transport: WindowTransport,
    *,
    window_id: int,
    requested: Mapping[str, int] | None,
) -> dict[str, Any] | None:
    """Resize once, then retain the measured result without aborting on drift."""
    if requested is None:
        return None
    width = int(requested["width"])
    height = int(requested["height"])
    transport.resize_window(window_id, width, height)
    geometry = transport.wmctrl_geometry(window_id)
    achieved = {
        "width": int(geometry.width),
        "height": int(geometry.height),
    }
    return {
        "requested": {"width": width, "height": height},
        "achieved": achieved,
        "honoured": (
            abs(achieved["width"] - width) <= SIZE_TOLERANCE_PX
            and abs(achieved["height"] - height) <= SIZE_TOLERANCE_PX
        ),
    }
