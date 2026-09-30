---
slug: android-queue-boundary-test-isolation
worktree: /home/marvin/Projects/reprise-android-queue-boundary-test-isolation
branch: feature/android-queue-boundary-test-isolation
phase: shipped
codex_session:
created: 2026-09-23
---
# Android queue-boundary tests stop opening a competing writer

Two tests in `crates/reprise-android-ffi/src/queue_boundary_reorder_tests.rs`
open a second *writing* SQLite connection while an `AndroidPlaybackSession` is
still alive. Under CI load that write loses the library write lock for longer
than the 5 s `busy_timeout` and the test panics. It turned the dev gate red on a
docs-only commit. This plan removes the second connection: after it, the delete
runs on the very connection every session writer already shares, so there is
nothing left to contend with.

Base: `origin/dev` at `89806ab44e`.

## The evidence

CI run **35803754431, attempt 1**, commit `f496c8cee3` — "Curate the release
notes for 0.1.217 (#1015)", which changed `CHANGELOG.md` and the metainfo XML
and no code. Attempt 2 of the same run, same commit, nothing changed in
between, was green. A flake, not a defect in that commit.

Two tests failed in the same binary run (271 passed, 2 failed, 77.02 s):

| Test | Panic site | Failing call |
| --- | --- | --- |
| `live_deleted_upcoming_track_is_pruned_and_the_last_window_terminates` | `queue_boundary_reorder_tests.rs:66:10` | `remove_tracks_matching_paths(…).unwrap()` |
| `pruning_a_live_deleted_duplicate_keeps_the_loaded_current_slot` | `queue_boundary_reorder_tests.rs:120:6` | `remove_tracks_matching_paths(…).unwrap()` |

Both with the identical error:

```
SqliteFailure(Error { code: DatabaseBusy, extended_code: 5 }, Some("database is locked"))
```

What that pins down:

- **The write fails, not the open.** `Db::open_ready(…)` succeeded at both
  sites; the panic is on the delete that follows it.
- **`extended_code` is 5 — plain `SQLITE_BUSY`.** Not 517
  (`SQLITE_BUSY_SNAPSHOT`), which is what a deferred read→write upgrade returns
  and which fails *instantly*, ignoring the busy handler. Code 5 means the busy
  handler ran and gave up, so the write lock really was unavailable for the full
  `DEFAULT_BUSY_TIMEOUT_MS` = 5000 (`crates/reprise-core/src/db.rs:32`).
- **Both failures are one shape:** live session, second connection, write.

## Why a second connection is the wrong thing for these tests to have

`MusicLibrary::open` creates exactly **one** writing connection and shares it as
`Arc<Mutex<Db>>` (`crates/reprise-android-ffi/src/lib.rs:97`, handed out by
`library_types.rs:67` as `writer_handle()`). Everything the session starts in
the background takes that same handle:

- `QueuePersister::spawn(…, library.writer_handle(), …)` — `playback_session.rs:451`
- `PlayRecorder::spawn(…, library.writer_handle(), …)` — `playback_session.rs:471`

A sweep of every non-test `Db::open_*` in the crate found no other writer:
`lib.rs:103` is the read handle, and `artist_portrait.rs:346` opens the **same**
library file again — not a separate consent database, as an earlier draft of
this plan claimed. It is write-capable (`Db::open_ready`, not the read-only
variant) and only ever reads, and it is reachable only through
`MusicLibrary::start_artist_portrait_backfill`, never from
`AndroidPlaybackSession::new` or `play_tracks`. So that site is isolated by call
graph, not by file — do not cite it as evidence of the latter.
`PlayRecorder`'s `database_path` argument opens a `PlayJournal` **file**, not a
connection (`play_recorder.rs:217`). So no background thread started by
`AndroidPlaybackSession::new` or by `play_tracks` opens a database of its own.

The third worker is the near-miss worth naming, because its name says otherwise:
`ListenExportRecorder::spawn` takes `library.reader_handle()`, and its Drop calls
itself "the Android listen-export **writer** thread" — and `open_ready` is a
read-write open, so nothing in the type system would stop it writing. It does
not: `write_queued_listens` holds the reader only for a `device_path_for_track`
lookup, drops the guard, and then writes a journal **file** through
`listen_export_journal::record_listen(database_path, …)`
(`listen_export_recorder.rs:74-115`). The reader connection never writes the
database.

Which makes the production picture plain: **on Android the app process holds
exactly one writing connection**, so two writers can never contend there. The
contention in run 35803754431 existed only because the test added a third
connection that production has no counterpart for. Deleting through
`writer_handle()` therefore does not weaken the test — it makes it model the
real Android write path, where every library write goes through that one handle.

Both background writers also *retry* on busy rather than failing:
`QueuePersister`'s `RETRY_BACKOFFS`/`RETRY_WAKEUP` loop (`queue_persister.rs:17-23`)
and `PlayRecorder`'s `with_busy_retries` (`play_recorder_retry.rs`, four attempts
with exponential backoff). After `play_tracks` there are two such loops re-offering
writes to the shared connection. The test's third connection is the outsider that
loses.

