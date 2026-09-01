---
slug: the-trash-run-lets-go-of-the-writer
worktree: /home/marvin/Projects/reprise-the-trash-run-lets-go-of-the-writer
branch: feature/the-trash-run-lets-go-of-the-writer
phase: refactored
codex_session:
created: 2026-09-01
---

# The trash run lets go of the writer

Android's move-to-trash holds the shared writer mutex for the whole batch,
including one SAF Binder round trip per file. Every other writing FFI call —
appearance, artist portraits, mobile sync, playback settings, listen reports —
waits behind the document provider. This plan takes the lock off the deletion
loop without weakening any guarantee the current code makes.

## What is actually true today

Measured on `origin/dev` @ `cae5527afa`. Two claims in
`docs/plans/the-repo-is-ready-to-show.HANDOFF.md`, which raised this item, are
wrong, and correcting them is what keeps the change small.

`crates/reprise-android-ffi/src/playback_session/trash_boundary.rs:53` takes
`self.inner.library.writer()` and keeps the guard alive for the whole first
block: the per-id `queries::track_source_path` resolution, and then
`trash_tracks_with`, whose loop in
`crates/reprise-core/src/library/trash_tracks.rs:36-70` calls the injected
action once per track. On Android that action is `MainActivity.kt:143-151`,
`DocumentsContract.deleteDocument` — a Binder round trip to the
DocumentsProvider for every file in the batch.

**It is not app-wide.** `MusicLibrary` holds two mutexes, `writer` and `reader`
(`crates/reprise-android-ffi/src/library_types.rs:26-27`), and the comment there
records that the reader is never held together with either. Reads keep running.
What blocks is the other 25 `writer()` call sites in that crate.

**The desktop does not share the pattern.** There is one production call site,
`crates/reprise-gnome/src/ui/delete_tracks.rs:255`, and `start_worker` at
`:196-200` opens its *own* `Db::open_migrated` on a worker thread. It holds no
shared handle at all. The other two `trash_tracks_with` uses in that file are
tests. Nothing on the desktop needs changing, and this plan does not change it.

The trash path is also the only place in the FFI crate that holds the writer
across a callback. The three `writer()` uses in `artist_portrait.rs` are all
under `#[cfg(test)]`; the production portrait backfill holds nothing across its
fetch.

## Why the split is safe — the guard is already in the transaction

The obvious worry is that dropping the lock during deletion widens the race that
`trash_tracks_with`'s pre-loop `SELECT path FROM tracks` guards against
("track path changed before trash; refusing stale request"). It does not,
because that check is not what makes the removal correct.

The removal goes through
`queries::remove_tracks_matching_paths_remembering_releases`
(`crates/reprise-core/src/queries/maintenance.rs:412`) with
`remember_deletion = true`, which lands in
`maintenance_delete::remove_path_requests_impl`. That function opens an
**IMMEDIATE** transaction and then runs `eligible_path_requests`
(`crates/reprise-core/src/queries/maintenance_delete.rs:137-158`), which
re-checks `SELECT 1 FROM tracks WHERE id = ?1 AND path = ?2` for every request
and silently drops the ones that no longer match. `trash_tracks_with` already
turns such a drop into a reported failure: "file was trashed but its database
row was not removed".

So path identity is re-verified under the write transaction, at the moment of
deletion, whatever happened while the file was being trashed. The pre-loop
check is an early refusal that saves an unnecessary SAF call — worth keeping,
worth nothing as a lock-scope argument.

## Task 1 — core: three phases instead of one

`crates/reprise-core/src/library/trash_tracks.rs` grows two public functions and
keeps `trash_tracks_with` as the composition of them.

```rust
/// Requests that still match a library row, and the ones that already do not.
pub struct TrashPlan {
    pub validated: Vec<(i64, PathBuf)>,
    pub failures: Vec<TrashFailure>,
}

/// Phase 1 — needs the database. De-duplicates ids and refuses stale paths.
pub fn plan_trash(db: &Db, tracks: &[(i64, PathBuf)]) -> TrashPlan;

/// Phase 3 — needs the database. `trashed` is what the caller's action
/// actually deleted; `failures` is what phase 1 and the action reported.
pub fn commit_trash(
    db: &Db,
    trashed: &[(i64, PathBuf)],
    failures: Vec<TrashFailure>,
) -> TrashReport;

/// Unchanged signature and unchanged behaviour: plan, act, commit, in one call
/// while the caller's `&Db` is held. The desktop keeps using this.
pub fn trash_tracks_with<F>(db: &Db, tracks: &[(i64, PathBuf)], trash_action: F) -> TrashReport
where
    F: Fn(&Path) -> Result<(), String>;
```

The bodies are the existing code cut in three at the two seams already present
in it — the validation loop, the action loop, the removal block. No behaviour
moves between phases, and **every failure string stays byte-identical**, so
nothing that matches on them changes.

