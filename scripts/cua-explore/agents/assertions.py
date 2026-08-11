"""Agent-owned cross-state assertions that complement runner oracles."""

from __future__ import annotations

from typing import Any, Mapping

from ui_vocabulary import ENTRY_ROLES, canonical_role
from workload_audit import label_shows_selection_count


def batch_selection_count(mission: Mapping[str, Any]) -> int | None:
    """How many rows this mission's batch workload selects, if it has one."""
    for workload in mission.get("workloads", []):
        if isinstance(workload, Mapping) and workload.get("kind") == "batch-edit":
            return int(workload.get("selection_count", 0))
    return None


def assertion_codes(
    action: Mapping[str, Any],
    observation: Mapping[str, Any],
    step_name: str | None = None,
    *,
    selection_count: int | None = None,
    section_changed: bool | None = None,
    known_token_values: Mapping[str, str] | None = None,
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
        target = action.get("target", {}).get("label")
        token = str(action.get("fixture_token", ""))
        known_values = dict(known_token_values or {})
        expected_value = known_values.get(token)
        # Escape does not reliably clear the entry (see agent-search-not-cleared),
        # so *some* non-empty value is no proof that this step typed it: the
        # previous source's value satisfies that test and invents a scope leak.
        stale_values = {
            value for name, value in known_values.items() if name != token and value
        }
        entry_values = [
            item.get("value")
            for item in observation.get("elements", [])
            if isinstance(item, dict)
            and item.get("label") == target
            and canonical_role(str(item.get("role", ""))) in ENTRY_ROLES
        ]
        entry_value_visible = any(
            isinstance(value, str)
            and bool(value)
            and (
                value == expected_value
                if expected_value
                else value not in stale_values
            )
            for value in entry_values
        )
        missing = []
        if section_changed is not True:
            missing.append("section-change")
        if not entry_value_visible:
            missing.append("typed-entry-value")
        if missing:
            results.append(
                (
                    f"agent-precondition-unmet:{step_name}",
                    "Section-search assertions were skipped because their observed preconditions were incomplete.",
                    {
                        "missing": missing,
                        "fixture_token": token,
                        "entry_values": [
                            value for value in entry_values if isinstance(value, str)
                        ][:4],
                    },
                )
            )
        elif len(rows) != 1:
            results.append(
                (
                    "agent-search-scope-leak",
                    "Section search did not expose exactly one result row.",
                    {"rows": rows[:20], "count": len(rows)},
                )
            )
    if (
        kind == "hotkey"
        and action.get("keys") == ["ctrl", "a"]
        and selection_count is not None
    ):
        if not any(
            label_shows_selection_count(label, selection_count) for label in labels
        ):
            results.append(
                (
                    "agent-missing-selection-count",
                    "The selected row count is not visible outside the tag dialog.",
                    {"selection_count": selection_count},
                )
            )
    return tuple(results)
