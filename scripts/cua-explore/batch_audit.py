"""The batch tag-edit audit: selection, progress, and the scroll anchor across the write."""

from __future__ import annotations

import re
from typing import Any, Mapping, Sequence

from audit_traces import ActionTrace, folded, header_clicks

# The anchor check compares row positions across an edit. A handful of rows that
# happen to keep their place says nothing: the rows the page shows must all keep
# it, and enough of them must be on both sides to be sure it is the same page.
SCROLL_ANCHOR_TOLERANCE_PX = 6.0
SCROLL_ANCHOR_MIN_SHARED_ROWS = 5


# A selection marker is the count standing next to a selection noun, as in the
# tag dialog title "Edit 512 Tracks" or a status line "512 selected". The count
# merely occurring somewhere inside a longer string is not evidence: the 100k
# fixture names its rows "Track NNNNNN", so "Track 005128" would otherwise pass.
SELECTION_MARKER_NOUNS = ("tracks", "track", "songs", "song", "items", "item", "selected")
SELECTION_MARKER_VERBS = ("selected", "selection", "select")


def selection_marker_pattern(selection_count: int) -> re.Pattern[str]:
    """Match the count immediately adjacent to a selection noun, either order."""
    nouns = "|".join(re.escape(noun) for noun in SELECTION_MARKER_NOUNS)
    verbs = "|".join(re.escape(verb) for verb in SELECTION_MARKER_VERBS)
    count = re.escape(str(selection_count))
    return re.compile(
        rf"(?<!\d){count}(?!\d)[\s\u00a0]*(?:{nouns})\b"
        rf"|\b(?:{verbs})[\s:\u00a0]*(?<!\d){count}(?!\d)",
        re.IGNORECASE,
    )


def label_shows_selection_count(label: str, selection_count: int) -> bool:
    return selection_marker_pattern(selection_count).search(label) is not None


def anchor_rows(
    rows: Sequence[tuple[str, float]], title_pattern: re.Pattern[str] | None
) -> dict[str, float]:
    """Row positions keyed by what identifies the row across a tag write.

    A row's label is all its cells, the year among them, and the batch edit sets
    every year, so the full label never survives the write. The title cell does:
    `anchor_title_pattern` picks it out of the label. Without a pattern the full
    label is the key.
    """
    anchors: dict[str, float] = {}
    for label, y in rows:
        if title_pattern is None:
            anchors.setdefault(label, y)
            continue
        found = title_pattern.search(label)
        if found is not None:
            anchors.setdefault(found.group(0), y)
    return anchors


def audit_batch(
    workload: Mapping[str, Any],
    traces: Sequence[ActionTrace],
) -> dict[str, Any]:
    field_tokens = workload.get("field_tokens", {})
    selection_count = int(workload.get("selection_count", 0))
    selection_pattern = selection_marker_pattern(selection_count)

    def shows_selection(labels: Sequence[str]) -> bool:
        return any(selection_pattern.search(label) for label in labels)

    edit_index = next(
        (
            index
            for index, trace in enumerate(traces)
            if trace.action.get("kind") == "activate"
            and "edit" in folded(trace.action.get("target_label"))
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
                word in folded(trace.action.get("target_label"))
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
            and field in folded(trace.action.get("target_label"))
            for field, token in field_tokens.items()
        )
    }
    # The dialog title "Edit 512 Tracks" is an unindexed label, so it is read
    # from the tree labels. It must stand on screen right after the dialog opened
    # and still right before Save is pressed (the dialog is gone or busy after).
    selection_observed = bool(
        edit_index is not None
        and apply_index is not None
        and shows_selection(
            (*traces[edit_index].after_labels, *traces[edit_index].after_tree_labels)
        )
        and shows_selection(
            (
                *traces[apply_index].before_labels,
                *traces[apply_index].before_tree_labels,
                *traces[apply_index].after_labels,
                *traces[apply_index].after_tree_labels,
            )
        )
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
    anchor_title = workload.get("anchor_title_pattern")
    title_pattern = re.compile(str(anchor_title)) if anchor_title else None
    before_anchor = anchor_rows(first_down.before_rows, title_pattern) if first_down else {}
    after_anchor = anchor_rows(last_up.after_rows, title_pattern) if last_up else {}
    shared_anchors = set(before_anchor) & set(after_anchor)
    held_anchors = {
        key
        for key in shared_anchors
        if abs(before_anchor[key] - after_anchor[key]) <= SCROLL_ANCHOR_TOLERANCE_PX
    }
    scroll_anchor_restored = (
        len(shared_anchors) >= SCROLL_ANCHOR_MIN_SHARED_ROWS
        and held_anchors == shared_anchors
    )
    sort_by = workload.get("sort_by")
    sorted_before_edit = sort_by is None or (
        edit_index is not None
        and any(
            column == folded(sort_by)
            for column, trace in header_clicks([str(sort_by)], traces[:edit_index])
        )
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
            and sorted_before_edit
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
        "scroll_anchor_rows_shared": len(shared_anchors),
        "scroll_anchor_rows_held": len(held_anchors),
        "sorted_by_unedited_column": sorted_before_edit,
        "requires_fixture_audit": True,
    }
