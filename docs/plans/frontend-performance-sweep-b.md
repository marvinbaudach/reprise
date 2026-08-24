---
slug: frontend-performance-sweep-b
worktree: /home/marvin/Projects/reprise-frontend-performance-sweep-b
branch: feature/frontend-performance-sweep-b
phase: planned
codex_session:
created: 2026-08-24
---
# Strand B — GNOME: build and cache what is actually shown

Mother plan: `docs/plans/frontend-performance-sweep.md`. Read it first — it
carries the rule this strand runs under (a task that cannot show its number is
reverted, not shipped).

**Owns `crates/reprise-gnome/**`.** Nothing outside that path.

Line numbers are against `origin/dev` at `7eaf16e4d3` (re-checked after dev moved; no Android or showroom file changed in between). The three tasks are
independent of each other and should be committed separately.

---

## B1 — Measure, then defer, the content stack's views

### The defect

`window.rs:251–254`, `window.rs:316–317` and `source_views.rs:145–147` add
Library, Stats, Library-Doctor, Concerts, Releases, Podcasts, YouTube and Radio
into one `GtkStack`. All eight are constructed and realised at startup; one is
visible. Construction is not free even beyond the widgets —
`PodcastsView::install` reads configuration from the database while building.

The same problem was found, measured and fixed one directory over.
`preferences_window.rs:127–142` records it: 128 ms of page construction plus
130 ms of realisation, "two thirds of the 314 ms it took the dialog to appear —
mostly on pages nobody had asked to see", fixed with empty `adw::Bin` holders
filled **synchronously** on `visible-child` — never idle-deferred, because
callers that navigate by setting the visible child must find the page there.

### Why this is two tasks

The preferences fix was justified by a measurement. This one is not yet, and
these views are harder: several are wired from the outside at construction —
`radio.set_on_mutated(...)` right after `source_views.rs:147`, sidebar
refreshes, navigation and search reaching in by name. Deferring construction
means deferring that wiring, and doing that blind is how a working sidebar
quietly stops refreshing. That failure is silent, not a crash.

### B1.0 — measure first

Put a `tracing` span around each view's construction and each `add_named`, run a
cold start against a real library, and record the per-view cost as a table in
the branch. That table decides the rest.

### B1.1 — defer only what the table indicts

Views costing single-digit milliseconds stay eager; the risk is not worth it.
For each view the table does indict:

- replace the stack child with an `adw::Bin` holder,
- move construction **and its external wiring** into the materialise closure,
- materialise synchronously on `visible-child`, exactly as the preferences shell
  does.

Before starting, grep for `content_stack` and for each view's accessors. Every
caller that reaches a view by name has to be routed through the same
materialise call; that set is the actual scope of this task, and it is larger
than the three files named above.

It is a legitimate outcome for this task to cover two views, or none.

### Measurement

Time from `activate` to first painted frame, cold, same database, five runs
each, median. The startup measurement path from the preferences ticket applies
unchanged.

---

## B2 — Key the cover cache by the file, not by the track, and evict by use

### The defect

Two independent weaknesses in `cover_loader.rs`:

1. **The key is the track.** `cover_loader.rs:67` and `:471` say it outright —
   the cache is keyed by track path, "so the second track of an album is a miss
   even though it resolves to the very same file". A 15-track album is 15
   decodes of one `cover.jpg` and 15 entries spent on one image.
2. **Eviction is FIFO.** `cover_loader.rs:24–25`: 256 entries, "evicts
   oldest-inserted first". That throws out exactly what a reader returns to when
   they scroll back up; an entry that has been on screen the whole time is as
   evictable as one nobody looked at.

### The change

Two maps instead of one — track path → resolved cover path, resolved cover path
→ texture — so an album decodes once. The resolved path is already carried in
`CachedCover.path` (`cover_loader.rs:30`), so nothing new has to be discovered
to key by it.

Then make eviction least-recently-**used**: touch on read, evict from the cold
end. The existing `VecDeque` order list becomes an LRU with a move-to-back on
hit.

**Invalidation must key off the cover path too.** Otherwise a re-downloaded
cover for one track keeps serving stale pixels to the rest of its album. That is
the one way this change can break behaviour, so it needs its own test.

### Measurement

Hit/miss counters around the lookup, then two runs: open a 15-track album
(expect 14 additional hits — one decode instead of 15), and scroll a 500-row
list down and back up (expect the FIFO cliff to disappear).

---

## B3 — Give the other list models the delta the track list already has

### The defect

`podcasts_model.rs:66`, `releases_model.rs:68` and `concerts_model.rs:66`
replace wholesale: `store.remove_all()` then `append` per row. That is
`items_changed(0, old, new)` — every widget rebound, selection and scroll
position gone, every cover re-requested.

`radio_model.rs:76` is **partly** fixed already: it has a no-op gate
(`if self.rows() == rows { return; }`) and documents the damage it repaired —
"`remove_all()` + append made `GtkSingleSelection` autoselect row 0 (it saw the
selected item removed) and reset the scroll offset while the store stood empty".
What it still lacks is the delta for the case where something *did* change: one
station edited still rebuilds the whole list.

The track list stopped doing this long ago: `track_list_model_change.rs`,
`now_playing_marker.rs`, `rating_cell_refresh.rs`, and
`window_queue_model.rs`'s `refresh_on_model_change`, which suppresses a refresh
over an identical model and has tests for it.

### The change

A shared diff, not a fourth hand-written variant. Each `replace()` becomes a
diff against what the store holds: unchanged rows stay, changed rows are
replaced in place, added and removed rows move only their own range, and an
identical list emits nothing at all.

Row identity is available in all four:

| Model | Identity |
|-------|----------|
| `podcasts_model.rs` | `EpisodeRow.id` |
| `releases_model.rs` | `HistoryEntry.release_group_mbid` |
| `concerts_model.rs` | `ConcertRow.id` |
| `radio_model.rs` | `StationRow` (already compared whole for the no-op gate) |

Keep radio's existing gate; it becomes the cheap fast path in front of the diff
rather than something to replace.

### Measurement

Per model: select a row, scroll away from the top, trigger a refresh that
changes one row, assert selection and scroll offset survive. Plus a bind counter
showing an unchanged refresh binds zero widgets.

---

## Verification

Rust workspace tests plus the display-owned tests under Xvfb. `xvfb` does not
isolate GTK4 on Wayland on its own — use the harness the repository already
provides rather than a bare `xvfb-run`, or the run reaches the real session.
Run the strand under `heavy-run`; strand C runs at the same time.
