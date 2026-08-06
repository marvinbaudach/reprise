#!/usr/bin/env python3
"""Compare animation-off and animation-on hover sweep summaries."""

from __future__ import annotations

import argparse
import json
import pathlib
from typing import Any, Mapping, Sequence


def _hover_entries(summary: Mapping[str, Any]) -> dict[tuple[str, str, str], set[str]]:
    entries: dict[tuple[str, str, str], set[str]] = {}
    for audit in summary.get("workload_audits", []):
        if not isinstance(audit, dict) or audit.get("kind") != "hover-sweep":
            continue
        for item in audit.get("hover_findings", []):
            if not isinstance(item, dict):
                continue
            key = (
                str(item.get("section", "")),
                str(item.get("label", "")),
                str(item.get("role", "")),
            )
            entries[key] = {str(code) for code in item.get("codes", [])}
    return entries


def compare_hover_summaries(
    animations_off: Mapping[str, Any], animations_on: Mapping[str, Any]
) -> list[dict[str, str]]:
    off = _hover_entries(animations_off)
    on = _hover_entries(animations_on)
    findings = []
    for section, label, role in sorted(off):
        if "hover-affordance-missing" not in off[(section, label, role)]:
            continue
        if "hover-affordance-missing" in on.get((section, label, role), set()):
            continue
        findings.append(
            {
                "code": "hover-animation-only",
                "section": section,
                "label": label,
                "role": role,
            }
        )
    return findings


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("animations_off", type=pathlib.Path)
    parser.add_argument("animations_on", type=pathlib.Path)
    args = parser.parse_args(argv)
    off = json.loads(args.animations_off.read_text(encoding="utf-8"))
    on = json.loads(args.animations_on.read_text(encoding="utf-8"))
    print(json.dumps(compare_hover_summaries(off, on), indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
