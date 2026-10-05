---
slug: refactor-wave-2026-10-a
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-a
branch: feature/refactor-wave-2026-10-a
phase: shipped
codex_session:
created: 2026-10-04
---
# Refactor wave 2026-10 — strand A

Mother plan: `docs/plans/refactor-wave-2026-10.md`. Its "Standing rules for every strand" bind this strand.

## Strand A — query parameter objects (consolidation 3.3)

**Purpose.** Replace the positional overload family with a value that describes a track view.
Thread the same value through the per-source helpers. Lock the gain in with a budget.

**Owns:**

- `crates/reprise-core/src/queries/**`
- Call sites:
  - `crates/reprise-core/src/library/library_doctor/scope.rs`
  - `crates/reprise-core/src/device_sync/settings.rs`
  - `crates/reprise-core/src/device_sync/snapshot.rs`
  - `crates/reprise-mcp/src/data.rs`
  - `crates/reprise-cli/src/commands/playlist.rs`
  - `crates/reprise-gnome/src/ui/browse/{browse_bar,browse_filter_count}.rs`
  - `crates/reprise-gnome/src/ui/library_doctor/start_page.rs`
  - `crates/reprise-gnome/src/ui/mpris_play_context.rs`
  - `crates/reprise-gnome/src/ui/playback/library_continuation.rs`
  - `crates/reprise-gnome/src/ui/sidebar/sidebar_rebuild.rs`
  - `crates/reprise-gnome/src/ui/tag_edit/tag_edit_flow.rs`
  - `crates/reprise-gnome/src/ui/track_list/{track_list_activation,track_list_model}.rs`
- Any test file that calls a changed function.
- `scripts/check-architecture.sh`, only the new budget block.

This list is a starting point, not a fence. A file that has to change to keep a call site
compiling may be added. Stop only if the contract itself is wrong.

**A1 — invariant test first (red).** Add `queries/tests_track_view.rs`. Seed a library with
enough tracks that a text filter and one browse facet each cut the set. For `Library`,
`RecentlyAdded`, a `Playlist` and `Queue`, assert that all three queries agree:

```
query_track_count(view) == query_track_ids(view, sort).len()
                        == query_track_window(view, sort, RowWindow { offset: 0, limit: <all> }, AiColumn::Project).len()
```

Also assert that `TrackViewQuery::new(&src)` has empty defaults, that its browse value equals
`BrowseFilter::default()`, and that `with_exclude_ai(true)` hides a provenance-flagged track in
`Library`. The test does not compile until A2 lands; that compile failure is the red step.

**A2 — the types and three entry points** go in the new `queries/track_view.rs`:

```rust
pub struct TrackViewQuery<'a> {            // Clone, Copy, Debug
    pub source: &'a ViewSource,
    pub filter: &'a str,
    pub browse: &'a BrowseFilter,
    pub queue_items: &'a [QueueItem],
    pub exclude_ai: bool,                  // only `Library` honours it, as today
}
impl<'a> TrackViewQuery<'a> {
    pub fn new(source: &'a ViewSource) -> Self;           // "", EMPTY_BROWSE, &[], false
    pub fn with_filter(self, filter: &'a str) -> Self;
    pub fn with_browse(self, browse: &'a BrowseFilter) -> Self;
    pub fn with_queue_items(self, items: &'a [QueueItem]) -> Self;
    pub fn with_exclude_ai(self, exclude: bool) -> Self;
}
pub struct TrackSort<'a> { pub field: &'a str, pub dir: &'a str }   // Clone, Copy, Debug
pub struct RowWindow { pub offset: i64, pub limit: i64 }             // Clone, Copy, Debug, Eq
pub enum AiColumn { #[default] Project, Skip }                       // Clone, Copy, Debug, Eq, Default

pub fn query_track_window(db: &Db, view: &TrackViewQuery<'_>, sort: TrackSort<'_>,
                          rows: RowWindow, ai: AiColumn) -> Result<Vec<Track>, rusqlite::Error>;
pub fn query_track_count(db: &Db, view: &TrackViewQuery<'_>) -> Result<…same as today…>;
pub fn query_track_ids(db: &Db, view: &TrackViewQuery<'_>, sort: TrackSort<'_>) -> Result<Vec<i64>, rusqlite::Error>;
pub(crate) fn query_track_ids_in(conn: &Connection, view: &TrackViewQuery<'_>, sort: TrackSort<'_>) -> …;
```

`EMPTY_BROWSE` is a module-level `static BrowseFilter` with every field empty. The A1 test pins
it to `BrowseFilter::default()`.

These three replace all nine overloads and the `_conn` variants. Nothing is released, so the old
names are deleted outright with no shims. A crate-internal `_in(conn, …)` form stays wherever a
caller holds only a `Connection`.

Expected mapping. The current code is the authority: if it disagrees with this table, keep what
the code does and say so in the summary.

| Old call | New `view` | `ai` |
| --- | --- | --- |
| `query_track_window(…)` | `new(src).with_filter(f).with_queue_items(q)` | `Project` |
| `query_track_window_browsed(…)` | the above `.with_browse(b)` | `Project` |
| `query_track_window_browsed_ai(…, ex, proj)` | the above `.with_exclude_ai(ex)` | `Project` if `proj`, else `Skip` |

The count and id families map the same way. Their `exclude_ai` default stays whatever the code
passes today.

**A3 — thread the objects through the helpers.** In `queries/{library,playlist,smart,
library_views,clauses,queue}.rs`, each helper takes its source-specific key (a playlist id, an
album key) plus the parameter objects it needs, instead of exploded fields.

- **Album and artist windows.** In `library_views.rs`, `query_album_track_window`,
  `query_artist_track_window` and `query_album_track_ids_browsed` get the same treatment. They
  may take `TrackSort` and `RowWindow` plus their own small struct where `TrackViewQuery` (which
  needs a `ViewSource`) does not fit. Do not invent a `ViewSource` for them.
- **Target:** zero `#[allow(clippy::too_many_arguments)]` under `crates/reprise-core/src/queries/`,
  test helpers such as `tests_issues.rs::seed_missing_track` included. A test helper gets a small
  fixture struct or fewer parameters.
- **Unneeded allows.** Clippy's threshold is "more than 7". An allow on a function with at most
  7 parameters is simply deleted.

**A4 — move the dispatch out of `queries/mod.rs`.**

- The window, count and ids dispatch moves into `track_view.rs`.
- `mod.rs` re-exports the moved items, so `reprise_core::queries::query_track_window` and the
  other paths keep resolving.
- Replace the doc comment above the old `query_track_window`, which argues against a parameter
  object, with one that describes the objects.
- Target: `mod.rs` at about 400 lines.

**A5 — update every call site and test.**

- Build the view once per function and reuse it when that function issues several queries
  (for example, count plus window in `track_list_model.rs`).
- Keep the builder chains short.
- Headroom is tight in three files: `tag_edit_flow.rs` (788), `track_list_model.rs` (767) and
  `device_sync/settings.rs` (763). If one of them would reach 800, extract a cohesive sibling
  there.

**A6 — budget.** In `scripts/check-architecture.sh`, add a `too_many_arguments_budget` block
modelled exactly on the `http_boundary_budget` block:

- Count lines matching `allow(clippy::too_many_arguments)` under `crates/`.
- Fail when the count is above the budget.
- When it is below, fail with "lower too_many_arguments_budget to N".
- Set the budget to the count after A3.

**Verification (A):**

```
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test -p reprise-core
cargo test -p reprise-mcp -p reprise-cli
cargo test -p reprise-gnome
scripts/check-architecture.sh
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must be empty
```
