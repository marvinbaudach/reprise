#!/usr/bin/env python3
"""Count how often each declared oracle was asked, not just what it found.

A `summary.json` used to list only `finding_codes` and `finding_counts` - what
was found. An oracle that never came up for evaluation therefore looked
exactly like a clean product. This counts the asking too, so a silent oracle
can be told apart from a satisfied one, and records when one is silent for a
named reason (`superseded_by`) rather than by accident.
"""

from __future__ import annotations

from collections import Counter
from typing import Any, Mapping


ORACLE_FINDING_CODES = {
    "feedback": {"slow-visible-feedback"},
    "waiting-state": {"missing-waiting-feedback"},
    "layout-shift": {"uninvited-layout-shift"},
    "pointer-reachability": {"suspected-occlusion", "misrouted-click"},
    "scroll-direction": {"wrong-scroll-direction", "scroll-jump", "scroll-lost-selection"},
    "main-loop-stall": {"main-loop-stall"},
    "accessibility": {
        "degraded-accessibility",
        "invisible-actionable",
        "no-accessible-action",
        "suspected-no-handler",
    },
    "offline-continuity": {
        "offline-broke-local-music",
        "offline-lost-cached-content",
        "reconnect-kept-offline-status",
    },
    "hover-affordance": {
        "hover-skipped",
        "hover-unmeasurable",
        "hover-affordance-missing",
        "hover-affordance-weak",
    },
}


class OracleActivityTracker:
    """Counts whether each declared oracle reached an applicable observation."""

    def __init__(self, names: Sequence[str]) -> None:
        self.activity = {
            str(name): {"evaluated": 0, "fired": 0} for name in names
        }
        self.activation_dispatches: Counter[str] = Counter()

    def record(self, evidence: Any, findings: Sequence[Any]) -> None:
        codes = {str(item.code) for item in findings}
        if str(getattr(evidence, "kind", "")) == "activate":
            self.activation_dispatches[str(getattr(evidence, "dispatch", ""))] += 1
        for name, record in self.activity.items():
            if name == "clean-runtime" or not self._applies(name, evidence):
                continue
            record["evaluated"] += 1
            record["fired"] += len(codes & ORACLE_FINDING_CODES.get(name, set()))

    def record_clean_runtime(self, *, fired: bool) -> None:
        record = self.activity.get("clean-runtime")
        if record is not None:
            record["evaluated"] += 1
            record["fired"] += int(fired)

    def supersede(self, name: str, finding_code: str) -> None:
        if name in self.activity:
            self.activity[name]["superseded_by"] = finding_code

    def only_pointer_activations(self) -> bool:
        """True when the run activated, but never over the semantic route."""
        return set(self.activation_dispatches) == {"px"}

    @staticmethod
    def _applies(name: str, evidence: Any) -> bool:
        kind = str(getattr(evidence, "kind", ""))
        return {
            "feedback": kind == "activate",
            "waiting-state": kind == "wait",
            "layout-shift": kind == "wait",
            "pointer-reachability": kind == "activate" and evidence.dispatch == "px",
            "scroll-direction": kind == "scroll",
            "main-loop-stall": bool(kind),
            "accessibility": kind == "activate" and evidence.dispatch == "ax",
            "offline-continuity": kind == "set-connectivity",
            "hover-affordance": kind == "hover",
        }.get(name, False)
