---
slug: refactor-wave-2026-10-w4b
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-w4b
branch: feature/refactor-wave-2026-10-w4b
phase: planned
codex_session:
created: 2026-10-05
---
# Refactor wave 2026-10, wave 4 — strand B: parameter objects

Mother plan: `docs/plans/refactor-wave-2026-10-w4.md`; its "Shared context" binds this strand.
Origin: wave 2 gave every `too_many_arguments` suppression a reason; 20 of the 29 reasons say the
function "should take a parameter object" (or a named type). This strand resolves 10 of them and
lowers the budget to the measured count. The other 10 are listed under "Not in this strand" with
the reason.

This plan is your only channel. When the code disagrees with a table below, the code wins: keep
what the code does and say so in your final message.

## Purpose

Ten suppressions go; `too_many_arguments_budget` in `scripts/check-architecture.sh` becomes 19.

**Behaviour-preserving means:** every function keeps its body's effects, order of operations,
logging, error handling and return value; the only change is how its inputs arrive. No widget
tree, string, SQL, timing or accessibility change. A test that pins a widget tree (the podcast
add-dialog chrome tests, the sync-row display tests, the block-move display test) must pass
unchanged except for the call expression it uses.

## Evidence (origin/dev @ 9465e997e8, 2026-10-05)

`scripts/check-architecture.sh:298-315` counts `(allow|expect)\(\s*clippy::too_many_arguments`
under `crates/` and fails when the count differs from `too_many_arguments_budget=29` (line 301) in
either direction. Clippy's threshold is 7: a function with 8 or more inputs (counting `self`)
triggers; 7 does not.

### Sites in this strand

| # | File:line (attribute) | Function | Inputs today | Becomes |
| --- | --- | --- | --- | --- |
| 1 | `crates/reprise-gnome/src/ui/browse/browse_filter_count.rs:26` | `update(bar, conn, source, count, search, browse, exclude_ai, queue_ids: &[i64])` | 8 | `update(bar, conn, view: TrackViewQuery<'_>, count)` — 4 |
| 2 | `crates/reprise-gnome/src/ui/track_list/track_list_model.rs:361` | `set_query_browsed_ai(&self, source, sort_field, sort_dir, filter, browse, queue_items, exclude_ai)` | 8 | `set_query_browsed_ai(&self, view: TrackViewQuery<'_>, sort_field, sort_dir)` — 4 (or 3 with `TrackSort`, see B1) |
| 3 | `…/track_list_model.rs:387` | `set_query_browsed_ai_changed(&self, …same seven…, change: ModelChange)` | 9 | `(&self, view, sort_field, sort_dir, change)` — 5 |
| 4 | `…/track_list_model.rs:418` | `set_query_browsed_ai_inner(&self, …same seven…, requested_change: Option<ModelChange>)` | 9 | `(&self, view, sort_field, sort_dir, requested_change)` — 5 |
| 5 | `crates/reprise-gnome/src/ui/updates/concerts_section.rs:179` | `render(&self, enabled, _has_credentials, total, unseen, rows, today, cached_portraits)` | 8 | drop `_has_credentials` — 7 |
| 6 | `crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs:120` | `replace(container, groups, playing_episode, expanded_sources, expanded_episode_sources, download_states, images_allowed, conn, connectivity, unavailable_episode, selection, query)` — `#[cfg(test)]` | 12 | `replace(container, groups, inputs: GroupRenderInputs<'_>)` — 3 |
| 7 | `…/podcasts_groups.rs:156` | `replace_with_sync(…same twelve…, syncing: &HashMap<i64, SyncRowState>)` | 13 | `replace_with_sync(container, groups, inputs, syncing)` — 4 |
| 8 | `crates/reprise-gnome/src/ui/podcasts/add_dialog.rs:471` | `attach_candidates(receiver, request_generation, generation, status, results, conn, on_added, heading, query, auto_download_default, empty_status, follower_request)` | 12 | `attach_candidates(receiver, request_generation, generation, surface: ResultSurface<'_>, conn, on_added, options: AddOptions)` — 7 |
| 9 | `…/add_dialog.rs:566` | `preview(request_generation, kind, url, generation, status, results, conn, on_added)` | 8 | `preview(request_generation, kind, url, generation, surface, conn, on_added)` — 7 |
| 10 | `crates/reprise-gnome/src/ui/track_list/tag_mutation_refresh.rs:160` | `refresh_after_tag_mutation_with_model_change(shared, ids, paths, anchor, viewport, model_change: Option<ModelChange>, current_ids: Vec<i64>, metadata_only: bool)` | 8 | `(shared, ids, paths, anchor, viewport, change: Option<TagMutationChange>)` — 6 |

