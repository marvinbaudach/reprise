#!/usr/bin/env python3
"""Conservative process CPU and host-load samples for response gaps."""

from __future__ import annotations

import os
import pathlib
from typing import Any


class ProcessActivityProbe:
    """Measure process CPU ticks around one driver response window."""

    def __init__(self, pid: int) -> None:
        self.pid = pid
        try:
            self.clock_ticks = int(os.sysconf("SC_CLK_TCK"))
        except (OSError, TypeError, ValueError):
            self.clock_ticks = 0

    def start(self) -> int | None:
        return self._cpu_ticks()

    def finish(self, started: int | None, *, gap_ms: int) -> dict[str, Any]:
        finished = self._cpu_ticks()
        app_cpu_ms = None
        if (
            started is not None
            and finished is not None
            and finished >= started
            and self.clock_ticks > 0
        ):
            app_cpu_ms = round((finished - started) * 1000 / self.clock_ticks)
        try:
            host_load_1m = os.getloadavg()[0]
        except (AttributeError, OSError):
            host_load_1m = None
        return {
            "gap_ms": gap_ms,
            "app_cpu_ms": app_cpu_ms,
            "host_load_1m": host_load_1m,
            "host_cpu_count": os.cpu_count(),
        }

    def _cpu_ticks(self) -> int | None:
        try:
            stat = pathlib.Path(f"/proc/{self.pid}/stat").read_text(encoding="utf-8")
            # comm is parenthesised and may itself contain spaces or `)`, so the
            # final close parenthesis is the only stable split point. The tail
            # starts at field 3; utime/stime (14/15) are therefore indices 11/12.
            fields = stat[stat.rfind(")") + 2 :].split()
            return int(fields[11]) + int(fields[12])
        except (IndexError, OSError, ValueError):
            return None
