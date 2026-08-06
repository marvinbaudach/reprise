#!/usr/bin/env python3
"""Independent checks that mission workloads were actually exercised."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping, Sequence


@dataclass(frozen=True)
class ActionTrace:
    action: Mapping[str, Any]
    before_labels: tuple[str, ...] = ()
    after_labels: tuple[str, ...] = ()
    before_rows: tuple[tuple[str, float], ...] = ()
    after_rows: tuple[tuple[str, float], ...] = ()
    before_selected_labels: tuple[str, ...] = ()
    after_selected_labels: tuple[str, ...] = ()
    after_actionable_labels: tuple[str, ...] = ()
    before_values: tuple[tuple[str, str], ...] = ()
    after_values: tuple[tuple[str, str], ...] = ()
    finding_codes: tuple[str, ...] = ()
    state_changed: bool = False
    after_busy: bool = False


def _folded(value: object) -> str:
    return str(value or "").casefold()


def _actions(traces: Sequence[ActionTrace], kind: str) -> list[Mapping[str, Any]]:
    return [trace.action for trace in traces if trace.action.get("kind") == kind]


def _ordered_contains(values: Sequence[str], expected: Sequence[str]) -> bool:
    cursor = iter(values)
    return all(any(item == wanted for item in cursor) for wanted in expected)


def _activate_labels(traces: Sequence[ActionTrace]) -> list[str]:
    return [
        _folded(action.get("target_label"))
        for action in _actions(traces, "activate")
    ]


def _row_labels(rows: Sequence[tuple[str, float]]) -> tuple[str, ...]:
    """Project row identity separately from geometry-only layout movement."""
    return tuple(label for label, _y in rows)


def _audit_sort(workload: Mapping[str, Any], traces: Sequence[ActionTrace]) -> dict[str, Any]:
    columns = tuple(_folded(item) for item in workload.get("columns", []))
    repetitions = int(workload.get("repetitions", 0))
    successful = [
        trace
        for trace in traces
        if trace.action.get("kind") == "activate"
        and any(
            column in _folded(trace.action.get("target_label"))
            for column in columns
        )
        and trace.state_changed
        and trace.before_rows != trace.after_rows
    ]
    covered = {
        column
        for column in columns
        if any(column in _folded(trace.action.get("target_label")) for trace in successful)
    }
    matching = len(successful)
    return {
        "complete": matching >= repetitions and covered == set(columns),
        "matching_actions": matching,
        "required_actions": repetitions,
        "covered_columns": sorted(covered),
    }


def _audit_filter(
    workload: Mapping[str, Any],
    traces: Sequence[ActionTrace],
    fixture_tokens: Mapping[str, str],
) -> dict[str, Any]:
    labels = _activate_labels(traces)
    route = tuple(_folded(item) for item in workload.get("route", []))
    facets = tuple(_folded(item) for item in workload.get("facets", []))
    route_complete = not route or _ordered_contains(labels, route)
    active_labels = {
        _folded(facet): _folded(label)
        for facet, label in workload.get("active_labels", {}).items()
    }
    facet_results: dict[str, bool] = {}
    facet_cursor = 0
    for facet in facets:
        expected = active_labels.get(facet, "")
        matching_index = None
        if expected:
            matching_index = next(
                (
                    index
                    for index in range(facet_cursor, len(traces))
                    if traces[index].action.get("kind") == "activate"
                    and traces[index].state_changed
                    and _row_labels(traces[index].before_rows)
                    != _row_labels(traces[index].after_rows)
                    and any(
                        expected in _folded(label)
                        for label in traces[index].after_labels
                    )
                    and not any(
                        expected in _folded(label)
                        for label in traces[index].before_labels
                    )
                ),
                None,
            )
        facet_results[facet] = matching_index is not None
        if matching_index is not None:
            facet_cursor = matching_index + 1
    facet_complete = (
        set(facet_results) == set(facets) and all(facet_results.values())
    )
    search_token = workload.get("search_token")
    expected_row = fixture_tokens.get(str(search_token), "")
    search_traces = [
        trace
        for trace in traces
        if trace.action.get("kind") == "type"
        and trace.action.get("target_label") == "Search all fields"
        and (search_token is None or trace.action.get("fixture_token") == search_token)
        and trace.state_changed
        and (
            not expected_row
            or (
                len(trace.after_rows) == 1
                and expected_row in trace.after_rows[0][0]
            )
        )
    ]
    search_complete = not workload.get("include_search", False) or bool(search_traces)
    combined_visible = bool(
        search_traces
        and set(active_labels) == set(facets)
        and all(
            any(expected in _folded(label) for label in search_traces[-1].after_labels)
            for expected in active_labels.values()
        )
    )
    return {
        "complete": (
            route_complete
            and facet_complete
            and search_complete
            and combined_visible
        ),
        "route_complete": route_complete,
        "facets_complete": facet_complete,
        "facet_results_changed": facet_results,
        "search_complete": search_complete,
        "combined_facets_visible": combined_visible,
    }


def _audit_scroll(workload: Mapping[str, Any], traces: Sequence[ActionTrace]) -> dict[str, Any]:
    required = int(workload.get("pages", 1))
    directions = tuple(str(item) for item in workload.get("directions", []))
    totals = {direction: 0 for direction in directions}
    rejected_findings = {
        "scroll-direction-mismatch",
        "wrong-scroll-direction",
        "scroll-jump",
        "scroll-lost-selection",
    }
    for trace in traces:
        action = trace.action
        if action.get("kind") != "scroll":
            continue
        direction = str(action.get("direction", ""))
        moved = trace.state_changed and trace.before_rows != trace.after_rows
        clean = not rejected_findings.intersection(trace.finding_codes)
        if (
            direction in totals
            and action.get("by", "page") == "page"
            and moved
            and clean
        ):
            totals[direction] += int(action.get("amount", 1))
    return {
        "complete": bool(totals) and all(total >= required for total in totals.values()),
        "page_totals": totals,
        "required_pages_per_direction": required,
    }


def _audit_section_search(
    workload: Mapping[str, Any],
    traces: Sequence[ActionTrace],
    fixture_tokens: Mapping[str, str],
) -> dict[str, Any]:
    cursor = 0
    section_names = {
        *(_folded(item) for item in workload.get("route_tokens", {})),
        *(_folded(item) for item in workload.get("unsupported", [])),
    }
    route_results: dict[str, bool] = {}
    for source, token_name in workload.get("route_tokens", {}).items():
        source_index = next(
            (
                index
                for index in range(cursor, len(traces))
                if traces[index].action.get("kind") == "activate"
                and traces[index].action.get("target_label") == source
                and traces[index].state_changed
            ),
            None,
        )
        if source_index is None:
            route_results[str(source)] = False
            continue
        typed_index = None
        for index in range(source_index + 1, len(traces)):
            candidate = traces[index]
            if (
                candidate.action.get("kind") == "activate"
                and _folded(candidate.action.get("target_label")) in section_names
            ):
                break
            if (
                candidate.action.get("kind") == "type"
                and candidate.action.get("target_label") == "Search all fields"
                and candidate.action.get("fixture_token") == token_name
                and candidate.state_changed
            ):
                typed_index = index
                break
        expected_row = fixture_tokens.get(str(token_name), "")
        typed_trace = traces[typed_index] if typed_index is not None else None
        passed = bool(
            typed_trace is not None
            and expected_row
            and source in typed_trace.before_selected_labels
            and source in typed_trace.after_selected_labels
            and len(typed_trace.after_rows) == 1
            and expected_row in typed_trace.after_rows[0][0]
        )
        route_results[str(source)] = passed
        if typed_index is not None:
            cursor = typed_index + 1
    unsupported_results: dict[str, bool] = {}
    for source in workload.get("unsupported", []):
        trace = next(
            (
                item
                for item in traces[cursor:]
                if item.action.get("kind") == "activate"
                and item.action.get("target_label") == source
                and item.state_changed
            ),
            None,
        )
        unsupported_results[str(source)] = bool(
            trace and "Search all fields" not in trace.after_actionable_labels
        )
    return {
        "complete": (
            bool(route_results)
            and all(route_results.values())
            and bool(unsupported_results)
            and all(unsupported_results.values())
        ),
        "route_results": route_results,
        "unsupported_search_disabled": unsupported_results,
    }


def _audit_offline(
    workload: Mapping[str, Any],
    traces: Sequence[ActionTrace],
    fixture_tokens: Mapping[str, str],
) -> dict[str, Any]:
    connectivity = [
        str(trace.action.get("connectivity"))
        for trace in traces
        if trace.action.get("kind") == "set-connectivity"
    ]
    phases = [str(item) for item in workload.get("phases", [])]
    required_transitions = phases[1:] if phases[:1] == ["online"] else phases
    transitions_complete = _ordered_contains(connectivity, required_transitions)
    offline_at = next(
        (
            index
            for index, trace in enumerate(traces)
            if trace.action.get("kind") == "set-connectivity"
            and trace.action.get("connectivity") == "offline"
        ),
        None,
    )
    online_at = next(
        (
            index
            for index, trace in enumerate(traces)
            if offline_at is not None
            and index > offline_at
            and trace.action.get("kind") == "set-connectivity"
            and trace.action.get("connectivity") == "online"
        ),
        None,
    )
    source_tokens = workload.get("source_tokens", {})
    source_checks: dict[str, bool] = {}
    for source, token_name in source_tokens.items():
        expected_row = fixture_tokens.get(str(token_name), "")
        source_folded = _folded(source)
        offline_visits = [
            trace
            for index, trace in enumerate(traces)
            if offline_at is not None
            and online_at is not None
            and offline_at < index < online_at
            and trace.action.get("kind") == "activate"
            and _folded(trace.action.get("target_label")) == source_folded
        ]
        recovery_visits = [
            trace
            for index, trace in enumerate(traces)
            if online_at is not None
            and index > online_at
            and trace.action.get("kind") == "activate"
            and _folded(trace.action.get("target_label")) == source_folded
        ]
        source_checks[str(source)] = bool(
            expected_row
            and offline_visits
            and recovery_visits
            and offline_visits[-1].after_labels.count(expected_row) == 1
            and recovery_visits[-1].after_labels.count(expected_row) == 1
        )
    refresh_before_loss = bool(
        offline_at
        and traces[offline_at - 1].action.get("kind") == "activate"
        and "refresh" in _folded(
            traces[offline_at - 1].action.get("target_label")
        )
    )
    retry_while_offline = any(
        trace.action.get("kind") == "activate"
        and "retry" in _folded(trace.action.get("target_label"))
        and offline_at is not None
        and online_at is not None
        and offline_at < index < online_at
        for index, trace in enumerate(traces)
    )
    return {
        "complete": (
            transitions_complete
            and refresh_before_loss
            and retry_while_offline
            and bool(source_checks)
            and all(source_checks.values())
        ),
        "transitions_complete": transitions_complete,
        "refresh_before_loss": refresh_before_loss,
        "retry_while_offline": retry_while_offline,
        "source_rows_single_and_retained": source_checks,
    }


def _audit_restart(
    workload: Mapping[str, Any],
    traces: Sequence[ActionTrace],
    fixture_tokens: Mapping[str, str],
) -> dict[str, Any]:
    connectivity = "online"
    matched = False
    preserve_results: dict[str, bool] = {}
    clear_results: dict[str, bool] = {}
    required_connectivity = workload.get("connectivity")
    expected_status = _folded(workload.get("status_label"))
    for trace in traces:
        action = trace.action
        if action.get("kind") == "set-connectivity":
            connectivity = str(action.get("connectivity"))
        if action.get("kind") == "restart" and (
            required_connectivity is None or connectivity == required_connectivity
        ):
            preserve = workload.get("preserve", [])
            clear = workload.get("clear", [])
            expected_section = str(workload.get("section", ""))
            preserve_results = {
                str(item): (
                    item == "section"
                    and bool(expected_section)
                    and expected_section in trace.before_selected_labels
                    and expected_section in trace.after_selected_labels
                )
                for item in preserve
            }
            search_token = str(workload.get("search_token", ""))
            expected_search = fixture_tokens.get(search_token, "")
            before_values = dict(trace.before_values)
            after_values = dict(trace.after_values)
            clear_results = {
                str(item): (
                    item == "transient-search"
                    and bool(expected_search)
                    and before_values.get("Search all fields") == expected_search
                    and "Search all fields" in after_values
                    and after_values["Search all fields"] == ""
                )
                for item in clear
            }
            connectivity_preserved = (
                required_connectivity is None
                or (
                    bool(expected_status)
                    and any(
                        expected_status in _folded(label)
                        for label in trace.before_labels
                    )
                    and any(
                        expected_status in _folded(label)
                        for label in trace.after_labels
                    )
                )
            )
            matched = (
                all(preserve_results.values())
                and all(clear_results.values())
                and connectivity_preserved
            )
    return {
        "complete": matched,
        "restart_observed": matched,
        "preserve_results": preserve_results,
        "clear_results": clear_results,
        "connectivity_preserved": matched if required_connectivity else None,
    }


def _audit_batch(
    workload: Mapping[str, Any],
    traces: Sequence[ActionTrace],
) -> dict[str, Any]:
    field_tokens = workload.get("field_tokens", {})
    selection_count = int(workload.get("selection_count", 0))
    selection_pattern = str(selection_count)

    def has_selection_marker(trace: ActionTrace) -> bool:
        return any(
            selection_pattern in label and "select" in label.casefold()
            for label in trace.after_labels
        )

    edit_index = next(
        (
            index
            for index, trace in enumerate(traces)
            if trace.action.get("kind") == "activate"
            and "edit" in _folded(trace.action.get("target_label"))
            and trace.state_changed
        ),
        None,
    )
    apply_index = next(
        (
            index
            for index, trace in enumerate(traces)
            if edit_index is not None
            and index > edit_index
            and trace.action.get("kind") == "activate"
            and any(
                word in _folded(trace.action.get("target_label"))
                for word in ("apply", "save")
            )
            and trace.state_changed
        ),
        None,
    )
    edit_opened = edit_index is not None
    edit_applied = apply_index is not None
    typed_tokens = {
        trace.action.get("fixture_token")
        for index, trace in enumerate(traces)
        if edit_index is not None
        and apply_index is not None
        and edit_index < index < apply_index
        and trace.action.get("kind") == "type"
        and trace.state_changed
        and any(
            token == trace.action.get("fixture_token")
            and field in _folded(trace.action.get("target_label"))
            for field, token in field_tokens.items()
        )
    }
    selection_observed = bool(
        edit_index is not None
        and apply_index is not None
        and has_selection_marker(traces[edit_index])
        and has_selection_marker(traces[apply_index])
    )
    progress_probed = any(
        trace.action.get("kind") == "wait"
        and trace.action.get("expect_status") is True
        and (
            trace.after_busy
            or "missing-waiting-feedback" in trace.finding_codes
        )
        for index, trace in enumerate(traces)
        if apply_index is not None and index > apply_index
    )
    first_down_entry = next(
        (
            (index, trace)
            for index, trace in enumerate(traces)
            if edit_index is not None
            and index < edit_index
            if trace.action.get("kind") == "scroll"
            and trace.action.get("direction") == "down"
            and trace.state_changed
            and trace.before_rows != trace.after_rows
        ),
        None,
    )
    last_up_entry = next(
        (
            (index, trace)
            for index, trace in reversed(list(enumerate(traces)))
            if apply_index is not None
            and index > apply_index
            and trace.action.get("kind") == "scroll"
            and trace.action.get("direction") == "up"
            and trace.state_changed
            and trace.before_rows != trace.after_rows
        ),
        None,
    )
    first_down = first_down_entry[1] if first_down_entry else None
    last_up = last_up_entry[1] if last_up_entry else None
    before_anchor = dict(first_down.before_rows) if first_down else {}
    after_anchor = dict(last_up.after_rows) if last_up else {}
    shared_anchors = set(before_anchor) & set(after_anchor)
    scroll_anchor_restored = any(
        abs(before_anchor[label] - after_anchor[label]) <= 6.0
        for label in shared_anchors
    )
    return {
        "complete": (
            set(field_tokens.values()).issubset(typed_tokens)
            and selection_observed
            and edit_opened
            and edit_applied
            and progress_probed
            and first_down is not None
            and last_up is not None
            and scroll_anchor_restored
        ),
        "field_tokens_typed": sorted(typed_tokens & set(field_tokens.values())),
        "selection_observed": selection_observed,
        "edit_opened": edit_opened,
        "edit_applied": edit_applied,
        "progress_probed": progress_probed,
        "scroll_anchor_probe_directions": [
            direction
            for direction, present in (
                ("down", first_down is not None),
                ("up", last_up is not None),
            )
            if present
        ],
        "scroll_anchor_restored": scroll_anchor_restored,
        "requires_fixture_audit": True,
    }


def audit_action_workload(
    workload_index: int,
    workload: Mapping[str, Any],
    traces: Sequence[ActionTrace],
    fixture_tokens: Mapping[str, str] | None = None,
) -> dict[str, Any]:
    """Audit one checkpoint against actions and retained before/after labels."""
    kind = str(workload.get("kind", "unknown"))
    if kind == "batch-edit":
        details = _audit_batch(workload, traces)
    elif kind == "sort-cycle":
        details = _audit_sort(workload, traces)
    elif kind == "combined-filter":
        details = _audit_filter(workload, traces, fixture_tokens or {})
    elif kind == "scroll-sweep":
        details = _audit_scroll(workload, traces)
    elif kind == "offline-transition":
        details = _audit_offline(workload, traces, fixture_tokens or {})
    elif kind == "section-search":
        details = _audit_section_search(workload, traces, fixture_tokens or {})
    elif kind == "restart":
        details = _audit_restart(workload, traces, fixture_tokens or {})
    else:
        details = {"complete": False, "error": "unsupported workload kind"}
    return {"workload_index": workload_index, "kind": kind, **details}
