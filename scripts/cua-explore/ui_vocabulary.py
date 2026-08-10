#!/usr/bin/env python3
"""Central vocabulary for CUA roles and stable Reprise surface labels."""

from __future__ import annotations

from typing import Iterable, Mapping


CANONICAL_ROW_ROLE = "row"

# Every spelling variant seen from cua-driver or Atspi.get_role_name(), folded
# onto one token. This is the only place that knows about spellings; the
# geometry matcher and the hover sweep both go through canonical_role. Each
# entry was measured, never guessed, and an unknown spelling still falls
# through unchanged so it fails visibly where it is used.
ROLE_ALIASES: Mapping[str, str] = {
    "list item": CANONICAL_ROW_ROLE,
    "listitem": CANONICAL_ROW_ROLE,
    "table row": CANONICAL_ROW_ROLE,
    "tablerow": CANONICAL_ROW_ROLE,
    "tree item": CANONICAL_ROW_ROLE,
    "treeitem": CANONICAL_ROW_ROLE,
    "push button": "button",
    "pushbutton": "button",
    "togglebutton": "toggle button",
    "checkbox": "check box",
    "radiobutton": "radio button",
    "menuitem": "menu item",
    "table cell": "grid cell",
    "tablecell": "grid cell",
    "gridcell": "grid cell",
    "tree grid": "table",
    "treegrid": "table",
    "frame": "window",
    "grouping": "group",
    "scrollbar": "scroll bar",
}

# Measured over all 1020 recorded snapshots of the 2026-08-10 exploratory run
# (GTK 4.22, Reprise edd458e8df): 27 distinct action names, exactly one of them
# invocable. Structural means assistive technology may call it, but it is not a
# user affordance - GTK4 puts listitem.scroll-to on every row and cell of a
# ColumnView, list.select-all on every list, and the win.*/window.*/default.*
# GActions on the window itself.
STRUCTURAL_ACTION_PREFIXES = ("listitem.", "list.", "win.", "window.", "default.")
MEASURED_INVOCABLE_ACTIONS = frozenset({"click"})

# Only the spellings that really denote a row - ROLE_ALIASES also carries
# button, cell and window variants now, and folding those in here would have
# made the row matcher accept everything.
ROW_ROLES = frozenset(
    {CANONICAL_ROW_ROLE}
    | {
        spelling
        for spelling, canonical in ROLE_ALIASES.items()
        if canonical == CANONICAL_ROW_ROLE
    }
)
BUTTON_ROLES = frozenset(
    {
        "button",
        "push button",
        "toggle button",
        "link",
        "check box",
        "radio button",
        "menu item",
    }
)
SOFT_HOVER_ROLES = frozenset(
    {CANONICAL_ROW_ROLE, "cell", "tab", "chip", "cover tile"}
)
ENTRY_ROLES = frozenset({"entry", "search box", "text field"})
VALUE_BEARING_ROLES = frozenset({*ENTRY_ROLES, "slider", "spin button"})
ACTIONABLE_ROLES = frozenset(
    {*BUTTON_ROLES, *SOFT_HOVER_ROLES, *ENTRY_ROLES, "switch"}
)
WINDOW_ROLES = frozenset({"window", "frame", "dialog", "application"})
BUSY_ROLES = frozenset({"progress bar", "spinner", "status", "statusbar"})
BUSY_WORDS = (
    "loading",
    "refreshing",
    "scanning",
    "saving",
    "working",
    "progress",
    "queued",
    "waiting",
)
OFFLINE_WORDS = ("offline", "no connection", "needs network", "queued offline")
SEARCH_ENTRY_LABEL = "Search all fields"

# Static labels mirrored from sidebar_rebuild.rs and its section headings.
KNOWN_SECTION_LABELS = (
    "Music",
    "Podcasts",
    "YouTube",
    "Radio",
    "Queue",
    "Playlists",
    "Releases",
    "Concerts",
    "My Stats",
    "Import errors",
    "Missing files",
    "Library Doctor",
)


def canonical_role(role: str) -> str:
    """Return the stable role spelling used by every harness consumer."""
    normalized = str(role or "unknown").strip().casefold()
    return ROLE_ALIASES.get(normalized, normalized)


def is_structural_action(name: str) -> bool:
    return str(name).startswith(STRUCTURAL_ACTION_PREFIXES)


def invocable_actions(names: Iterable[str]) -> tuple[str, ...]:
    """Keep measured invocable and unknown names visible to the harness."""
    return tuple(name for name in names if not is_structural_action(name))


def unknown_action_names(names: Iterable[str]) -> tuple[str, ...]:
    return tuple(
        name
        for name in names
        if not is_structural_action(name) and name not in MEASURED_INVOCABLE_ACTIONS
    )


def is_row(role: str) -> bool:
    return canonical_role(role) == CANONICAL_ROW_ROLE


def is_buttonish(role: str) -> bool:
    return canonical_role(role) in BUTTON_ROLES


def is_entry(role: str) -> bool:
    return canonical_role(role) in ENTRY_ROLES


def hover_strictness(role: str) -> str:
    normalized = canonical_role(role)
    if normalized in BUTTON_ROLES:
        return "strict"
    if normalized in SOFT_HOVER_ROLES:
        return "soft"
    return "skip"
