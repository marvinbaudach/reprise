---
slug: responsive-editing-and-one-table-grammar-b
worktree: /home/marvin/Projects/reprise-responsive-editing-and-one-table-grammar-b
branch: feature/responsive-editing-and-one-table-grammar-b
phase: refactored
codex_session:
created: 2026-09-05
---
# Strand B — `tables`: one grammar for the source views and the track list's bar

Strand B of `docs/plans/responsive-editing-and-one-table-grammar.md`. Read the
mother plan's §1 (goal G6, non-goals) and §2 (rules) first; §2 binds every
task here. Branch `feature/responsive-editing-and-one-table-grammar-b`.

This is Package 2.2 of the consolidation plan, cut into landable steps, plus the
sort and reload halves the audit's matrix showed. Order matters: each
conversion lands green on its own.

## File ownership

- Owns: `crates/reprise-gnome/src/ui/browse/**` — including `browse_bar.rs`,
  whose **public API is frozen**: `BrowseBar` keeps every `pub` signature the
  track list and strand A call (`refresh()` among them) —
  `crates/reprise-gnome/src/ui/releases/**`, `crates/reprise-gnome/src/ui/radio/**`,
  `crates/reprise-gnome/src/ui/concerts/**`,
  `crates/reprise-gnome/src/ui/podcasts/podcasts_{filter_bar,model,presentation,view}*.rs`,
  `crates/reprise-gnome/src/ui/library_doctor/review_filter_bar.rs`,
  `crates/reprise-gnome/src/ui/table_columns/**`,
  `crates/reprise-gnome/src/ui/list_store_delta.rs`,
  `crates/reprise-gnome/src/ui/filter_bar_layout.rs`,
  `crates/reprise-gnome/src/ui/source_empty_state.rs`,
  `docs/ux-rules.md`, and **one** of `scripts/check-frontend-thinness.sh` or
  `scripts/check-project-quality.sh` (for B5).
- Never edits: `ui/podcasts/source_image*.rs`, `ui/track_list/**`,
  `ui/window/**`. The view constructors (`ConcertsView::new`,
  `ReleasesView::new`, …) keep their signatures because strand C's wiring
  files call them; a task that needs a new input adds a setter.
- This is the **only** strand that edits `docs/ux-rules.md` (section K
  exceptions).

## Task B1 — `FilterBar<M: FilterModel>` exists, unused

**Files.** New `crates/reprise-gnome/src/ui/browse/filter_bar.rs` (and
`filter_bar_tests.rs`), `crates/reprise-gnome/src/ui/filter_bar_layout.rs`
(read; extend only if a slot is missing).

The generic bar owns what the five bars duplicate: the search entry with its
committed query and debounce, the facet page → value page popover, the chips,
"Clear all", the counting line (FIL-2a), the end-of-results hand-off (FIL-3a),
and the 12 `FilterBarLayout` slots.

`FilterModel` is the per-source trait and it is **string-keyed**: facets and
values are identified by `&str` ids and carry display labels; there are no
associated types. The four bars' facets are string-ish already, and generics
over facet and value types would multiply GObject boilerplate for nothing.
Shape:

```rust
pub trait FilterModel {
    fn facets(&self) -> Vec<FacetDescriptor>;            // { id: &'static str, label: String }
    fn values(&self, facet_id: &str) -> Vec<ValueDescriptor>; // { id: String, label: String }
    fn apply(&self, query: &str, selections: &[(String, String)]) -> Filter;
    fn persistence_key(&self) -> &'static str;
}
```

Adjust field names to what the existing bars already use; the point is string
ids plus labels, no `type Facet`/`type Value`. Public surface of the bar:
`new(model)`, `widget()`, `filter()`, `set_on_changed`, `set_committed_query`,
`set_counts`, `clear_all` — the seven names the survey found in four or more of
the existing bars.

Tests first, in `filter_bar_tests.rs`, against a test `FilterModel` with two
facets: committed query round-trip, chip add/remove, clear-all resets both
query and chips, the count line text for 0/1/n, Escape clears the section
(the behaviour `search_4a_*_escape_and_chip_share_the_section_clear_path` is
copy-pasted into three bars today — this is its single home).

## Task B2 — the four sources and `BrowseBar` move onto it (five commits)

**Files.** `crates/reprise-gnome/src/ui/releases/releases_filter_bar.rs`,
`ui/radio/radio_filter_bar.rs`, `ui/podcasts/podcasts_filter_bar.rs`,
`ui/concerts/concerts_filter_bar.rs`, `ui/browse/browse_bar.rs`, plus each
view's wiring file that constructs the bar (`*_view.rs`).

One commit per bar, in this order (smallest special-case count first):

1. **releases** (unique: `show_widest`);
2. **radio** (`set_rows`, public `apply_filter`);
3. **podcasts** (`result_text`, `set_selection_count`);
4. **concerts** (`set_on_open_location`, `reload_persisted`);
5. **browse_bar** — the track list's bar, last.

Each `<source>_filter_bar.rs` shrinks to its `FilterModel` impl plus the one or
two source-specific methods, expected 60–120 lines. The source's existing tests
(7, 8, 5, 0) stay **unchanged** and green; that is the proof the conversion
changed nothing. Concerts has no tests, so its commit starts by writing the
four that the other three share (query round-trip, chip, clear-all, count line)
against the *old* bar, watching them pass, then converting.

