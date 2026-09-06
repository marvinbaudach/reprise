---
slug: responsive-editing-and-one-table-grammar-a
worktree: /home/marvin/Projects/reprise-responsive-editing-and-one-table-grammar-a
branch: feature/responsive-editing-and-one-table-grammar-a
phase: reviewed
codex_session:
created: 2026-09-05
---
# Strand A — `edits`: the tag editor and the delete path stop stalling

Strand A of `docs/plans/responsive-editing-and-one-table-grammar.md`. Read the
mother plan's §0 (measurements), §1 (goals G1–G5) and §2 (rules) first; §2
binds every task here. Branch
`feature/responsive-editing-and-one-table-grammar-a`.

Five tasks, in order. A0 lands first because A2–A4 are judged by its fields
after landing (mother §6).

## File ownership

- Owns: `crates/reprise-gnome/src/ui/tag_edit/**`,
  `crates/reprise-gnome/src/ui/delete_tracks*.rs`,
  `crates/reprise-gnome/src/ui/window/window_action_wiring.rs`,
  `crates/reprise-platform-linux/src/trash.rs`,
  `crates/reprise-core/src/library/trash_tracks.rs`.
- Reads but never edits: `ui/track_list/**`, `ui/cover/**`,
  `ui/browse/browse_bar.rs` (calls `refresh()` only),
  `ui/playback/queue_transport.rs`, `ui/sidebar/sidebar.rs`.
- Does not edit `docs/ux-rules.md`. The MOT-6 test in A1 is mapped by its
  name alone.

## Task A0 — the two paths report their phases

**Files.** `crates/reprise-gnome/src/ui/tag_edit/tag_edit_flow.rs`,
`crates/reprise-gnome/src/ui/tag_edit/tag_editor.rs`,
`crates/reprise-gnome/src/ui/delete_tracks.rs`.

Add `info`-level `tracing` lines with millisecond fields, measured with
`std::time::Instant`:

- `tag_edit_flow.rs:311` / `:328`: promote "tag editor presented" to `info` and
  give it `build_ms` (from the `present()` entry to `dialog.present`) and
  `tracks`.
- `tag_edit_flow.rs:481` (worker result): `write_ms` (from `spawn_save` to the
  result), `tracks`; `finish_apply` (`:504`) ends with `reload_ms` and
  `delta: bool` (Task A2 makes it true).
- `delete_tracks.rs:186` (`start_worker`): "delete confirmed" with `tracks`,
  `mode`; `finish` (`:378`): `worker_ms` (confirm → result) and one field per
  step — `mutated_ms` (the `on_library_mutated` callback), `advance_ms`,
  `browse_bar_ms`, `reload_ms`; the existing "delete batch completed" line
  (`:401`) gains `main_thread_ms` as their sum.

Keep the field names exactly as written; the post-merge measurement greps for
them. No test beyond compile and the gates: these are log lines. Do not add a
timing helper module — three `Instant::now()` pairs do not justify one.

## Task A1 — the tag editor's cover comes through the loader

**Files.** `crates/reprise-gnome/src/ui/tag_edit/tag_editor_widgets.rs`,
`tag_editor_form.rs`, `tag_editor.rs`, `tag_edit_flow.rs`.

