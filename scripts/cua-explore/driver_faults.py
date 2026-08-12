#!/usr/bin/env python3
"""Bounded persistence for cua-driver transport failures."""

from __future__ import annotations

import json
import pathlib
from typing import Any, Mapping


# Per-field truncation alone does not bound the file: a driver that answers
# every read with garbage writes one record per call for a whole run. The
# 2026-08-10 night run retained a single fault across twelve runs, so 200 lines
# keep every realistic diagnosis while a broken driver cannot fill the
# evidence directory. The cap is not silent: the last line says that it was
# reached, and transport_faults keeps counting.
MAX_RETAINED_FAULT_LINES = 200


def append_fault(
    evidence_dir: pathlib.Path,
    retained_lines: int,
    *,
    tool: str,
    attempt: int,
    result: Any,
    response: Mapping[str, Any] | None = None,
) -> int:
    """Append one bounded record and return the new attempted line count."""

    retained_lines += 1
    if retained_lines > MAX_RETAINED_FAULT_LINES + 1:
        return retained_lines
    if retained_lines == MAX_RETAINED_FAULT_LINES + 1:
        record = {
            "truncated": True,
            "tool": tool,
            "retained": MAX_RETAINED_FAULT_LINES,
            "note": "further payloads are dropped; transport_faults keeps counting",
        }
    else:
        stdout = getattr(result, "stdout", None)
        if stdout is None:
            stdout = getattr(result, "output", None)
        record = {
            "tool": tool,
            "attempt": attempt,
            "returncode": getattr(result, "returncode", None),
            "stdout_head": _head(stdout),
            "stderr_head": _head(getattr(result, "stderr", None)),
        }
        if response is not None:
            # The parsed response is the complete semantic payload. Keeping it
            # intact is what makes structured refusals diagnosable even when
            # the process exits successfully.
            record["response"] = dict(response)
    evidence_dir.mkdir(parents=True, exist_ok=True)
    with (evidence_dir / "driver-faults.jsonl").open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(record, sort_keys=True) + "\n")
    return retained_lines


def _head(value: Any) -> str:
    if isinstance(value, bytes):
        value = value.decode("utf-8", errors="replace")
    return str(value or "")[:2000]