The fifth commit converts `BrowseBar` internally onto `FilterBar<M>` while
keeping **every `pub` signature** on `BrowseBar` byte-identical — the track
list (`ui/track_list/**`, not owned here) and strand A's `delete_tracks.rs`
call it, and neither may need a change. Its existing tests stay unchanged and
green. If `BrowseBar` needs more than two special-case methods outside
`FilterModel`, or a `pub` signature would have to change, **drop the fifth
commit** and say so in the report; the four source conversions stand on their
own. Never partially convert it.

Section K of `docs/ux-rules.md` applies to all five for the first time. Where a
source must deviate (Package 2.2 expects one to three cases), write the
exception into the rule text in the same commit — never a silent deviation.

## Task B3 — one sort grammar

**Files.** New `crates/reprise-gnome/src/ui/table_columns/sort.rs`;
`ui/releases/releases_presentation.rs`, `ui/radio/radio_model.rs`,
`ui/podcasts/podcasts_presentation.rs`, `ui/concerts/concerts_presentation.rs`
(`sort_rows`, `:12-19`).

`SortSpec<K> { key: K, direction: SortDirection }` and one generic
`sort_rows<R, K: SortKey<R>>(rows, spec)`; the four `*SortKey` enums implement
`SortKey<Row>` (a `cmp(&self, a, b)` per key). `single_sort_indicator.rs`
already lives in `table_columns/` and stays the header-side half. The track
list keeps its SQL `ORDER BY` whitelist (`track_list_sort.rs:18-45`) — it sorts
in the database, not in memory, and that is correct for 100k rows.

Tests first: a property-style test in `sort.rs` (stable for equal keys, reverse
direction reverses order, ties broken by the secondary key each source
declares), then the four existing per-view sort tests unchanged.

## Task B4 — one delta reload

**Files.** `crates/reprise-gnome/src/ui/list_store_delta.rs` (read),
`ui/concerts/concerts_model.rs`, `ui/podcasts/podcasts_model.rs`,
`ui/radio/radio_model.rs` (`replace`, each with the `identical`/`changed`
tests), and their `*_view.rs` callers.

Releases already goes through `list_store_delta::replace<R, K>`. The other three
`<view>_model.rs::replace` implementations become calls into it, keyed the way
their tests already key (`radio_model.rs:70` documents why `remove_all` +
append was abandoned: `GtkSingleSelection` lost the selection). The three sets
of `identical`/`changed` tests stay unchanged and green.

MOT-8 ("lists do not move") is the rule this serves; if a display test named
after it exists for releases, add the same for radio (the other view with a
live-updating list).

## Task B5 — the gate line

**Files.** `scripts/check-frontend-thinness.sh` or `scripts/check-project-quality.sh`
(whichever already greps for duplicated constants; read both first, edit one).

One check: `FILTER_BAR_MIN_HEIGHT`, `FACET_PAGE`, `VALUE_PAGE` are defined in
exactly one file under `crates/reprise-gnome/src/ui`. The audit counted ×5 and
×3; after B2 the count is 1, and the gate keeps it there. Prove the gate: it is
red on a deliberately duplicated constant (revert that before committing) and
green on the converted tree.

## Acceptance for strand B

- The five filter-bar test sets, four sort test sets and three model test sets
  pass unchanged.
- `wc -l` of the four `*_filter_bar.rs` files sums to under 600 (from 2 635).
- The gate line is red on a deliberately duplicated constant and green on the
  converted tree.
- The report states whether B2's fifth commit (`browse_bar`) landed, and if not,
  which special case or `pub` signature stopped it.

## Abort criteria

- A source that needs more than two special-case methods outside `FilterModel`
  stays on its own bar; say so and keep the other conversions.
- A task that needs `ui/track_list/**` or `ui/window/**` stops and reports; a
  view constructor signature change is replaced by a setter.

## Refactor-pass disposition

- **B2 fifth commit — dropped.** `BrowseBar` exceeds the abort criterion with
  three independent special cases outside `FilterModel`: its source/place
  zone and callbacks, the database-backed searchable value chooser with row
  counts, and the sticky Library-only "Hide AI music" filter. Preserving its
  frozen public signatures would therefore leave a wrapper larger than the
  plan permits. Commit `4bfebfceb0` only imported the canonical chooser page
  constants; it did not partially convert `BrowseBar`.
- **B4 — deferred.** The original strand did not attempt the one-delta-reload
  task or record an abort. This review-finding pass is restricted to the
  accepted findings and was explicitly instructed not to attempt B4, so no
  `list_store_delta.rs` or source-model reload path was changed.

## Post-merge follow-ups

- **Finding 14 — release sorting ownership.** Reconcile the duplicate release
  comparison logic after merge: either remove the now-unused
  `reprise_core::artist_news::sort_release_rows_by_display_text` helper or
  have `ReleaseRowSortKey` delegate to Core's comparison functions. This
  strand does not own `reprise-core`, so this pass deliberately made no Core
  change.
