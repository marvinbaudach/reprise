"""What an audit reads of one executed action, and the helpers every audit shares."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping, Sequence

from ui_vocabulary import column_header_label


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
    after_roles: tuple[tuple[str, str], ...] = ()
    # Labels of nodes the driver lists without indexing them - a dialog title, a
    # filter chip's two halves - in tree order. Absent when the snapshot has none.
    before_tree_labels: tuple[str, ...] = ()
    after_tree_labels: tuple[str, ...] = ()
    finding_codes: tuple[str, ...] = ()
    state_changed: bool = False
    after_busy: bool = False


def folded(value: object) -> str:
    return str(value or "").casefold()


def header_clicks(
    columns: Sequence[str], traces: Sequence[ActionTrace]
) -> list[tuple[str, ActionTrace]]:
    """Activations of a column header that moved the rows, with their column.

    A column is credited for a click on its own header only. The header row is
    one node to the driver ("Title Artist Album Year Length Rating") and used to
    be matched by column name as a substring, which credited every column for a
    click that lands on the middle of the row.
    """
    column_of = {folded(column_header_label(column)): folded(column) for column in columns}
    return [
        (column_of[label], trace)
        for trace in traces
        if trace.action.get("kind") == "activate"
        and (label := folded(trace.action.get("target_label"))) in column_of
        and trace.state_changed
        and trace.before_rows != trace.after_rows
    ]
