#!/usr/bin/env python3
"""Expectations both pipeline streams assert against - written down exactly once.

Stream I changes what counts as an affordance; stream II tests an agent that
walks the same observation. If each of them wrote this set down separately they
would drift, and the drift would only show after the merge - green in isolation,
red together. So it lives here and both import it.

Derived on 2026-08-10 by applying the invocable/structural rule to
`fixtures/night-2026-08-10-music-collapsed.json`: 83 labels before, 31 after.
Every table *cell* is gone; the table *rows* and the column header stay, because
role `row` is in ACTIONABLE_ROLES and that is unrelated to actions.

Do not hand-edit this list. Regenerate it from the fixture.
"""

from __future__ import annotations

# `actionable_labels` of night-2026-08-10-music-collapsed.json, after the rule.
MUSIC_COLLAPSED_ACTIONABLE_LABELS = (
    "Add filter",
    "Back to previous view",
    "Close",
    "Dismiss",
    "Go to album Fixture Album 00 Writable Batch 0001 Fixture Artist 00 Fixture Album 00 1980 3:00 —",
    "Go to album Fixture Album 00 Writable Batch 0065 Fixture Artist 00 Fixture Album 00 1999 3:00 ★ ★ ★ ★ ☆",
    "Go to album Fixture Album 01 Writable Batch 0002 Fixture Artist 01 Fixture Album 01 1981 3:00 ★ ☆ ☆ ☆ ☆",
    "Go to album Fixture Album 01 Writable Batch 0066 Fixture Artist 01 Fixture Album 01 2000 3:00 ★ ★ ★ ★ ★",
    "Go to album Fixture Album 02 Writable Batch 0003 Fixture Artist 02 Fixture Album 02 1982 3:00 ★ ★ ☆ ☆ ☆",
    "Go to album Fixture Album 02 Writable Batch 0067 Fixture Artist 02 Fixture Album 02 2001 3:00 —",
    "Go to album Fixture Album 03 Writable Batch 0004 Fixture Artist 03 Fixture Album 03 1983 3:00 ★ ★ ★ ☆ ☆",
    "Go to album Fixture Album 32 Writable Batch 0033 Fixture Artist 00 Fixture Album 32 2012 3:00 ★ ★ ☆ ☆ ☆",
    "Go to album Fixture Album 32 Writable Batch 0097 Fixture Artist 00 Fixture Album 32 1986 3:00 —",
    "Go to album Fixture Album 33 Writable Batch 0034 Fixture Artist 01 Fixture Album 33 2013 3:00 ★ ★ ★ ☆ ☆",
    "Go to album Fixture Album 33 Writable Batch 0098 Fixture Artist 01 Fixture Album 33 1987 3:00 ★ ☆ ☆ ☆ ☆",
    "Go to album Fixture Album 34 Writable Batch 0035 Fixture Artist 02 Fixture Album 34 2014 3:00 ★ ★ ★ ★ ☆",
    "Go to album Fixture Album 34 Writable Batch 0099 Fixture Artist 02 Fixture Album 34 1988 3:00 ★ ★ ☆ ☆ ☆",
    "Main menu (F10)",
    "Maximise",
    "Minimise",
    "Play (Space)",
    "Repeat off — playback stops after the queue",
    "Search all fields",
    "Shuffle",
    "Title Artist Album Year Length Rating",
    "Toggle Now Playing panel",
    "Toggle sidebar",
    "Undo",
    "Volume",
    "★",
    "☆",
)

MUSIC_COLLAPSED_ACTIONABLE_COUNT = 31
MUSIC_COLLAPSED_ACTIONABLE_COUNT_BEFORE = 83

# Structural action names measured across all 1020 snapshots of the 2026-08-10
# night run. Exactly one invocable name existed in the whole corpus.
MEASURED_STRUCTURAL_EXAMPLES = (
    "listitem.scroll-to",
    "list.select-all",
    "list.unselect-all",
    "win.about",
    "window.close",
    "default.activate",
)
MEASURED_INVOCABLE_EXAMPLES = ("click",)
