"""Agent-owned cross-state assertions that complement runner oracles."""

from __future__ import annotations

from typing import Any, Mapping


def assertion_codes(
    action: Mapping[str, Any],
    observation: Mapping[str, Any],
    step_name: str | None = None,
) -> tuple[tuple[str, str, Mapping[str, Any]], ...]:
    kind = action.get("kind")
    labels = [
        str(item.get("label"))
        for item in observation.get("elements", [])
        if isinstance(item, dict) and item.get("label")
    ]
    rows = [
        str(item.get("label"))
        for item in observation.get("elements", [])
        if isinstance(item, dict)
        and item.get("label")
        and str(item.get("role", "")) == "row"
    ]
    results = []
    if (
        kind == "type"
        and action.get("target", {}).get("label") == "Search all fields"
        and step_name is not None
        and step_name.startswith("search-")
    ):
        if len(rows) != 1:
            results.append(
                (
                    "agent-search-scope-leak",
                    "Section search did not expose exactly one result row.",
                    {"rows": rows[:20], "count": len(rows)},
                )
            )
    if kind == "hotkey" and action.get("keys") == ["ctrl", "a"]:
        if not any("512" in label and "select" in label.casefold() for label in labels):
            results.append(
                (
                    "agent-missing-selection-count",
                    "The selected row count is not visible outside the tag dialog.",
                    {"selection_count": 512},
                )
            )
    return tuple(results)