## What stays unexplained

Nothing identified **which** writer held the SQLite write lock past 5 s, or why
for that long. The CI log does not say, and the event fired once in the whole
2026-09-21…23 window, so there is no repro to instrument.

The fix does not need the answer: after this change the test's delete runs on the
same connection as every session writer, serialised by the `Mutex<Db>` before
SQLite ever sees it. Whichever writer was holding the lock, it cannot contend
with the delete any more, because it *is* the delete's connection.

No production issue is filed for the unexplained hold. On Android the process has
one writing connection, so self-contention of this shape cannot occur there. If a
future change gives the Android process a second writing connection, this note is
the thing to re-read.

## Non-goals

- **Raising `busy_timeout` anywhere.** That widens the window the flake needs
  instead of closing it.
- **Changing `queue_persister.rs`, `play_recorder*.rs` or any commit path.**
  The defect is the test's extra connection, not the product.
- **Touching `crates/reprise-core`.** `remove_tracks_matching_paths` and
  `DEFAULT_BUSY_TIMEOUT_MS` stay exactly as they are. A task that ends up
  editing core means the plan was wrong, not that the plan grew.
- **Adding a `scripts/` guard.** `AGENTS.md` gives `scripts/` to the Flathub
  strand A; the accessor's doc comment carries the rule instead.
- **The other second-connection sites.** All were classified and none is in this
  class. Recorded here so nobody re-derives the sweep:
  - `queue_boundary_reorder_tests.rs:189` and `trash_boundary_tests.rs:207` —
    **reads while the session is live**. A concurrent reader is exactly what WAL
    is for; readers never contend with the writer.
  - `playback_tests.rs:523` and `queue_persistence_boundary_tests.rs:85`
    and `:216` — **after `drop(session)`**. That is a genuine quiesce: all three
    workers join on drop (`queue_persister.rs:207`, `play_recorder.rs:188`,
    `listen_export_recorder.rs:63`), so it does not matter whether the site reads
    or writes. `playback_tests.rs:523` belongs here and not in the bullet above:
    it is safe because the session is torn down first, not because it is a read.
  - `queue_persistence_boundary_tests.rs:202` — **before the session exists**;
    it is seeding, so there is nothing to contend with yet.

## Tasks

### T1 — a test-only writer accessor on the session

In `crates/reprise-android-ffi/src/playback_session.rs`, extend the existing
`#[cfg(test)] impl AndroidPlaybackSession` block at lines 693-697 (the one that
already holds `flush_queue_persistence`) with:

```rust
/// The one writing connection every session writer shares — the queue
/// persister and the play recorder both hold this exact handle.
///
/// A test that mutates the library while this session is live must write
/// through it, never through a `Db::open_ready` of its own: a second writing
/// connection contends with those background writers for the SQLite write
/// lock, and losing that race past `DEFAULT_BUSY_TIMEOUT_MS` is a
/// `DatabaseBusy` panic rather than a wait. Android production has exactly
/// one writing connection per process, so this is also the faithful shape.
pub(crate) fn library_writer(&self) -> Arc<Mutex<Db>> {
    self.inner.library.writer_handle()
}
```

Add only the imports this needs. The file goes from 723 to roughly 728 lines —
under the 800-line cap, and nothing else in it moves.

### T2 — the two tests delete through that handle

In `crates/reprise-android-ffi/src/queue_boundary_reorder_tests.rs`, rewrite the
database section of `live_deleted_upcoming_track_is_pruned_and_the_last_window_terminates`
(currently lines 56-69) and of `pruning_a_live_deleted_duplicate_keeps_the_loaded_current_slot`
(currently lines 111-121):

```rust
let writer = session.library_writer();
session.flush_queue_persistence();
let removed = {
    let database = writer.lock().unwrap();
    reprise_core::queries::remove_tracks_matching_paths(
        &database,
        &[(track("Deleted").id, PathBuf::from(&track("Deleted").path))],
    )
};
assert_eq!(removed.unwrap(), vec![track("Deleted").id]);
```

Note what the block returns: the `Result`, **not** `.unwrap()`'d. Unwrapping
inside the guard is the very thing point 3 below forbids. The `assert_eq!` on the
returned ids belongs to the first test only —
`pruning_a_live_deleted_duplicate_keeps_the_loaded_current_slot` never asserted
on the delete's return value and should keep only the `.unwrap()`, outside the
block, with no `assert_eq!` around it.

Four properties the shape has to keep, each for its own reason:

1. **No `Db::open_ready` and no `database_path` local** in either test. That is
   the whole fix.
