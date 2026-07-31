# Place Pill vs Filter Pill — Design

**Status:** design approved 2026-07-31, not yet planned into tasks.
**Branch:** `feat/place-pill-vs-filter-pill` (worktree `../reprise-place-pill`, based on `origin/dev` @ `31d8fa062a`).

## Problem

The filter row renders two different things in one identical shape. An
artist/album/genre scope appears as a chip that looks exactly like a search or
facet chip, under the heading `FILTER`, with an accented `3 of 9 tracks`
count — but its `×` does not remove a filter. It leaves the location via a
NAV-2 history push (`filter_restriction::scope_chip_label` →
`browse_bar` scope button → `MetadataNavigator::leave_scope`).

Same shape, same heading, same counting vocabulary; different meaning,
different gesture, different consequence. The inconsistency is visible inside
the row itself: the row says `FILTER`, yet `Clear all ×` never appears for a
scope, because `filters_restrict()` is false for it.

### Measured behaviour (2026-07-31, isolated instance)

Measured against a private Xvfb display, a private D-Bus session, a throwaway
XDG profile, `REPRISE_AUDIO_SINK=fakesink`, and nine generated fixture tracks
across three artists. Findings, in the order they were observed:

1. Artist page shows `FILTER`, pill `Alpha Artist ×`, count `3 of 9 tracks` —
   visually indistinguishable from a facet chip.
2. Double-clicking a track on the artist page returned the view to the full
   library immediately (`queue set from view queue_len=3`, then 20 ms later
   `query matched 9 tracks source=library`). Plain "Add to queue", with no
   playback at all, did the same. Cause: every queue mutation emits
   `up next changed` → sidebar refresh → `resolve_select_source` falls back to
   Library because artist/album/genre scopes deliberately have no sidebar row.
3. Clearing the pill deliberately logs `scope chip cleared` and navigates to
   Library; the playback queue is untouched (PLAY-8, as designed).
4. At the end of the queue: automatic end stops; a *manual* next refills from
   the now-visible view (`refill_len=9`), so the other artists do play — but
   only later and only on explicit demand.

Finding 2 is a separate bug and is **out of scope here**: it is already fixed
on the parallel branch `fix/scope-chip-survives-sidebar-refresh` (worktree
`../reprise-scope-chip`) via a `has_sidebar_row()` guard in
`sidebar_rebuild::rebuild`. This design must not reimplement it.

## Goal

Make "this is a place" and "this is a filter" distinguishable without trying
them out — in shape, position, gesture, and counting vocabulary.

Non-goal: changing what playback does. PLAY-8 (immutable snapshot) and PLAY-3b
(later filter changes do not touch a built queue) stay exactly as they are, for
scopes and facets alike.

## Model

Two concepts, two presentations:

| | **Place** | **Filter** |
|---|---|---|
| Meaning | where you are | what is withheld inside it |
| Entered/left by | navigation, history push | state change at the same place |
| Shown by | sidebar row — or, when none exists, the **place pill** | chips + `Clear all` |
| Applies to | Artist, Album, Genre | search, facets, Hide AI music |

**Place pill exactly when the place has no sidebar row.** Music, Radio, Queue,
playlists, smart playlists, Missing and Recently added are selected in the
sidebar and need no second location display. Artist, album and genre pages are
opened from inside the track list and have no row — there the pill is the only
thing naming the location and the only way back.

This is the same distinction `has_sidebar_row()` draws in the parallel bugfix.
One truth, two uses; keep them in one place rather than duplicating the match.

## Design

### 1. Row layout

```
place zone            │ filter zone                        counter
[ ‹ Alpha Artist ]    │ FILTER ( ⌕ falling × )  Clear all ×   2 of 3 tracks
```

- Left: place zone. Empty when the place has a sidebar row.
- Separator: only when both zones are populated.
- Right: filter zone — `FILTER` label, chips, `Clear all ×`; only when a real
  filter is active. In the Library the idle state keeps `+ Add filter`.
- Far right: the counter.

At a sidebar place the row therefore looks exactly as it does today.

### 2. Shape and gesture

- **Place pill:** outlined, leading `‹`, **no** `×`. The whole pill is the click
  target rather than a 20 px cross — this exceeds FIL-1c's hit-target
  requirement instead of merely meeting it. Tooltip and accessible label name
  the destination ("Leave the artist page"), not a removal.
