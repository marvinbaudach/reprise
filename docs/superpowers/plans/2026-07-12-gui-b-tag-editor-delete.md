# GUI-B: Batch-Tag-Editor + Remove/Trash — Implementation Plan

**Goal:** Edit classic tags for one or many selected tracks without clobbering
per-track values, and provide explicit DB-only removal and safe trash workflows.

**Baseline:** 409 passed; 1 ignored. Core must remain free of GTK/GStreamer/zbus.
Every task runs fmt, strict workspace clippy, workspace tests, audit, purity, and
the <800-line gate before commit. No real music/database/trash in tests.

## Task 1 — Pure batch patch and mixed-value model

**Files:** create `crates/reprise-core/src/library/tag_edit.rs`; modify
`crates/reprise-core/src/library/mod.rs`.

Add `MixedValue<T>`, `EditableTags`, and `TagPatch` exactly as the design specifies.
`summarize(&[EditableTags]) -> Option<EditableTagSummary>` returns `None` for empty
input and Uniform/Mixed per field. `TagPatch::is_empty()` is true only when every
outer option is `None`.

RED tests:

```rust
#[test] fn summary_marks_only_differing_fields_mixed() { /* two rows, title differs */ }
#[test] fn empty_selection_has_no_summary() { assert!(summarize(&[]).is_none()); }
#[test] fn untouched_patch_is_empty_but_clear_is_not() { /* year Some(None) */ }
```

Commit: `feat: add pure batch tag patch and mixed-value model`

## Task 2 — Lofty patch writer preserves untouched metadata

**Files:** modify `tag_edit.rs`.

Add:

```rust
pub fn apply_patch_to_file(path: &Path, patch: &TagPatch) -> Result<(), TagEditError>;
pub fn read_editable_tags(path: &Path) -> Result<EditableTags, TagEditError>;
```

Use the existing tag if present, otherwise create the file's primary tag type.
For strings, empty means remove the corresponding item/accessor. For numbers,
`Some(None)` removes. Save with lofty `WriteOptions::default()`. Never replace the
whole tag and never touch pictures or unrelated items.

RED tests use a copied `tests/fixtures/sine.flac`:

```rust
#[test] fn patch_changes_only_dirty_fields_and_preserves_picture_and_custom_item();
#[test] fn numeric_patch_can_set_and_clear_year_and_track();
#[test] fn empty_patch_leaves_file_tags_unchanged();
```

Commit: `feat: apply selective tag patches with lofty`

## Task 3 — Batch worker core and scanner reconciliation

**Files:** modify `tag_edit.rs`.

Add `TagWriteFailure`, `TagBatchReport`, and:

```rust
pub fn apply_patch_batch(
    conn: &Connection,
    tracks: &[(i64, PathBuf)],
    patch: &TagPatch,
) -> TagBatchReport;
```

For each track: write, then `scanner::scan_folder(conn, path)`. Continue after
errors. Report exact successful ids and `(id,path,error)` failures. Empty patch and
empty selection are no-ops. RED tests prove one valid + one invalid path gives a
partial result and that rating/play_count survive the successful rescan.

Commit: `feat: batch tag writes with per-track results and DB reconciliation`

## Task 4 — General transactional DB removal

**Files:** modify `crates/reprise-core/src/queries/maintenance.rs` and tests.

Add:

```rust
pub fn remove_tracks(conn: &mut Connection, ids: &[i64]) -> Result<Vec<i64>, Error>;
```

Generalize the existing transactional playlist-compaction implementation. Keep
`remove_missing_tracks` as a guarded wrapper/variant so non-missing rows remain
protected there. RED tests cover arbitrary rows, duplicates/nonexistent ids,
gapless playlists, and rollback on injected failure.

Commit: `feat: transactional remove_tracks with playlist compaction`

## Task 5 — Safe trash batch with injectable filesystem action

**Files:** add `trash = "5"` to core; create
`crates/reprise-core/src/library/trash_tracks.rs`; modify library mod/Cargo.lock.

Add:

```rust
pub struct TrashReport { pub removed_ids: Vec<i64>, pub failures: Vec<TrashFailure> }
pub fn trash_tracks_with<F>(conn: &mut Connection, tracks: &[(i64, PathBuf)], trash: F) -> TrashReport
where F: Fn(&Path) -> Result<(), String>;
pub fn trash_tracks(conn: &mut Connection, tracks: &[(i64, PathBuf)]) -> TrashReport;
```

Call the injected action per file; pass only successes to `remove_tracks`. Production
uses `trash::delete`. RED tests use temp files and closures only—never the desktop
trash—and prove failures retain DB rows/files while successes compact playlists.
Audit and purity are mandatory immediately after dependency addition.

Commit: `feat: safe batch move-to-trash with DB cleanup`

## Task 6 — Tag editor dialog and dirty-field semantics

**Files:** create `crates/reprise-gnome/src/ui/tag_editor.rs`; modify `ui/mod.rs`,
`strings.rs`.

Build an `adw::Dialog` with seven `adw::EntryRow`s. Mixed values use the
`(multiple values)` placeholder and empty initial text. Connect dirty tracking only
after initial values are installed. Parsing year/track accepts empty=clear and
positive integer=set; invalid text prevents apply and shows an inline error.

Keep pure helpers testable:

```rust
fn string_patch(dirty: bool, text: &str) -> Option<String>;
fn number_patch(dirty: bool, text: &str) -> Result<Option<Option<u32>>, ParseFieldError>;
```

RED tests pin unchanged/mixed behavior, clear, set, and invalid numeric input.

Commit: `feat: multi-selection tag editor dialog with dirty-field patches`

## Task 7 — Context action, off-thread write worker, and refresh

**Files:** modify `track_list_context_menu.rs`, `track_actions.rs`, `track_list.rs`,
`window.rs`; create `tag_edit_worker.rs` if needed.

Add “Edit tags…” for non-empty track selections. Resolve selected IDs to full
editable DB rows/paths all-or-nothing. Open the dialog with a mixed summary. Apply
on one dedicated worker connection using `apply_patch_batch`; return only plain
report data to GTK. On completion reload list/sidebar, refresh current cover if
needed, and show plural success/failure toasts. No RefCell borrow crosses a callback.

Add `REPRISE_SMOKE_TAG_EDIT=title:<value>` that selects the first two scratch tracks,
drives the exact handler, and logs the report. Isolated E2E verifies both DB and file
tags changed while ratings remain distinct.

Commit: `feat: wire batch tag editor through context menu and worker`

## Task 8 — Remove/Trash confirmations and Delete shortcut

**Files:** create `delete_tracks.rs`; modify context menu, shortcuts, track list,
window, strings.

Add separate context actions “Remove from library…” and “Move to Trash…”. Both use
`adw::AlertDialog`; wording and destructive appearance must distinguish DB-only from
filesystem trash. Delete key opens the chooser for current selection. DB work/trash
runs off-thread with its own connection. Purge exact removed IDs from Queue, reload
list/sidebar, and report partial trash failures.

Permanent smoke hooks:

- `REPRISE_SMOKE_DELETE=db-only` on copied fixtures: DB rows removed, files remain.
- `REPRISE_SMOKE_DELETE=trash` only on files inside the command's scratch directory;
  rows removed and paths disappear. Never point at user files.

Commit: `feat: confirmed remove-from-library and move-to-trash workflows`

## Stage close-out

Run all gates, core standalone build/purity, isolated tag-edit + DB-only + scratch-trash
smokes, and whole-branch review. Manual checks: dialog copy/layout, mixed placeholders,
file-manager trash visibility. Record GUI-B complete and advance STATUS to GUI-C.

