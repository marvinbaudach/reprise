#!/usr/bin/env python3
"""Aggregate exploratory CUA summaries without guessing from runner logs."""

from __future__ import annotations

import argparse
import json
import pathlib
from collections import Counter
from dataclasses import dataclass
from typing import Any, Mapping, Sequence


@dataclass(frozen=True)
class RunRecord:
    path: pathlib.Path
    summary: Mapping[str, Any]
    findings: tuple[Mapping[str, Any], ...]


def _integer(value: Any, default: int = 0) -> int:
    return value if isinstance(value, int) and not isinstance(value, bool) else default


def _unknown_action_count(value: Any) -> int:
    if isinstance(value, Mapping):
        return sum(max(0, _integer(count, 1)) for count in value.values())
    if isinstance(value, (list, tuple, set, frozenset)):
        return len(value)
    return 0


def _oracle_totals(value: Any) -> tuple[int, int, int]:
    if not isinstance(value, Mapping):
        return 0, 0, 0
    entries = [item for item in value.values() if isinstance(item, Mapping)]
    return (
        sum(_integer(item.get("evaluated")) for item in entries),
        sum(_integer(item.get("fired")) for item in entries),
        len(entries),
    )


def health_line(summary: Mapping[str, Any]) -> str:
    """Return the compact per-run health record used in the night report."""
    mission = str(summary.get("mission_id", "unknown"))
    seed = summary.get("seed", "?")
    outcome = str(summary.get("outcome", "unknown"))
    parts = [f"{mission}[seed={seed}]", f"outcome={outcome}"]
    abort_reason = summary.get("abort_reason")
    if abort_reason:
        parts.append(f"abort={abort_reason}")
    resolution = summary.get("geometry_resolution")
    if isinstance(resolution, Mapping):
        resolved = resolution.get("resolved", "?")
        # The denominator is this one field. Summing every integer in the
        # resolution record produced the fictitious 164/1129 night metric.
        driver_elements = resolution.get("driver_elements", "?")
        parts.append(f"geometry={resolved}/{driver_elements}")
    parts.append(f"transport_faults={_integer(summary.get('transport_faults'))}")
    parts.append(
        f"unknown_actions={_unknown_action_count(summary.get('unknown_action_names'))}"
    )
    evaluated, fired, declared = _oracle_totals(summary.get("oracle_activity"))
    parts.append(f"oracles={evaluated} evaluated/{fired} fired/{declared} declared")
    return "; ".join(parts)


def _finding_target(finding: Mapping[str, Any]) -> str:
    evidence = finding.get("evidence")
    if isinstance(evidence, Mapping) and evidence.get("target") is not None:
        return str(evidence["target"])
    if finding.get("target") is not None:
        return str(finding["target"])
    return ""


def group_findings(runs: Sequence[RunRecord]) -> list[dict[str, Any]]:
    """Group by finding code and target, ordered by cross-run reproduction."""
    grouped: dict[tuple[str, str], dict[str, Any]] = {}
    for run in runs:
        mission = str(run.summary.get("mission_id", "unknown"))
        seed = run.summary.get("seed")
        seen_in_run: set[tuple[str, str]] = set()
        for finding in run.findings:
            key = str(finding.get("code", "unknown")), _finding_target(finding)
            group = grouped.setdefault(
                key,
                {
                    "code": key[0],
                    "target": key[1],
                    "run_paths": set(),
                    "mission_names": set(),
                    "seed_values": set(),
                    "occurrences": 0,
                    "severity": str(finding.get("severity", "unknown")),
                    "summary": str(finding.get("summary", "")),
                },
            )
            group["occurrences"] += 1
            if key in seen_in_run:
                continue
            seen_in_run.add(key)
            group["run_paths"].add(str(run.path))
            group["mission_names"].add(mission)
            group["seed_values"].add(seed)
    result = []
    for group in grouped.values():
        result.append(
            {
                "code": group["code"],
                "target": group["target"],
                "runs": len(group["run_paths"]),
                "missions": len(group["mission_names"]),
                "seeds": len(group["seed_values"]),
                "occurrences": group["occurrences"],
                "severity": group["severity"],
                "summary": group["summary"],
            }
        )
    return sorted(
        result,
        key=lambda item: (
            -item["runs"],
            -item["missions"],
            -item["seeds"],
            -item["occurrences"],
            item["code"],
            item["target"],
        ),
    )


def _trajectory_findings(path: pathlib.Path) -> list[Mapping[str, Any]]:
    if not path.is_file():
        return []
    findings = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            step = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError(f"{path}:{line_number}: invalid trajectory JSON") from error
        if not isinstance(step, Mapping):
            continue
        step_findings = step.get("findings")
        if isinstance(step_findings, list):
            findings.extend(item for item in step_findings if isinstance(item, Mapping))
    return findings


def load_run(summary_path: pathlib.Path) -> RunRecord:
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    if not isinstance(summary, Mapping):
        raise ValueError(f"{summary_path}: summary must be a JSON object")
    embedded = summary.get("findings")
    findings = (
        [item for item in embedded if isinstance(item, Mapping)]
        if isinstance(embedded, list)
        else _trajectory_findings(summary_path.with_name("trajectory.jsonl"))
    )
    return RunRecord(summary_path.parent, summary, tuple(findings))


def discover_runs(root: pathlib.Path) -> list[RunRecord]:
    paths = [root] if root.name == "summary.json" and root.is_file() else sorted(root.glob("*/summary.json"))
    return [load_run(path) for path in paths]


def render_report(runs: Sequence[RunRecord]) -> str:
    outcomes = Counter(str(run.summary.get("outcome", "unknown")) for run in runs)
    lines = ["# Exploratory CUA aggregate", "", f"Runs: {len(runs)}"]
    if outcomes:
        lines.append(
            "Outcomes: " + ", ".join(f"{name}={count}" for name, count in sorted(outcomes.items()))
        )
    lines.extend(["", "## Run health", ""])
    lines.extend(f"- {health_line(run.summary)}" for run in runs)
    lines.extend(["", "## Findings by reproducibility", ""])
    groups = group_findings(runs)
    if not groups:
        lines.append("No findings were retained.")
    for group in groups:
        target = f" target={group['target']!r}" if group["target"] else ""
        lines.append(
            f"- `{group['code']}`{target}: runs={group['runs']}, "
            f"missions={group['missions']}, seeds={group['seeds']}, "
            f"occurrences={group['occurrences']}"
        )
    return "\n".join(lines) + "\n"


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence_root", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args(argv)
    runs = discover_runs(args.evidence_root)
    if not runs:
        parser.error("no summary.json files found")
    report = render_report(runs)
    if args.output is None:
        print(report, end="")
    else:
        args.output.write_text(report, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
