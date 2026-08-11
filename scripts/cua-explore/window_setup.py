#!/usr/bin/env python3
"""Apply and verify the window size a mission declares.

One concern only: resize once, measure what the window manager actually
granted, and hand back both numbers. It knows neither the runner nor the
report - the caller decides what a mismatch means. Launching the app and
waiting for its accessibility tree lives in `launch.py`.
"""

from __future__ import annotations

import subprocess
import time
from typing import Any, Mapping, Protocol


# Window managers round; a couple of pixels of drift is not a failed resize.
SIZE_TOLERANCE_PX = 2
# E3 runs this setup once per app generation, so a single transient wmctrl
# hiccup used to end a whole mission. Measuring is a read, so E7 allows the
# retry; the resize itself is a write and is never repeated.
MEASURE_RETRY_DELAYS_SECONDS = (0.25, 0.50)
# The transport is a Protocol here, so its own error type is out of reach:
# a failed wmctrl surfaces as DriverError (RuntimeError), a hung one as
# TimeoutExpired, and a missing binary as OSError.
TRANSPORT_ERRORS = (RuntimeError, OSError, subprocess.SubprocessError)


class WindowTransport(Protocol):
    def resize_window(
        self, window_id: int, width: int, height: int
    ) -> Mapping[str, Any]: ...

    def wmctrl_geometry(self, window_id: int) -> Any: ...


def _measure(transport: WindowTransport, window_id: int) -> tuple[Any | None, str | None]:
    """Read the granted geometry, retrying a transient transport failure."""
    error_text = None
    for attempt in range(len(MEASURE_RETRY_DELAYS_SECONDS) + 1):
        if attempt:
            time.sleep(MEASURE_RETRY_DELAYS_SECONDS[attempt - 1])
        try:
            return transport.wmctrl_geometry(window_id), None
        except TRANSPORT_ERRORS as error:
            error_text = f"{type(error).__name__}: {error}"
    return None, error_text


def _degraded(width: int, height: int, error_text: str) -> dict[str, Any]:
    """A setup we could not prove is a finding for the caller, not an abort."""
    return {
        "requested": {"width": width, "height": height},
        "achieved": None,
        "honoured": False,
        "error": error_text,
    }


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
    try:
        transport.resize_window(window_id, width, height)
    except TRANSPORT_ERRORS as error:
        # A resize is a write: repeating it would be a second visible change,
        # so a failed one degrades to a finding instead of a retry.
        return _degraded(width, height, f"resize failed: {type(error).__name__}: {error}")
    geometry, error_text = _measure(transport, window_id)
    if geometry is None:
        return _degraded(width, height, f"measurement failed: {error_text}")
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
