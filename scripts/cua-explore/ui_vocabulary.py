#!/usr/bin/env python3
"""Central vocabulary for CUA roles and stable Reprise surface labels."""

from __future__ import annotations

from typing import Mapping


CANONICAL_ROW_ROLE = "row"

ROLE_ALIASES: Mapping[str, str] = {
    "list item": CANONICAL_ROW_ROLE,
    "table row": CANONICAL_ROW_ROLE,
    "tree item": CANONICAL_ROW_ROLE,
}

ROW_ROLES = frozenset({CANONICAL_ROW_ROLE, *ROLE_ALIASES})
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