- **Filter chips:** filled/accented with `×`, unchanged.

The two differ in form, position and gesture, not only in wording — the current
design's whole weakness.

### 3. Counting

- Place without filter: `3 tracks`, neutral.
- Place with filter: `2 of 3 tracks`, accented — **relative to the place**.
- Library with filter: `15 of 1,664 tracks`, unchanged.

No place speaks about the whole library any more. A playlist showing `12 tracks`
while 1,652 others exist is the established precedent; an artist page is the
same kind of location.

Implementation shrinks rather than grows: `browse_filter_count::source_total`
currently substitutes `ViewSource::Library` as the counting base whenever
`scope_restricts(source)` holds. That branch is deleted; `total_source` is
always the source itself.

### 4. Row visibility

`scope_restricts()` loses its role as a restriction — otherwise the row would
permanently claim `FILTER` at a place with no filter set.

New law: the row is visible when a filter is active **or** a place pill is due
**or** the preference asks for it. "An invisible active filter is a bug" (FIL-1a)
still holds, and the place pill can never be hidden by the preference.

`filter_restriction.rs` keeps its role as the single pure visibility law. Its
scope vocabulary is renamed to place vocabulary (`scope_restricts` →
`has_place_pill`, `scope_chip_label` → `place_pill_label`), and
`ViewSource::RecentlyAdded` leaves that set.

### 5. Follow-on copy

FIL-3's end-of-results line and FIL-6's zero-hit empty state talk about
filters, so inside a place they must talk about the place: "Show all 3 tracks",
not "Show all 1,664 tracks". Clearing never leaves the place — which is already
true of `Clear all` today (it "never changes location", FIL-1c), the copy just
has to stop implying otherwise.

## Rule changes (docs/ux-rules.md section K)

These were grilled decisions on 2026-07-17. They are being changed knowingly,
because the measurement shows this exact filter vocabulary at a place is what
produces the confusion. Recorded as changes with rationale, not silently.

- **FIL-1c** — rewritten: place pill instead of scope chip; own outlined shape;
  whole-pill click target; no filter vocabulary; counting relative to the place.
  The NAV-2 history push and the restoration of remembered search/facets on
  return are unchanged.
- **FIL-2** — extended by the counting base: `X of Y` always relates to the
  current place, never to the library from within a sub-place. Row visibility
  gains the place-pill condition.
- **FIL-8** — `Recently added` keeps its source and its sort, but loses the
  scope pill: it is a sidebar place and the sidebar already marks it.

## Files

| File | Change |
|---|---|
| `browse/browse_bar.rs` | two zones, separator, place pill construction, `FILTER` label gating |
| `browse/filter_restriction.rs` | place vocabulary, visibility law, `RecentlyAdded` out |
| `browse/browse_filter_count.rs` | drop the Library counting-base branch |
| `browse/browse_filter_strings.rs`, `strings_filter.rs` | place pill labels, tooltips, accessible names, place-relative "Show all N" |
| `style/buttons.rs` / CSS | outlined place pill class next to `reprise-filter-chip` |
| `track_list/end_of_results.rs` | place-relative copy |
| tests alongside each | rule-named tests per the section-K conventions |

Overlap with `fix/scope-chip-survives-sidebar-refresh` is limited to
`window/metadata_navigation.rs`, and there only in tests. That branch lands
first; this work rebases onto it.

## Testing

- Pure decisions (`filter_restriction`, `browse_filter_count`) get display-free
  rule-named tests — they carry the flipped rules and must run in the workspace
  suite.
- Widget-level checks (zone layout, pill shape, click target, tooltip) are
  non-rule-named `#[ignore = "requires a display; run via xvfb-run"]` tests via
  `scripts/check-display-tests.sh`, run individually: the display suite is
  herd-flaky, only single runs are evidence.
- One end-to-end re-measurement of the original report: artist page → play →
  leave via the pill → queue unchanged → manual next refills from the library.
  The rig used for the findings above is reusable.

## Core support (verified)

`queries::query_track_count_browsed_conn` already dispatches
`Artist`/`Album`/`Genre` to their own per-place counters
(`library_views::query_album_track_count` and siblings), so passing the place
itself as `total_source` with an empty filter yields the place's own total. No
core change is required — the GTK helper only stops overriding the base.
