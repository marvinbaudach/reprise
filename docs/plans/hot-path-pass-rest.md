---
slug: hot-path-pass-rest
worktree: /home/marvin/Projects/reprise-hot-path-pass-rest
branch: feature/hot-path-pass-rest
phase: planned
codex_session:
created: 2026-10-05
---
# Hot-path pass — the rest (#1080)

Finishes issue #1080, the items the hot-path pass (#1073, #1076, #1083) left open. Based on `dev` @
0322ae01df. Claude workers implement it, because the Codex quota is exhausted until 2026-10-09.

**The owner overrode the ownership hold (2026-10-05).** A3 and A9 touch files claimed by the unlanded
`feature/loudness-and-cue-sheets-r128`, which owns `reprise-android-ffi/**` and registers its own v88.
Whichever branch lands second renumbers or rebases. Keep every hunk minimal, so the rebase stays
mechanical.

A2 (the browse statements on `prepare_cached`) stays dropped. There is no measurement behind it.

## Strand Y — core and FFI (`feature/hot-path-pass-rest`)

### Y1 — batched source paths for Android play and queue (A3)

- **Core:** add `track_source_paths(db, ids: &[i64]) -> Result<HashMap<i64, PathBuf>>`.
  - It lives next to the `placeholders()` and `IN (...)` code in `crates/reprise-core/src/queries/queue.rs`.
  - Chunk the ids below SQLite's bound-variable limit, and dedupe them per chunk.
  - Export it through the existing `pub use queue::{…}` list in `queries/mod.rs`. That one-line edit
    is the only `mod.rs` change.
- **FFI:** `crates/reprise-android-ffi/src/playback_session/queue_boundary.rs::resolve_track_uris`
  resolves every requested id with ONE call to `track_source_paths`, instead of one
  `track_source_path` per id.
  - The `(index, id, uri)` result keeps its contract: the input order, the requested index, a
    missing id skipped while the indexes of the others are kept, and the same error mapping.
  - Apply the same change to `playback_session/trash_boundary.rs::trash_tracks`, if its per-id loop
    only resolves paths.
- **Tests first:** in core, test missing ids, duplicate ids, input order, and more ids than one chunk.
  The existing FFI tests are the behaviour proof: `play_track_ids_tests.rs`,
  `queue_boundary_tests.rs`, `trash_boundary_tests.rs` and `playback_writer_lock_tests.rs`. Do not
  edit them unless a test has to change, and then say why.

### Y2 — a `playlist_tracks(track_id)` index (A9)

- **Migration:** the next free version is 88. Add a new `crates/reprise-core/src/db_playlist_track_index.rs`
  with `migrate_v88`, modelled on `db_sort_indexes.rs` (`migrate_v82`).
  - It runs `CREATE INDEX IF NOT EXISTS` plus the `user_version` update inside
    `unchecked_transaction`, and is idempotent.
  - Register it in `db_migrations.rs` and declare the module in `lib.rs`.
- **Fresh databases:** check how a fresh database gets the v82 indexes, either through the baseline
  schema plus the migration replay or through `db_schema_baseline.rs`. Make sure a fresh database
  ends with the new index too.
- **Version asserts:** bump the asserts that pin 87, in `db_concerts_migration_tests.rs`,
  `db_device_sync.rs`, and any others found by grep.
- **Test first:** the migration creates the index, a second run is a no-op, and
  `EXPLAIN QUERY PLAN` for `SELECT … FROM playlist_tracks WHERE track_id = ?` uses it.

## Strand X — tag-edit membership (`feature/hot-path-pass-rest-tags`)

Files: `crates/reprise-gnome/src/ui/tag_edit/{tag_edit_flow,tag_save_refresh,tag_reload_anchor}.rs`
and their tests.

- **X1 (A10(b)):** the `tag_changed_paths` filter in `tag_edit_flow.rs` runs
  `report.updated_ids.contains` once per write. Change it to set membership, preferably by reusing
  `tag_save_refresh::tag_changed_ids`. `tag_edit_flow.rs` is at 794 of 800 lines and must not grow.
- **X2:** in `tag_save_refresh.rs::plan`, `writes.iter().find(..)` runs per updated id. Index the
  writes by id once.
- **X3:** in `tag_reload_anchor.rs::first_sort_key_write`, `writes.iter().any(..)` runs per updated
  id. Use a set or a map built once.
- The behaviour (order, the first match on duplicate ids) stays identical. Pin it with a test
  wherever the existing tests do not.

## Shared rules

- Behaviour-preserving. Write the test first wherever a test can pin something.
- Lint-clean under the lint wave (#1082):
  - a new suppression needs `reason`;
  - no new `too_many_arguments`;
  - no guard held in a `match` or `if let` scrutinee.
- Every code file stays below 800 lines. English everywhere, and one prose commit per task.

## Parallelität

X and Y have disjoint files, and each runs in its own worktree. X's commits are cherry-picked onto Y.
One review, one gate run and one PR follow.
