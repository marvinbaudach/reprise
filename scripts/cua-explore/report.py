#!/usr/bin/env python3
"""Replay, confirmation, minimization, and privacy-safe UX reports."""

from __future__ import annotations

import json
import math
import pathlib
import re
from collections import Counter
from typing import Any, Callable, Mapping, Sequence


URL = re.compile(r"https?://\S+", re.IGNORECASE)


def _sanitize(value: Any) -> Any:
    home = str(pathlib.Path.home())
    if isinstance(value, str):
        sanitized = value.replace(home, "<HOME>")
        return URL.sub("<URL>", sanitized)
    if isinstance(value, list):
        return [_sanitize(item) for item in value]
    if isinstance(value, tuple):
        return [_sanitize(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _sanitize(item) for key, item in value.items()}
    return value


def _finding_key(finding: Mapping[str, Any]) -> tuple[str, str]:
    evidence = _sanitize(finding.get("evidence", {}))
    return str(finding.get("code", "unknown")), json.dumps(evidence, sort_keys=True)


def confirm_findings(runs: Sequence[Sequence[Mapping[str, Any]]]) -> list[dict[str, Any]]:
    """Keep findings reproduced in at least two independent profile runs."""
    if len(runs) < 2:
        return []
    per_run = [
        {_finding_key(finding): dict(_sanitize(finding)) for finding in findings}
        for findings in runs
    ]
    counts = Counter(key for findings in per_run for key in findings)
    confirmed = []
    for key in sorted(key for key, count in counts.items() if count >= 2):
        finding = next(findings[key] for findings in per_run if key in findings)
        finding["confirmations"] = counts[key]
        confirmed.append(finding)
    return confirmed


def minimize_actions(
    actions: Sequence[Mapping[str, Any]],
    *,
    reproduces: Callable[[Sequence[Mapping[str, Any]]], bool],
    valid_sequence: Callable[[Sequence[Mapping[str, Any]]], bool] | None = None,
) -> list[Mapping[str, Any]]:
    """Classic delta reduction while preserving action-sequence validity."""
    valid = valid_sequence or (lambda _candidate: True)
    current = list(actions)
    if not current or not valid(current) or not reproduces(current):
        return current
    granularity = 2
    while len(current) >= 2:
        chunk_size = math.ceil(len(current) / granularity)
        reduced = False
        for start in range(0, len(current), chunk_size):
            candidate = current[:start] + current[start + chunk_size :]
            if candidate and valid(candidate) and reproduces(candidate):
                current = candidate
                granularity = max(2, granularity - 1)
                reduced = True
                break
        if reduced:
            continue
        if granularity >= len(current):
            break
        granularity = min(len(current), granularity * 2)
    return current


class RunReport:
    """Accumulates an append-only trajectory and writes its final summaries."""

    def __init__(
        self,
        output_dir: pathlib.Path,
        *,
        mission_id: str,
        profile: str,
        seed: int,
        commit: str,
        required_workloads: int = 0,
        required_audits: Sequence[int] = (),
    ) -> None:
        self.output_dir = output_dir
        self.mission_id = mission_id
        self.profile = profile
        self.seed = seed
        self.commit = commit
        self.required_workloads = required_workloads
        self.required_audits = tuple(required_audits)
        self.steps: list[dict[str, Any]] = []
        self.workload_audits: dict[int, dict[str, Any]] = {}
        self.startup_timings: list[dict[str, Any]] = []
        self.geometry_failures: list[str] = []
        self.geometry_calibration: dict[str, Any] | None = None
        self.geometry_resolution: dict[str, Any] | None = None
        self.cursor_visibility: dict[str, Any] | None = None
        self.hover_coverage: list[dict[str, Any]] = []
        self.output_dir.mkdir(parents=True, exist_ok=True)

    def set_geometry_failures(self, failures: Sequence[str]) -> None:
        """Snapshots whose element positions could not be proven; oracles stayed quiet."""
        self.geometry_failures = [str(_sanitize(item)) for item in failures]

    def set_geometry_calibration(self, calibration: Mapping[str, Any] | None) -> None:
        """The measured shadow border, so the normalisation stays checkable."""
        self.geometry_calibration = (
            dict(_sanitize(calibration)) if calibration else None
        )

    def set_hover_coverage(self, coverage: Sequence[Mapping[str, Any]] | None) -> None:
        """Per section: how many hover targets existed and how many were reached."""
        self.hover_coverage = [dict(_sanitize(item)) for item in coverage or ()]

    def set_cursor_visibility(self, measurement: Mapping[str, Any] | None) -> None:
        """Whether the pointer reaches the capture, and therefore needs excluding."""
        self.cursor_visibility = dict(_sanitize(measurement)) if measurement else None

    def set_geometry_resolution(self, resolution: Mapping[str, Any] | None) -> None:
        """How many driver elements got a measured position, and why the rest did not."""
        self.geometry_resolution = dict(_sanitize(resolution)) if resolution else None

    def set_startup_timings(self, timings: Sequence[Mapping[str, Any]]) -> None:
        """Measured launch cost per app start; a slow start is a product finding."""
        self.startup_timings = [dict(_sanitize(timing)) for timing in timings]

    def add_workload_audit(self, audit: Mapping[str, Any]) -> None:
        workload_index = int(audit.get("workload_index", -1))
        self.workload_audits[workload_index] = dict(_sanitize(audit))

    def add_step(
        self,
        *,
        action: Mapping[str, Any],
        before_state: str,
        after_state: str,
        findings: Sequence[Mapping[str, Any]],
    ) -> None:
        self.steps.append(
            _sanitize(
                {
                    "schema_version": 1,
                    "step": len(self.steps) + 1,
                    "action": action,
                    "before_state": before_state,
                    "after_state": after_state,
                    "findings": list(findings),
                }
            )
        )

    def write(self) -> Mapping[str, Any]:
        findings = [finding for step in self.steps for finding in step["findings"]]
        severity_counts = Counter(str(item.get("severity", "unknown")) for item in findings)
        code_counts = Counter(str(item.get("code", "unknown")) for item in findings)
        completed_workloads = sorted(
            {
                int(step["action"]["workload_index"])
                for step in self.steps
                if step["action"].get("kind") == "complete-workload"
            }
        )
        workload_audits = [
            self.workload_audits[key] for key in sorted(self.workload_audits)
        ]
        required_audits = set(self.required_audits)
        audits_complete = required_audits.issubset(self.workload_audits) and all(
            self.workload_audits[index].get("complete") is True
            for index in required_audits
        )
        finished = any(step["action"].get("kind") == "finish" for step in self.steps)
        mission_complete = (
            len(completed_workloads) == self.required_workloads
            and audits_complete
            and finished
        )
        summary = _sanitize(
            {
                "schema_version": 1,
                "mission_id": self.mission_id,
                "profile": self.profile,
                "seed": self.seed,
                "commit": self.commit,
                "steps": len(self.steps),
                "startup_timings": self.startup_timings,
                "geometry_failures": self.geometry_failures,
                "geometry_calibration": self.geometry_calibration,
                "geometry_resolution": self.geometry_resolution,
                "cursor_visibility": self.cursor_visibility,
                "hover_coverage": self.hover_coverage,
                "hover_candidates": sum(
                    int(item.get("candidates", 0)) for item in self.hover_coverage
                ),
                "hover_reached": sum(
                    int(item.get("hovered", 0)) for item in self.hover_coverage
                ),
                "geometry_trusted": not self.geometry_failures,
                "finding_counts": dict(sorted(severity_counts.items())),
                "finding_codes": dict(sorted(code_counts.items())),
                "required_workloads": self.required_workloads,
                "completed_workload_indices": completed_workloads,
                "workload_audits": workload_audits,
                "required_audits": sorted(required_audits),
                "finished": finished,
                "mission_complete": mission_complete,
                "automatic_gate": False,
                "requires_confirmation_runs": True,
            }
        )
        (self.output_dir / "summary.json").write_text(
            json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        with (self.output_dir / "trajectory.jsonl").open("w", encoding="utf-8") as output:
            for step in self.steps:
                output.write(json.dumps(step, separators=(",", ":"), sort_keys=True) + "\n")
        (self.output_dir / "report.md").write_text(
            self._markdown(summary, findings), encoding="utf-8"
        )
        return summary

    def _markdown(
        self, summary: Mapping[str, Any], findings: Sequence[Mapping[str, Any]]
    ) -> str:
        lines = [
            f"# Exploratory UX report: {self.mission_id}",
            "",
            f"- Profile: `{self.profile}`",
            f"- Seed: `{self.seed}`",
            f"- Commit: `{self.commit}`",
            f"- Actions: {summary['steps']}",
            "- Status: advisory until reproduced in two fresh profiles",
            "",
        ]
        if summary.get("hover_coverage"):
            lines.extend(["## Hover coverage", ""])
            lines.append(
                f"- Hovered {summary['hover_reached']} of "
                f"{summary['hover_candidates']} eligible targets"
            )
            for item in summary["hover_coverage"]:
                lines.append(
                    f"    - `{item.get('section')}`: {item.get('hovered')} of "
                    f"{item.get('candidates')} "
                    f"(cap {item.get('limit_per_section')}, "
                    f"{item.get('skipped_budget')} left to budget, "
                    f"{item.get('skipped_without_geometry')} without geometry)"
                )
            lines.append("")
        resolution = summary.get("geometry_resolution")
        if resolution:
            lines.extend(["## Geometry", ""])
            lines.append(
                f"- Measured positions: {resolution.get('resolved')} of "
                f"{resolution.get('driver_elements')} driver elements "
                f"({resolution.get('resolved_ratio')})"
            )
            lines.append(
                f"  ({resolution.get('resolved_unique')} on a unique key, "
                f"{resolution.get('resolved_ordered')} paired in walk order "
                f"within an equally sized group)"
            )
            lines.append(
                f"- Unresolved: {resolution.get('unmatched')} without a match, "
                f"{resolution.get('ambiguous')} ambiguous, "
                f"{resolution.get('degenerate')} without usable bounds, "
                f"{resolution.get('out_of_window')} outside the window"
            )
            violations = resolution.get("subset_violations") or 0
            if violations:
                lines.append(
                    f"- **{violations} elements sit in groups where the driver "
                    f"reports more nodes than the walk can see.** Ordered "
                    f"pairing is refused there, and its subset argument is "
                    f"weakened for the "
                    f"{resolution.get('resolved_ordered')} elements it did "
                    f"resolve elsewhere - treat those with care."
                )
            else:
                lines.append(
                    f"- No group had more driver elements than walk nodes, so "
                    f"the subset argument behind the "
                    f"{resolution.get('resolved_ordered')} ordered pairings "
                    f"held everywhere it was checked."
                )
            unresolved = resolution.get("unresolved") or {}
            for reason in sorted(unresolved):
                entries = unresolved[reason]
                if not entries:
                    continue
                lines.append(f"- `{reason}` ({len(entries)} shown):")
                for entry in entries[:10]:
                    lines.append(
                        f"    - `{entry.get('role')}` "
                        f"\"{entry.get('label')}\" "
                        f"{entry.get('width')}x{entry.get('height')} "
                        f"- {entry.get('driver_count')} driver, "
                        f"{entry.get('candidates')} walk"
                    )
            lines.append("")
        if summary["geometry_failures"]:
            if not resolution:
                lines.extend(["## Geometry", ""])
            lines.append(
                "Element positions could not be proven, so the position oracles "
                "stayed silent for the affected snapshots:"
            )
            lines.append("")
            for failure in summary["geometry_failures"][:10]:
                lines.append(f"- {failure}")
            lines.append("")
        if summary["startup_timings"]:
            lines.append("## Startup")
            lines.append("")
            for timing in summary["startup_timings"]:
                lines.append(
                    f"- Launch {timing.get('launch')}: window after "
                    f"{timing.get('window_ms')} ms, usable accessibility tree after "
                    f"{timing.get('accessibility_tree_ms')} ms"
                )
            lines.append("")
        lines.extend(["## Findings", ""])
        if not findings:
            lines.append("No anomaly was observed within this mission and action budget.")
        for finding in findings:
            severity = str(finding.get("severity", "unknown")).upper()
            code = str(finding.get("code", "unknown"))
            confidence = finding.get("confidence")
            summary_text = str(finding.get("summary", ""))
            lines.extend(
                [
                    f"### {severity}: `{code}`",
                    "",
                    summary_text,
                    "",
                    f"Confidence: `{confidence}`",
                    "",
                    "```json",
                    json.dumps(_sanitize(finding.get("evidence", {})), indent=2, sort_keys=True),
                    "```",
                    "",
                ]
            )
        lines.extend(["", "## Workload completion", ""])
        if self.required_workloads == 0:
            lines.append("This mission has no enforced workload checkpoints.")
        else:
            lines.append(
                f"Completed checkpoints: {len(summary['completed_workload_indices'])} "
                f"of {self.required_workloads}."
            )
        for audit in summary["workload_audits"]:
            marker = "complete" if audit.get("complete") else "incomplete"
            lines.append(f"- `{audit.get('kind', 'unknown')}`: **{marker}**")
        return "\n".join(lines).rstrip() + "\n"
