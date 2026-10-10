#!/usr/bin/env python3
"""Independent checks that mission workloads were actually exercised."""

from __future__ import annotations

import re
from typing import Any, Mapping, Sequence

from audit_traces import ActionTrace, folded as _folded, header_clicks as _header_clicks
from batch_audit import (  # noqa: F401 - the names other modules import from here
    SCROLL_ANCHOR_MIN_SHARED_ROWS,
    SCROLL_ANCHOR_TOLERANCE_PX,
    SELECTION_MARKER_NOUNS,
    SELECTION_MARKER_VERBS,
    anchor_rows,
    audit_batch as _audit_batch,
    label_shows_selection_count,
    selection_marker_pattern,
)
from section_handles import section_handle
from ui_vocabulary import RETRY_WORDS


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
    columns = tuple(str(item) for item in workload.get("columns", []))
    repetitions = int(workload.get("repetitions", 0))
    successful = _header_clicks(columns, traces)
    covered = {column for column, _trace in successful}
    matching = len(successful)
    return {
        "complete": matching >= repetitions and covered == {_folded(c) for c in columns},
        "matching_actions": matching,
        "required_actions": repetitions,
        "covered_columns": sorted(covered),
        "clicks_per_column": {
            column: sum(1 for item, _trace in successful if item == _folded(column))
            for column in columns
        },
    }


# The list's own counter, "100,000 tracks · 265 d 3 h" or "4,974 of 100,000 tracks".
RESULT_COUNTER = re.compile(r"^[\d,]+(?: of [\d,]+)? tracks?\b", re.IGNORECASE)


def _results_changed(trace: ActionTrace) -> bool:
    """Whether the action changed what the list shows, by its rows or its counter.

    A filter can leave the visible rows alone: with the list sorted by artist, the
    hundred rows of "Artist 0000" all pass a genre filter, and the first page is
    the same before and after. The counter ("4,974 of 100,000 tracks") still moves.
    """

    def counters(labels: Sequence[str]) -> frozenset[str]:
        return frozenset(label for label in labels if RESULT_COUNTER.match(label))

    return _row_labels(trace.before_rows) != _row_labels(trace.after_rows) or counters(
        trace.before_tree_labels
    ) != counters(trace.after_tree_labels)


