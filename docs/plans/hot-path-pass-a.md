---
slug: hot-path-pass-a
worktree: /home/marvin/Projects/reprise-hot-path-pass-a
branch: feature/hot-path-pass-a
phase: refactored
codex_session:
created: 2026-10-04
---
# Hot-path pass — strand A: scanner and playlist statements, FFI queue

Read the mother plan `docs/plans/hot-path-pass.md` first. Its shared rules,
out-of-scope list and file ownership bind this strand.

## Gate cadence for this run (overrides AGENTS.md "Gates before every commit")

AGENTS.md asks for the full gate battery before every commit. On this run that
rule is **overridden**, because the host is shared with other heavy runs.

- **Per commit:**
  - `cargo fmt --check`;
  - `cargo clippy -p <each touched crate> --all-targets -- -D warnings`;
  - `cargo test -p <each touched crate>`.
- **Once, before the final commit:** the full battery from AGENTS.md:
  - `cargo fmt --check`;
  - `cargo clippy --all-targets --workspace -- -D warnings`;
  - `cargo test --workspace`;
  - `cargo audit`. RUSTSEC-2024-0436 is the only accepted advisory; a new one means STOP and report;
  - the core purity check.

One commit per task below. Commit subjects are plain English sentences with no
`type:` prefix and no agent attribution trailers. Do not push.

## Tasks

### A1 — scanner statements are prepared once per connection
- **Files:**
  - `crates/reprise-core/src/library/scanner_entry.rs`: `known_row` near :121 and `upsert_track` near :409;
  - `crates/reprise-core/src/library/exclusions.rs`: `matches_file` near :37;
  - `crates/reprise-core/src/library/import_errors.rs`: `check_dismissed` near :188.
- **Change:** `conn.query_row`/`conn.execute` re-prepare on every call. Use
  `conn.prepare_cached(..)` and run the query on the cached statement.
  `known_row` keeps its behaviour: a prepare or query error yields the default
  `KnownRow`, so keep the prepare inside the same `.ok()` fallback chain.
- **Proof:** no new test. The scanner suite is the proof.

### A5 — playlist writes stop issuing one statement per row
- **Files:**
  - `crates/reprise-core/src/library/playlist_membership.rs` (near :28);
  - `crates/reprise-core/src/library/playlists.rs` (near :241, :407, and the reinsert in `move_position` near :501).
- **Change, `add_unique_tracks`-style membership check:** preload the playlist's existing
  `track_id`s into a `HashSet` with one query, instead of one `EXISTS` per input id.
  The existing order-preserving `seen` dedupe stays.
- **Change, insert loops:** hoist them onto one prepared or `prepare_cached` statement.
  **Keep the delete-then-reinsert shape**: shifting positions with `UPDATE` collides with
  the `(playlist_id, position)` primary key.
- **Test first:** adding a mix of new, already-present and repeated ids gives the same
  playlist as before, and the `trace_v2` count shows one membership SELECT per call.

### A6 — queue snapshots move instead of cloning
- **Files:**
  - `crates/reprise-android-ffi/src/playback_session/queue_persister.rs` (`persist`, near :122);
  - its call sites in `queue_boundary.rs`, `stream_events.rs` and `history.rs`.
- **Change, `persist`:** take `Queue` by value. Write the snapshot from `&queue`, then move
  it into `PendingSnapshot`. Call sites that already own the queue move it instead of
  cloning. Where a call site still needs the queue afterwards, it keeps one clone.
- **Change, `enqueue_tracks` (near :241):** stop rebuilding the whole index with
  `index_tracks(&state.track_ids)`. Extend the map for the appended ids only, with
  `entry(id).or_insert(old_len + i)`, which keeps `index_tracks`' first-index-wins rule.
- **Test first (index):** enqueueing ids that are already queued, plus repeats within the
  batch, yields a map equal to `index_tracks` over the full list.

### A7 — the spectrogram blob is moved, not copied
- **Files:** `crates/reprise-core/src/spectrogram.rs` (near :60), which gains
  `pub fn into_cells(self) -> Vec<…>`; `crates/reprise-android-ffi/src/track_analysis.rs:62`,
  which replaces `cells().to_vec()` with `into_cells()` where it owns the value.
- **Proof:** no new test.

## Deferred — collides with refactor-wave-2026-10

A parallel refactor wave, `docs/plans/refactor-wave-2026-10.md` on its own branches, owns
`reprise-core/src/queries/**` (its strand A) and `db.rs`/`db_migrations.rs` (its strand B,
which replaces the hand-written migration list). The following tasks touch those paths, so
they are **not part of this run**. They follow once that wave has landed:
- A2: `prepare_cached` in the browse queries;
- A3: the batched `track_source_paths` for Android play/queue;
- A4: renumbering each playlist once per delete;
- A8: the MCP page-collector refactor, because the wave's strand A also owns `crates/reprise-mcp/src/data.rs`;
- A9: the `playlist_tracks(track_id)` index as the next migration.

Do not edit any file under `crates/reprise-core/src/queries/`, `crates/reprise-core/src/db*.rs` or `crates/reprise-mcp/`.

## Done when

Every task is committed, or skipped with a stated reason, and the full battery is green.
The final summary (`.pipeline-codex.md`) lists per task:
- what changed structurally;
- the proof that ran;
- any skip and its reason;
- the exact gate commands and their results.
