---
slug: browse-bar-joins-the-filter-grammar
worktree:
branch:
phase: planned
codex_session:
created: 2026-09-06
---
# The track list's browse bar joins the filter grammar

**Draft — not grilled.** Split out of
`deleting-and-tag-saving-stop-paying-on-the-main-thread.md` during its grill
(2026-09-06) because it measures nothing that plan measures. Carries the
fifth commit strand b of `responsive-editing-and-one-table-grammar.md` could
not land (record in `…-b.md`, "B2 fifth commit — dropped").

## Why it stopped

`BrowseBar` (`crates/reprise-gnome/src/ui/browse/browse_bar.rs`, ~640 lines)
exceeds the mother plan's abort criterion with three special cases outside
`FilterModel`:

1. the source/place zone and its callbacks (`set_source_context`,
   `set_on_scope_cleared`, `set_on_search_cleared`, `set_on_clear_all`);
2. the database-backed searchable value chooser with row counts
   (`browse_chooser.rs`);
3. the sticky Library-only "Hide AI music" filter.

Keeping every `pub` signature byte-identical would have left a wrapper larger
than the plan permitted. Commit `4bfebfceb0` only imported the canonical
chooser page constants.

## Tasks (to be grilled)

1. Source/place zone as a `FilterBar` slot: the bar takes an optional leading
   widget and forwards the scope callbacks — one commit, `pub` signatures
   unchanged for the four callers (`track_list_builder.rs`,
   `track_list_reload.rs`, `track_list.rs`, `preference_layout.rs`).
2. The DB-backed chooser as a paging `FilterModel::values` (a page size and a
   query string) — `Releases`' static values become the trivial case.
3. "Hide AI music" as a non-removable picker chip, the way the Releases
   window is one.
4. `BrowseBar` on `FilterBar<TrackListFilterModel>`; the thinness
   single-definition check covers it; ux-rules §K gets the track list's row.

Abort criterion as before: a `pub` signature the callers cannot follow in the
same commit.

## Parallelität

_to be decided in the grill; likely a single strand — every task changes
`browse_bar.rs`._