def chip_shown(
    labels: Sequence[str], tree_labels: Sequence[str], expected: str
) -> bool:
    """Whether the filter chip "Facet: value" is on screen.

    A chip used to be one label and is now two unindexed ones in a row, the facet
    name and its value ("Genre", "Genre 00"), so the tree labels are read as
    consecutive pairs. A value may carry a suffix ("4 stars"). The filter popover
    lists the same values as items with a count, so a value on its own proves
    nothing; the facet name has to stand right before it.
    """
    folded = _folded(expected)
    if any(folded in _folded(label) for label in labels):
        return True
    facet, separator, value = folded.partition(":")
    if not separator:
        return False
    facet, value = facet.strip(), value.strip()
    tree = [_folded(label) for label in tree_labels]
    return any(
        first == facet and (second == value or second.startswith(f"{value} "))
        for first, second in zip(tree, tree[1:])
    )


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
                    and _results_changed(traces[index])
                    and chip_shown(
                        traces[index].after_labels,
                        traces[index].after_tree_labels,
                        expected,
                    )
                    and not chip_shown(
                        traces[index].before_labels,
                        traces[index].before_tree_labels,
                        expected,
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
            chip_shown(
                search_traces[-1].after_labels,
                search_traces[-1].after_tree_labels,
                expected,
            )
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
    # The mission file lists the routes in one order and a reasoning agent, which
    # reads the mission with sorted keys, may walk them in another. Each route is
    # therefore looked up on its own; only the unsupported sections must come
    # after every typed route.
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
                for index in range(len(traces))
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
            cursor = max(cursor, typed_index + 1)
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
    section_names = {_folded(source) for source in source_tokens}

    def visit(start: int) -> Sequence[ActionTrace]:
        """The traces of one visit: the section activation and what follows it.

        A visit ends at the next section, a connectivity change or a restart.
        Podcasts and YouTube keep the cached episode behind a card, so the
        episode is on screen after the click that unfolds the card, not after the
        click that opened the section.
        """
        for index in range(start + 1, len(traces)):
            action = traces[index].action
            if action.get("kind") in {"set-connectivity", "restart"} or (
                action.get("kind") == "activate"
                and _folded(action.get("target_label")) in section_names
            ):
                return traces[start:index]
        return traces[start:]

    def listed_once(window: Sequence[ActionTrace], expected_row: str) -> bool:
        shown = [trace for trace in window if expected_row in trace.after_labels]
        return bool(shown) and shown[-1].after_labels.count(expected_row) == 1

    source_checks: dict[str, bool] = {}
    for source, token_name in source_tokens.items():
        expected_row = fixture_tokens.get(str(token_name), "")
        source_folded = _folded(source)
        starts = [
            index
            for index, trace in enumerate(traces)
            if trace.action.get("kind") == "activate"
            and _folded(trace.action.get("target_label")) == source_folded
        ]
        offline_starts = [
            index
            for index in starts
            if offline_at is not None
            and online_at is not None
            and offline_at < index < online_at
        ]
        recovery_starts = [
            index for index in starts if online_at is not None and index > online_at
        ]
        source_checks[str(source)] = bool(
            expected_row
            and offline_starts
            and recovery_starts
            and listed_once(visit(offline_starts[-1]), expected_row)
            and listed_once(visit(recovery_starts[-1]), expected_row)
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
        and any(
            word in _folded(trace.action.get("target_label"))
            for word in RETRY_WORDS
        )
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


def _audit_hover_sweep(
    workload: Mapping[str, Any],
    traces: Sequence[ActionTrace],
    fixture_tokens: Mapping[str, str],
) -> dict[str, Any]:
    sections = tuple(str(item) for item in workload.get("sections", []))
    section_of = {
        section_handle(workload, fixture_tokens, section): section
        for section in sections
    }
    minimum = int(workload.get("min_targets_per_section", 0))
    visited = {section: False for section in sections}
    hovered = {section: 0 for section in sections}
    measured = {section: 0 for section in sections}
    hover_findings: list[dict[str, Any]] = []
    current_section: str | None = None
    unmeasured_codes = {"hover-unmeasurable", "hover-skipped"}
    for trace in traces:
        action = trace.action
        if action.get("kind") == "activate" and action.get("target_label") in section_of:
            current_section = section_of[str(action.get("target_label"))]
            if trace.state_changed:
                visited[current_section] = True
            continue
        if action.get("kind") != "hover" or current_section is None:
            continue
        label = str(action.get("target_label", ""))
        roles = dict(trace.after_roles)
        codes = [str(code) for code in trace.finding_codes]
        hovered[current_section] += 1
        if not unmeasured_codes.intersection(codes):
            measured[current_section] += 1
        hover_findings.append(
            {
                "section": current_section,
                "label": label,
                "role": roles.get(label, "unknown"),
                "codes": codes,
            }
        )
    complete = bool(sections) and all(
        visited[section]
        and hovered[section] >= minimum
        and measured[section] >= 1
        for section in sections
    )
    return {
        "complete": complete,
        "sections_visited": visited,
        "hovered_per_section": hovered,
        "measured_per_section": measured,
        "hover_findings": hover_findings,
    }


def workloads_click_column_headers(workloads: Sequence[Mapping[str, Any]]) -> bool:
    """Whether any workload sorts, so the observation must carry the headers."""
    return any(
        workload.get("kind") == "sort-cycle" or workload.get("sort_by")
        for workload in workloads
    )


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
    elif kind == "hover-sweep":
        details = _audit_hover_sweep(workload, traces, fixture_tokens or {})
    else:
        details = {"complete": False, "error": "unsupported workload kind"}
    return {"workload_index": workload_index, "kind": kind, **details}