`plan_trash` keeps taking `&[(i64, PathBuf)]`, not ids. Merging it with the
FFI's own `track_source_path` resolution would be a second, separately
reviewable change: it would collapse two failure strings that today name
different states (`ALREADY_GONE`, "this track was already gone from the
library", against "track path changed before trash"), and a review could then no
longer tell an intended wording change from collateral damage. The redundant
second read costs microseconds under the lock, against a Binder round trip — it
is not the problem this plan solves.

Phase 2 does not exist as a function. It is whatever the caller does between the
two calls, which is the entire point.

Commit: `refactor(core): split move-to-trash into plan, act and commit`

## Task 2 — Android: three scopes instead of one

`crates/reprise-android-ffi/src/playback_session/trash_boundary.rs`,
`trash_tracks`:

- **Scope A, writer held.** Resolve every `track_id` through
  `queries::track_source_path` as today, collecting `already_gone` for the ids
  whose row has vanished, then call `plan_trash`. Drop the guard at the end of
  the block.
- **Scope B, no lock.** Loop over `plan.validated`, calling
  `trash_path(action.as_ref(), path)` per entry and sorting the results into
  `trashed` and `failures`. This is where the Binder calls happen.
- **Scope C, writer taken again.** `commit_trash`, then the existing queue
  reconciliation and report assembly, unchanged.

The queue block below already takes `self.inner.lock()` in its own scope and is
untouched. Lock order is unchanged: this only ever holds one of the two at a
time, and it never holds `writer` while taking `inner`.

**This introduces one new failure mode, deliberately.** Scope C re-acquires the
writer, and `MusicLibrary::writer()` can fail with "library handle poisoned by
an earlier panic" — which today cannot happen between deletion and cleanup,
because the guard is never released there. When it does, the files are gone from
the device and their rows are still in the library. The decision is to
**propagate that error with `?`**, exactly like every other `writer()` failure in
this function: the caller gets a `LibraryError::Database`, not a wrong count, and
the next scan reconciles rows whose files have disappeared. A poisoned mutex
means a panic already ran somewhere under it; a prettily formatted partial report
is not the pressing problem in that state. Note it in the commit message so the
review reads it as a choice, not an oversight.

Commit: `fix(android): free the library writer during the trash callbacks`

## Task 3 — the test that proves it

`crates/reprise-android-ffi/src/trash_boundary_tests.rs` gets one test whose
control arm is the defect itself.

A `TrashAction` implementation holds a clone of the library's `Arc<Mutex<Db>>`
— obtained through `MusicLibrary::writer_handle()`, which is `pub(crate)` and so
reachable from this in-crate test module — and, on each `trash` call, records
whether `writer.try_lock()` succeeded. Seed **three** tracks and assert success
for every one of them, so a partial fix that frees the lock only after the first
file still fails.

`try_lock`, never `lock`. A blocking `lock()` from inside the callback does not
fail on today's code, it **hangs**: the callback runs on the thread that already
holds the guard, so it deadlocks against itself and takes the suite with it.
`try_lock` returns `Err(WouldBlock)` today and `Ok` after the change — red to
green, deterministic, no timing and no deadline.

What this does not cover is "the mutex is free but another thread still could
not get in". For a `std::sync::Mutex` that state does not exist, which is why a
second thread with a deadline — the realistic-looking alternative — would only
add a future flake.

Commit: `test(android): the writer is free while files go to the trash`

## Verification

In the code phase:

- `cargo test -p reprise-android-ffi -- --test-threads=1` — the new probe plus
  the three existing trash tests
  (`trash_tracks_reports_partial_failure_and_keeps_failed_database_rows`,
  `trashing_the_playing_track_advances_plays_and_removes_it_from_upcoming`,
  `trashing_the_last_playing_track_stops_playback`).
- `cargo test -p reprise-core -- --test-threads=1` — the two existing
  `trash_tracks` tests must pass untouched. That is what proves the wrapper kept
  its behaviour.
- `cargo test -p reprise-gnome -- --test-threads=1` — the desktop path is not
  edited; this is the control arm for that claim.
- `cargo check --locked --workspace --all-targets`, `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`. The workspace check earns its
  place here: it is what catches a dependency that is gone while still in use,
  the break that reached the merge in `cd8ac6229e` because only a suite run
  would have shown it.

Once, before landing: `cargo test --locked --workspace -- --test-threads=1`.
Serial is mandatory — the suite is known flaky in parallel in
`reprise-platform-linux`, `reprise-core::podcasts::ytdlp` and
`reprise-android-ffi`.

`scripts/check-architecture.sh` is red on clean `origin/dev`
(`crates/reprise-platform-linux/src/device_sync.rs`, 831 lines) and stays red for
that reason alone. Anchor on the rule identity, not the exit code. Both edited
source files are far below the 800-line ceiling and must stay there.

Not attempted: an on-device measurement of the blocked interval. The property
under test is "the writer is free during the callback", which the unit test
states directly; a wall-clock figure from a software-GPU emulator would say
nothing about a phone.

## Parallelität

**This plan is not cut into strands.**

The one conceivable cut — core in one strand, the Android boundary and its tests
in the other — produces genuinely disjoint file sets, and is still wrong: the
Android strand calls `plan_trash` and `commit_trash`, which do not exist in its
worktree. It cannot compile, so it cannot test, so its Codex run ends correctly
with finished work it is not allowed to commit. That is the failure measured on
2026-08-11 in the Flathub wave, only harder — there a comparison value was
missing, here the API is. Papering over it by having the core strand land stubs
first would put both strands in the same file and destroy the disjointness that
motivated the cut.

Size argues the same way: three files, roughly 150 changed lines. Two worktrees
with a `cargo` build each cost more wall-clock here than they save.

The changed-file set:

- `crates/reprise-core/src/library/trash_tracks.rs`
- `crates/reprise-android-ffi/src/playback_session/trash_boundary.rs`
- `crates/reprise-android-ffi/src/trash_boundary_tests.rs`

Merge order: not applicable. Post-merge cross-checks: none — every verification
step above reads only files this strand owns.