Line numbers are those of the `#[expect(` line on origin/dev. The generation-token types in
`add_dialog.rs` may be the `Generation` newtype from #1114 rather than `u64`/`Rc<Cell<u64>>`; keep
whatever the file has.

### Existing types to reuse

- `reprise_core::queries::TrackViewQuery<'a>` (`crates/reprise-core/src/queries/track_view.rs:24`,
  re-exported from `queries/mod.rs:213`): `#[derive(Clone, Copy, Debug)]` with public fields
  `source: &'a ViewSource`, `filter: &'a str`, `browse: &'a BrowseFilter`,
  `queue_items: &'a [QueueItem]`, `exclude_ai: bool`, and builders `new(source)`, `with_filter`,
  `with_browse`, `with_queue_items`, `with_exclude_ai`. `set_query_browsed_ai_inner` already builds
  one from its seven arguments (`track_list_model.rs:441-…`); `source_total` in
  `browse_filter_count.rs:70-94` builds one too. `TrackSort<'a>` (`track_view.rs`, "The
  caller-selected ordering for a track query", `Copy`) exists right below it.
- `GroupRenderContext<'a>` (`podcasts_groups.rs:77-…`) is the private struct `replace_with_sync`
  assembles from its arguments plus `paths` and `episode_artwork`. The new public input struct
  feeds it; do not merge the two (the context carries render-pass-only fields).
- `ContentPages<'a>` (`library_shell.rs:239`) is not used by this strand.

### Callers (measured)

| Site | Production callers | Test callers |
| --- | --- | --- |
| 1 | `track_list_reload.rs:673-682` (passes `&[]` for `queue_ids`) | `browse_filter_count.rs` `mod tests` (check whether they call `update` or `source_total`) |
| 2 | `track_list_model.rs:343-351` (`set_query_browsed`), `track_list_reload.rs:638-646` | — |
| 3 | 6 call sites in `track_list/` (find with the compiler) | — |
| 4 | sites 2 and 3 | — |
| 5 | `updates/popover.rs:397-406` (`self.concerts_section.render(concerts_enabled, concerts.credentials, …)`) | — |
| 6 | — | `podcasts_groups_tests.rs:238, 372, 429, 480` |
| 7 | `podcasts_view.rs:470-487` | `podcasts_sync_row_display_tests.rs:68-82, 141-154, 210-223` |
| 8 | `add_dialog.rs` ×3 (Apple Podcasts, YouTube, RSS branches, around lines 380-470) | — |
| 9 | `add_dialog.rs` ×2 (RSS detection ~283, search-result preview ~300) | — |
| 10 | `tag_mutation_refresh.rs:148-157`, `:193-202` | `tag_mutation_refresh_block_move_display_tests.rs:162-171` |

File sizes: `track_list_model.rs` 763, `track_list_reload.rs` 771, `podcasts_groups.rs` 719,
`podcasts_groups_tests.rs` 742, `add_dialog.rs` 711, `podcasts_view.rs` 568, `concerts_section.rs`
352, `tag_mutation_refresh.rs` 314, `browse_filter_count.rs` 201. All must end below 800. A new
struct with doc comment is ~15 lines; `track_list_model.rs` gains none (it reuses the core type).

## Decisions (fixed — do not re-open)

1. **Reuse `TrackViewQuery`; do not invent a GTK twin.** The reason text names it, the core type
   is `Copy`, and the inner function already constructs it. If `_inner` also assembles a
   `TrackSort` from `sort_field`/`sort_dir`, take `TrackSort<'_>` instead of the two `&str`s;
   otherwise keep the two strings (B1 decides by reading the body).
2. **`update` keeps today's counting semantics exactly.** `source_total` ignores the caller's
   `browse` and uses `BrowseFilter::default()` with the queue items; it must keep doing that, now
   reading `view.source` and `view.queue_items` (the caller passes `&[]`, so the conversion from
   `&[i64]` to `QueueItem::Track` disappears with no behaviour change — say so in the commit).
3. **New types are private to their module** (`pub(super)`/`pub(in crate::ui)` as the callers
   require), live beside the function they serve, carry a one-line doc comment and derive only
   what their fields allow (`Debug` where every field is `Debug`; `Clone` only if a caller needs
   it). No `Default`.
4. **`concerts_section::render` loses `_has_credentials`.** If that makes the `credentials` field
   of the popover's concerts state unread, remove the field and its producer in the same commit
   provided every edit stays inside `crates/reprise-gnome/src/ui/updates/`; otherwise keep the
   field and report where it is still read.
5. **`replace` (test-only) stays** as a thin `#[cfg(test)]` wrapper that passes an empty `syncing`
   map, so the four group tests change only their argument shape.
6. **Seven inputs is the ceiling, not the target.** Sites 8 and 9 end at exactly 7; that is under
   clippy's threshold and the suppression goes. Do not bundle further to "make room".
7. **Budget 29 → 19** in the same PR, in the last commit, after the count is measured by the gate
   itself (`scripts/check-architecture.sh` prints the number when it disagrees).

## Owns

- `crates/reprise-gnome/src/ui/browse/browse_filter_count.rs`
- `crates/reprise-gnome/src/ui/track_list/{track_list_model,track_list_reload,tag_mutation_refresh,tag_mutation_refresh_block_move_display_tests}.rs`
  and any other `track_list/` file the compiler names as a caller of sites 2-4 (list them)
- `crates/reprise-gnome/src/ui/updates/{concerts_section,popover}.rs` (+ the state file if
  decision 4 applies)
- `crates/reprise-gnome/src/ui/podcasts/{podcasts_groups,podcasts_groups_tests,podcasts_sync_row_display_tests,podcasts_view,add_dialog}.rs`
- `scripts/check-architecture.sh` — the single line `too_many_arguments_budget=29`

Not owned: the 10 sites under "Not in this strand"; `reprise-core`; `reprise-view`;
`ui/podcasts/add_dialog_followers.rs` (foreign, strand b of the tests-stop-waiting program — do
not touch it even if a follower-request type seems to belong there); any `Cargo.toml`.

## Tasks (in order, one commit each)

**B1 — the track view query (sites 1-4).** Read `set_query_browsed_ai_inner` to see how it builds
`TrackViewQuery` (and whether a `TrackSort`). Change the three `track_list_model.rs` signatures
per the table; the inner body uses the passed `view` instead of rebuilding it. Update
`set_query_browsed` (internal caller) and the `track_list_reload.rs` callers to construct the view
at the call site: `queries::TrackViewQuery::new(&source).with_filter(&filter).with_browse(&browse).with_queue_items(&queue_items).with_exclude_ai(exclude_ai)`.
Then `browse_filter_count::update`: the caller builds the same kind of view (with
`.with_queue_items(&[])`, which is `new`'s default — omit the call); `update` reads
`view.source`, `view.filter`, `view.browse`, `view.exclude_ai`; `source_total(conn, view,
restricted, count)` builds its own counting view from `view.source` and `view.queue_items` with a
default browse, exactly as today. Delete the four `#[expect(…)]` attributes.
`cargo test -p reprise-gnome browse_filter_count` and `cargo test -p reprise-gnome track_list_model`
(non-display tests) must pass.

**B2 — `concerts_section::render` (site 5).** Drop the parameter and the argument at
`popover.rs:397-406`; decision 4 for the field. Delete the attribute.
`cargo test -p reprise-gnome updates` (non-display) must pass.

**B3 — `GroupRenderInputs` (sites 6-7).** In `podcasts_groups.rs`, directly above `replace`:

```rust
/// The inputs of one render pass of the source-group list; `replace_with_sync` turns them into
/// the private `GroupRenderContext` together with the per-pass artwork and path tables.
pub(super) struct GroupRenderInputs<'a> {
    pub(super) playing_episode: Option<EpisodeMark>,
    pub(super) expanded_sources: &'a Rc<RefCell<BTreeSet<i64>>>,
    pub(super) expanded_episode_sources: &'a Rc<RefCell<BTreeSet<i64>>>,
    pub(super) download_states: &'a BTreeMap<i64, DownloadState>,
    pub(super) images_allowed: bool,
    pub(super) conn: &'a Rc<Db>,
    pub(super) connectivity: Connectivity,
    pub(super) unavailable_episode: Option<i64>,
    pub(super) selection: &'a Rc<RefCell<PodcastSelection>>,
    pub(super) query: &'a str,
}
```

`replace_with_sync(container, groups, inputs, syncing)` moves the fields into `GroupRenderContext`
in the same order as today. `replace(container, groups, inputs)` (test-only) calls it with
`&HashMap::new()`. Adapt the one production caller and the seven test call sites. Delete both
attributes. `cargo test -p reprise-gnome podcasts_groups` (non-display) must pass; the three
display tests in `podcasts_sync_row_display_tests.rs` run under xvfb, one process each (use the
isolation recipe), and must pass.

**B4 — `ResultSurface` and `AddOptions` (sites 8-9).** In `add_dialog.rs`:

```rust
/// The two widgets a candidate list is rendered into.
struct ResultSurface<'a> { status: &'a gtk4::Label, results: &'a gtk4::Box }

/// How the candidates of one request are offered: the list heading, the query that produced
/// them, the auto-download default, the empty-list status, and the YouTube follower request.
struct AddOptions {
    heading: String,
    query: Option<String>,
    auto_download_default: bool,
    empty_status: String,
    follower_request: Option<YoutubeFollowerRequest>,
}
```

`attach_candidates` clones `surface.status`/`surface.results` where it cloned the two widgets
before; `preview` likewise. Five call sites. Delete both attributes. Both add-dialog chrome display
tests (`add_dialog_chrome_tests.rs`) and `cargo test -p reprise-gnome add_dialog` (non-display)
must pass unchanged.

**B5 — `TagMutationChange` (site 10).** In `tag_mutation_refresh.rs`:

```rust
/// A model change the reload must apply after a tag mutation, with the ids that are current
/// after it and whether the mutation touched metadata only.
pub(in crate::ui) struct TagMutationChange {
    pub(in crate::ui) model: ModelChange,
    pub(in crate::ui) current_ids: Vec<i64>,
    pub(in crate::ui) metadata_only: bool,
}
```

The function maps `change` into today's `ReloadChange` (`query: reload_query_key(shared)` stays
inside the function). The caller at `:148-157` passes
`reload_change.map(|model| TagMutationChange { model, current_ids: after_ids, metadata_only })`;
`:193-202` passes `Some(TagMutationChange { model: model_change, current_ids: after_ids,
metadata_only: false })`; the display test likewise. Delete the attribute. The block-move display
test runs under xvfb and must pass.

**B6 — the budget.** Run `scripts/check-architecture.sh`; it must report
"too-many-arguments suppressions are down to 19". Set `too_many_arguments_budget=19`. Run it again:
green. Never cite a `docs/plans/…` path in the script.

## Not in this strand (and why)

| Site | Reason it stays |
| --- | --- |
| `window/window_playing_source_wiring.rs:22 install` (7), `window/window_layout_test_hook.rs:34 publish` (8), `window/window_deferred_source_wiring.rs:13 install` (9), `window/library_shell.rs:82 wire_source_routing` (16), `library_shell.rs:302 route_to_place_with_viewport` (8), `preferences/preferences.rs:155 new` (22) | one-caller composition-root wiring; a struct of 7-22 handle references is a side-grade, and `wire_source_routing`'s own reason asks for a pages bundle that does not exist yet — a strand of its own |
| `device_sync/device_sync_types.rs:73 replace_track` (10) | mirrors the platform-linux `replace_managed` request field by field, and that function's own suppression says it keeps every transport parameter explicit; the copy-request type must be designed in `reprise-platform-linux` first |
| `device_sync/device_sync_target_browser.rs:450 load_folders_if_current` (8) | request identity + generation token + two callbacks; wants the `Generation` pattern from #1114 generalised rather than a one-off struct |
| `scan/scan_worker.rs:116 reconcile_outcome` (8) | one caller; the five surfaces it updates are a `ScanSurfaces` bundle only inside this file — a judgement call left for the window-wiring strand |
| `issues/missing_menus.rs:75 show_row_menu` (9) | two capability bools plus a click point from a GTK gesture; bundling them buys little for two callers |
| the nine suppressions whose reason says the arguments stay explicit | not candidates by their own reason |

## Known traps

- **Lifetimes.** `TrackViewQuery<'a>` borrows; a caller that owned `String`/`Vec` locals keeps
  them alive for the call (they already do today). Do not introduce `.clone()` to satisfy the
  borrow checker — restructure the `let`s instead.
- **`RefCell` discipline** (the #1 panic class): building a view from `self.imp().state.borrow()`
  fields and then calling into the model re-enters; copy the values out in their own statements
  first, as the code already does.
- **Display tests** (`*_display_tests.rs`, `#[ignore]`) need the isolation recipe and one process
  per test; `scripts/check-display-tests.sh --rule-named` is the orchestrator's; you run the named
  tests you touched.
- **Clippy 1.99 vs 1.97**: an `#[expect]` that becomes unfulfilled on one but not the other —
  irrelevant here, you delete them; but a new suppression needs `reason = "…"`.
- **`significant_drop_in_scrutinee`** is on; no lock is touched here.
- **Attribute drift.** After B1-B5 the budget gate fails until B6 — commit B6 last; the PR is
  squashed, so intermediate red is acceptable, but every commit must compile.
- **Nothing foreign.** `add_dialog_followers.rs` is in a live branch's diff; `YoutubeFollowerRequest`
  is only moved by value into `AddOptions`, never edited.
- English everywhere, focused commits, no agent attribution lines.

## Verification

```
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo clippy --all-targets --workspace --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test -p reprise-gnome browse_filter_count
cargo test -p reprise-gnome track_list_model
cargo test -p reprise-gnome tag_mutation_refresh
cargo test -p reprise-gnome podcasts_groups
cargo test -p reprise-gnome add_dialog
cargo test -p reprise-gnome updates
# display tests, one process each, under the isolation recipe:
#   podcasts_sync_row_display_tests (3), add_dialog_chrome_tests (podcasts, 2), tag_mutation_refresh_block_move_display_tests
scripts/check-architecture.sh          # too-many-arguments suppressions: 19 (at budget)
scripts/check-frontend-thinness.sh     # unchanged numbers
scripts/check-accessibility-semantics.sh
scripts/check-ux-traceability.sh
```

Report: the ten sites with their new input counts, every caller file edited, the fate of the
`credentials` field (decision 4), and the budget line.