`build_cover_area(tracks, is_multi)` (`tag_editor_widgets.rs:38`) returns a box
whose `gtk4::Picture` shows the placeholder immediately; it no longer calls
`cover::resolve_source`, `cover::thumbnail` or `gdk::Texture::from_filename`.
`present()` (`tag_editor.rs:79`) takes `cover_loader: &Rc<CoverLoader>` — both
call sites in `tag_edit_flow.rs` (`:311`, `:360`) have `shared.cover_loader`
(`track_list.rs:197`) — and after building the form calls
`cover_loader.load_into_picture(&picture, first_track_path, ThumbnailSize::Grid,
token, &current, |_| {})` with a fresh `Rc<Cell<u64>>` generation the dialog
owns. The cache hit case (the row's cover was just shown in the list) paints on
the next main-loop iteration; the miss case decodes in `spawn_blocking` as the
list does.

Tests, written first:

- `tag_editor_widgets.rs`: `build_cover_area_returns_a_placeholder_without_touching_the_file`
  — a fixture path under `tempfile` with a 20 MB junk "cover" next to it; assert
  the returned picture has no paintable and that the call returns in under
  50 ms (upper bound generous on purpose; the point is "no decode", not a
  budget).
- Display test (ignored, xvfb): `mot_6_tag_editor_presents_before_its_cover_io`
  — present the dialog for a track with an embedded cover, assert the dialog is
  mapped before the picture has a paintable, then iterate the main loop until
  the paintable arrives. The name carries MOT-6 so `check-ux-traceability.sh`
  maps it; no edit to `docs/ux-rules.md` is needed or allowed.

Moves: `build_ms` (A0). Expect the cover I/O share of it to vanish; the widget
construction stays.

## Task A2 — a tag save reloads by delta

**Files.** `crates/reprise-gnome/src/ui/tag_edit/tag_edit_flow.rs`,
`tag_save_refresh.rs`, `tag_reload_anchor.rs`.

`finish_apply` already holds `OpenedReloadState { anchor, view_ids }` from before
the dialog opened. After the write, fetch `after_ids = shared.current_view_ids()`
once, compute `track_list_model_change::changed_range(&view_ids, &after_ids,
&written_ids, generation)` and call `reload_with_anchor_and_viewport(shared,
&anchor, ReloadViewport::PreserveAnchor, change, Some(after_ids))` — the call
shape `delete_tracks.rs:348-354` uses. `changed_range` returns `None` when the
edit moved rows (sort field or membership changed); that case keeps today's
full reload. Whatever `refresh_after_tag_mutation_with_anchor` does beyond the
reload (playing-marker, selection) must stay; read it before cutting.

Tests, written first, in `tag_edit/` (unit, no display):

- `tag_save_with_unchanged_order_reloads_by_delta` — seeded DB, edit the
  `comment` of two of five tracks, assert the reload received `Some(ModelChange
  { removed: 2, added: 2, .. })` at the right position (expose the decision as a
  pure function `tag_save_model_change(before, after, written, generation)` so
  the test needs no GTK).
- `tag_save_that_changes_the_sort_field_falls_back_to_a_full_reload` — rename
  the artist of one track under artist sort; assert `None`.

Moves: `reload_ms`, `delta` (A0).

## Task A3 — one trash session per batch

**Files.** `crates/reprise-platform-linux/src/trash.rs`,
`crates/reprise-core/src/library/trash_tracks.rs` (signature only if needed),
`crates/reprise-gnome/src/ui/delete_tracks.rs`.

`trash.rs` gains `pub struct Session` with `pub fn open() -> Result<Session,
String>` and `pub fn delete(&self, path: &Path) -> Result<(), String>`. Host
backend: `Session` is empty and `delete` keeps calling `trash::delete` (the
`trash` crate v5, per-file, cheap). Portal backend: `open` creates the
`zbus::blocking::Connection` and the `Proxy` once; `delete` reuses them. The
free function `delete(path)` stays as `Session::open()?.delete(path)` for the
other callers. `delete_tracks.rs:255` opens one session per batch and passes
`|p| session.delete(p)` into `trash_tracks_with`, whose `Fn(&Path)` signature
does not change. Per-file failures keep their `TrashFailure { id, path, error }`.

Tests, written first:

- `trash.rs`: `host_session_deletes_each_path_and_reports_per_file_failures`
  (tempdir; one existing file, one missing path; assert one `Ok`, one `Err`).
  The portal branch has no bus in the test environment; keep it behind the
  existing backend selection and cover it by the display-free unit test of the
  session's *construction* (`Session::open` on the host backend returns `Ok`).
- `trash_tracks.rs`: the existing tests stay; add
  `trash_tracks_with_calls_the_action_once_per_validated_path`.

After the `reprise-core` change: the purity proof from mother §2 must print
nothing.

Moves: `worker_ms` (A0) on the Flatpak backend only. On the host backend this
task is defence-in-depth; say so in the commit.

## Task A4 — the rows leave first, the sidebar follows

**Files.** `crates/reprise-gnome/src/ui/delete_tracks.rs`,
`crates/reprise-gnome/src/ui/window/window_action_wiring.rs`.

Reorder `finish` (`delete_tracks.rs:378`): keep `player.purge_queue_ids` and
`advance_after_user_catalog_delete` before the reload — BROWSE-11 depends on the
glide destination being set before the anchor restore — but move the
**sidebar refresh** and `browse_bar.refresh()` into a single
`glib::idle_add_local_once` scheduled *after* `reload_after_catalog_delete` and
the toast. Concretely: split the `on_library_mutated` closure at
`window_action_wiring.rs:320` into its two halves — the purge stays synchronous,
the sidebar refresh becomes the deferred part — or give `finish` a second
callback slot; pick the one that keeps `set_on_library_mutated`'s type for the
other caller (`grep set_on_library_mutated`). The deferred half must still run
exactly once even if the window closes in between (weak refs, as the closure
already does).

This task calls `sidebar.refresh` and `browse_bar.refresh` from a different
place; it does not change either method. `queue_transport.rs` and `sidebar.rs`
are read-only for this strand.

Tests, written first:

- `delete_tracks.rs` unit: `finish_orders_reload_before_the_deferred_refreshes`
  — record the call order through test hooks on `Shared` (the file already
  carries five unit tests with a fake `Shared`); assert
  `[purge, advance, reload, toast, sidebar_refresh, browse_bar_refresh]`.
- Existing `browse_11_*` and `browse_7_*` tests in `delete_tracks.rs` and the
  display tests in `delete_tracks_display_tests.rs`,
  `delete_tracks_large_block_display_tests.rs`,
  `track_list/delete_follow_display_tests.rs` stay green — run each one
  individually under `xvfb-run -a`.

Moves: `mutated_ms` and `browse_bar_ms` leave the pre-reload sum; the
expectation is G3, measured after landing (mother §6).

## Acceptance for strand A

- Tests and gates green (mother §2), including each display test named in A4
  run individually.
- Every A0 field is present in the journal on the first real run after landing
  — the orchestrator checks this in mother §6, step 3.
- G3 is an expectation, not a gate: this strand lands on green tests; the
  numbers are read afterwards and a miss opens a follow-up plan.

## Abort criteria

- If `delete_follow_display_tests` goes red under A4 and the fix would touch
  `track_list_reload.rs`, stop and report; that file belongs to the
  list-geometry service and is owned by nobody in this plan.
- If any task needs a file outside the ownership list above, stop and report
  instead of editing it.

## Post-merge follow-ups

- **Review finding 8 — replace the stale scan-watcher source-order guard.**
  `crates/reprise-gnome/src/ui/scan/scan_watcher.rs` is outside this strand's
  ownership, so its
  `catalog_deletion_has_its_own_sidebar_refresh_before_queue_purge` test remains
  unchanged here. The test only compares lexical positions inside
  `on_library_mutated`; after the delete-specific deferral was scoped out of
  that callback, it covers ordinary mutation callers but still says nothing
  about the confirmed-delete runtime order. Replace it with a runtime-order
  check (or rename and narrow it to the ordinary callback contract) so the
  confirmed-delete guard explicitly observes purge and reload before the
  deferred sidebar and browse-bar refreshes.
