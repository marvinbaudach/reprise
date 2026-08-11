#!/usr/bin/env python3
"""Classify long accessibility response gaps without discarding uncertainty."""

from __future__ import annotations

from typing import Any, Mapping, Sequence


STALL_EXCESS_MS = 250
# A response gap is attributed to app work only when it used at least 100 ms
# and one tenth of a core. Smaller deltas may be scheduler or background noise.
APP_BUSY_MIN_CPU_MS = 100
APP_BUSY_MIN_CPU_SHARE = 0.10
# One runnable task per logical CPU is an environment hint, not a product fault.
HOST_LOADED_PER_CPU = 1.0


def response_gap_findings(
    *,
    sample_gaps_ms: Sequence[int],
    snapshot_ms: Sequence[int],
    response_gaps: Sequence[Mapping[str, Any]],
) -> list[dict[str, Any]]:
    """Return finding records for every non-harness stall attribution."""

    baseline_ms = min(snapshot_ms) if snapshot_ms else 0
    stalled_samples = []
    for index, gap in enumerate(sample_gaps_ms):
        excess_ms = round(gap - baseline_ms)
        if excess_ms < STALL_EXCESS_MS:
            continue
        measured = (
            dict(response_gaps[index])
            if index < len(response_gaps)
            else {
                "gap_ms": gap,
                "app_cpu_ms": None,
                "host_load_1m": None,
                "host_cpu_count": None,
            }
        )
        measured.update({"gap_ms": gap, "excess_ms": excess_ms})
        stalled_samples.append(measured)

    groups: dict[str, list[dict[str, Any]]] = {
        "app-process-cpu": [],
        "loaded-host": [],
        "blocking-round-trip-suspected": [],
        "app-cpu-measurement-unavailable": [],
    }
    for sample in stalled_samples:
        if sample.get("harness_fault"):
            continue
        gap_ms = _number(sample.get("gap_ms"))
        app_threshold_ms = max(
            APP_BUSY_MIN_CPU_MS, gap_ms * APP_BUSY_MIN_CPU_SHARE
        )
        sample["app_cpu_threshold_ms"] = round(app_threshold_ms)
        cpu_count = sample.get("host_cpu_count")
        host_threshold = (
            cpu_count * HOST_LOADED_PER_CPU
            if isinstance(cpu_count, int)
            and not isinstance(cpu_count, bool)
            and cpu_count > 0
            else None
        )
        sample["host_load_threshold"] = host_threshold

        cpu_ms = sample.get("app_cpu_ms")
        if not _is_number(cpu_ms):
            groups["app-cpu-measurement-unavailable"].append(sample)
        elif cpu_ms >= app_threshold_ms:
            groups["app-process-cpu"].append(sample)
        elif (
            _is_number(sample.get("host_load_1m"))
            and host_threshold is not None
            and sample["host_load_1m"] >= host_threshold
        ):
            groups["loaded-host"].append(sample)
        else:
            groups["blocking-round-trip-suspected"].append(sample)

    shared = {
        "baseline_ms": round(baseline_ms),
        "gaps_ms": list(sample_gaps_ms),
    }
    specifications = {
        "app-process-cpu": (
            "main-loop-stall",
            "warning",
            0.8,
            "Observation sampling found long response gaps while the app process was computing.",
        ),
        "loaded-host": (
            "environment-load-hint",
            "info",
            0.7,
            "Long response gaps occurred while the app was CPU-idle and the host was loaded; this is not a product finding.",
        ),
        "blocking-round-trip-suspected": (
            "main-loop-stall",
            "warning",
            0.8,
            "Long response gaps remained unattributed because the app measured no meaningful CPU while the host was quiet; a blocking round trip is suspected.",
        ),
        "app-cpu-measurement-unavailable": (
            "main-loop-stall",
            "warning",
            0.7,
            "Long response gaps could not be attributed because the app CPU measurement was unavailable.",
        ),
    }
    findings = []
    for attribution, (code, severity, confidence, summary) in specifications.items():
        samples = groups[attribution]
        if samples:
            findings.append(
                {
                    "code": code,
                    "severity": severity,
                    "confidence": confidence,
                    "summary": summary,
                    "evidence": {
                        "gap_samples": samples,
                        **shared,
                        "attribution": attribution,
                    },
                }
            )
    return findings


def _is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _number(value: Any, default: float = 0.0) -> float:
    return float(value) if _is_number(value) else default