2. **`flush_queue_persistence()` before the lock.** Not load-bearing — these
   tests read the queue from in-memory `state.queue` and only row metadata from
   the reader connection (`playback_session/queue_boundary.rs:56-95`), so a
   pending queue commit cannot change what they assert. It is kept because it
   makes `writer.lock()` uncontended and because the sibling test at line 187
   already does it; a reader who sees one and not the other will wonder why.
3. **The delete inside the block; every `unwrap` and `assert` outside.** A panic
   inside the guard would poison the `Mutex<Db>` that the session's
   still-running workers hold, turning one clear test failure into a second,
   unrelated-looking one during teardown. The block returns the `Result`.
4. **The guard released before any `session.*` call.** An explicit block, not a
   `drop(database)` — let the compiler enforce it. The later
   `session.upcoming_tracks(…)` and `session.snapshot(…)` assertions are
   unchanged and run after the block.

Both tests keep their names, their assertions and their meaning. This task
changes *how* the row is deleted, never *what* is proven.

### T3 — evidence

Three pieces, in this order:

1. **By construction** — the load-bearing one:

   ```bash
   grep -n "Db::open_" crates/reprise-android-ffi/src/queue_boundary_reorder_tests.rs
   ```

   Must print **exactly one** match: the `Db::open_ready` at roughly line 189,
   inside `moving_and_removing_identity_checked_rows_changes_the_next_window`.
   That one is correct and stays: it asserts what the *next process* would load,
   so it must read through a connection that took no part in the write, and it
   only reads.

2. **Regression** — 20 consecutive runs of the touched tests, to catch a
   deadlock or an ordering regression this change could itself introduce:

   ```bash
   fails=0
   for i in $(seq 1 20); do
     cargo test --locked -p reprise-android-ffi queue_boundary \
       >> target/queue-boundary-runs.log 2>&1 || fails=$((fails + 1))
   done
   echo "fails=$fails"
   ```

   Expect `fails=0`. The log goes under `target/` — gitignored, and inside the
   worktree the sandbox can write. **Do not use `$SCRATCH`**: it is unset in the
   Codex worktree, so the redirect would expand to `/queue-boundary-runs.log`,
   fail, and leave a missing-file `grep` looking like a clean zero. Report the
   counter, not a `grep` over a file that may never have been created, and never
   read the log back with `cat`.

3. **The CI log above**, which is the measurement of the mechanism and is
   already in hand.

**What must not be claimed:** that the flake was reproduced locally and then
went away. It was not reproduced locally and no attempt is planned — it fired
once in three days of CI, so a local loop that comes back green settles nothing.
The proof is that the competing connection no longer exists, not that a race was
observed losing. Write the report that way.

## Gates

From the worktree root, all four, before the commit:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
```

`cargo test --workspace`, not bare `cargo test`. `RUSTSEC-2024-0436` (`paste`,
via `lofty`) is the only accepted advisory; a new one stops the task. Then
`scripts/check-merge-readiness.sh` on a clean integration worktree, per the
Definition of Done — and if the sandbox cannot run that wrapper, run its gates
directly and record which check was unavailable.

No `reprise-core` change, so the core-purity `cargo tree` proof does not apply.
Both files are test or test-only code, so there is no UX rule to add and no
`docs/ux-rules.md` change.

## Ownership

Checked against `AGENTS.md` at `89806ab44e`:

- "Active file ownership — multi-surface frontends" owns `crates/reprise-view/**`,
  not `reprise-android-ffi`.
- "Android Now Playing scene" is COMPLETE and released; its P1 owned only
  `crates/reprise-android-ffi/src/track_analysis.rs`, untouched here.
- "Library Doctor fix round 3" was released on `origin/dev` by `89806ab44e`
  ("The Library Doctor fix round 3 ownership is released (#1016)") and never
  named an `android-ffi` path.

No strand owns `playback_session.rs` or `queue_boundary_reorder_tests.rs`.
Nothing to rebase onto.

## Parallelität

**One strand. The cut was attempted and does not pay.**

There are two files and they are not independent: T2 does not compile without
T1's accessor, and T3's verification reads both. Splitting them would mean a
second worktree with its own `target/`, a second full `cargo test --workspace`,
and a merge order for a change of roughly thirty lines — more wall-clock spent
than saved, which is the criterion. The cut is for wall-clock, not for agent
count.

**File ownership (single strand `android-queue-boundary-test-isolation`):**

- `crates/reprise-android-ffi/src/playback_session.rs`
- `crates/reprise-android-ffi/src/queue_boundary_reorder_tests.rs`

**Merge order:** not applicable — one branch.

**Post-merge cross-checks:** none. Every step in T1–T3 reads only files this
strand owns.

One thing must *not* be inferred from a green post-merge CI run: that the flake
is gone. Its base rate is roughly one occurrence per several dozen runs, so a
single green dev run is consistent with the bug still being there. The
by-construction grep in T3 is the proof; CI is confirmation, and the absence of
this panic over the following weeks is the slower, weaker corroboration.
