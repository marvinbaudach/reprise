---
slug: android-queue-persistence-never-waits-for-the-writer
worktree: /home/marvin/Projects/reprise-android-queue-persistence-never-waits-for-the-writer
branch: feature/android-queue-persistence-never-waits-for-the-writer
phase: planned
codex_session:
created: 2026-09-15
---
# Android: queue persistence never waits for the library writer

## Problem

Two ANRs on the Pixel 10 Pro XL (app 0.1.128, 2026-09-03 and 2026-09-14,
`dumpsys dropbox --print data_app_anr`, "Input dispatching timed out") share one
main-thread stack:

```
main:      AndroidPlaybackSession::next → SessionInner::persist_queue
           → MusicLibrary::writer → std Mutex::lock_contended
Thread-21: MusicLibrary::scan → SafSource.listChildren   (holds `writer` for the
           whole SAF walk, crates/reprise-android-ffi/src/lib.rs:171, inside one
           SQLite transaction, scanner.rs `unchecked_transaction`)
```

Every queue mutation — tap on a track, next/previous, shuffle, repeat, enqueue,
trash, and the **automatic advance at the end of a track** (`stream_events.rs:181`,
raised on Media3's application thread = main thread) — goes through
`persist_queue` (`playback_session.rs:339`), which takes `library.writer()`
synchronously. During a scan of a large tree that lock is held for minutes;
Android kills the app after 5 s.

`play_recorder.rs` solved the identical problem for play counts (its module doc
names the scan transaction explicitly) with a journal plus a background writer.
Queue persistence never got the same treatment.

Red regression test, already written and verified red (`next() waited 1.50s
for the writer`): `playback_writer_lock_tests.rs`, kept outside the shared
checkout at
`/tmp/claude-1000/-home-marvin-Projects-reprise/defed149-1d7d-43af-8217-15881c5e250a/scratchpad/playback_writer_lock_tests.rs`.
**Code phase, before Codex starts:** copy it to
`crates/reprise-android-ffi/src/playback_writer_lock_tests.rs` in the worktree
and register it in `playback.rs` right after `reader_lock_tests`:

```rust
#[cfg(test)]
#[path = "playback_writer_lock_tests.rs"]
mod writer_lock_tests;
```

`cargo test -p reprise-android-ffi writer_lock_tests` must be red before task 1
and green after task 5.

## Goal

No transport call ever blocks on the library writer. The queue still survives
process death — Android removes the task routinely (`REMOVE TASK` in
`exit-info`), so "persist later" means "persist to a file now, to SQLite
later".

Kotlin does not change; the FFI surface does not change (no new `pub` uniffi
items, bindings stay byte-identical).

Only `ReprisePlaybackService` creates an `AndroidPlaybackSession`
(`ReprisePlaybackService.kt:149`), so there is exactly one persister per
process and its sequence numbers are unique.

## Decisions (grilled 2026-09-15)

1. **Durability through a file**, not a deferred-only write: the last queue
   position must survive a kill during a scan.
2. **A single-slot snapshot file**, not a generalised play journal: only the
   latest queue matters.
3. **The worker takes the writer with `try_lock` + backoff**, never `lock()`,
   so it is joinable within one backoff step and `close()` stays quick.
4. **Restore prefers the file** without any timestamp comparison; the
   invariant "file exists ⇒ file is at least as new as the DB" holds by
   construction (the file is removed only after a commit, and only when its
   sequence still matches).
5. **`#[cfg(test)] flush()`** is the test seam for the drain; no sleeps, no
   synchronous test-only write path.
6. **The scan lock stays out of scope** (follow-up, see below).
7. **One strand.**

## Design

### 1. Snapshot file: `queue_snapshot_file.rs` (new)

Sibling of `play_journal.rs`, same directory (`library.database_path` parent),
same discipline (temp file → fsync → rename, a `.lock` companion, damage is
discarded with a `tracing::warn!`, never propagated).

- File `android-queue-snapshot.v1`. One record: `sequence: u64` + the queue,
  encoded with the same serde type `session::save` uses for the queue inside
  `ui.session.v1` — no second wire format. The record carries that type's
  version tag; a mismatch counts as damage.
- `write(sequence, &Queue)`: atomic replace. One small file + fsync,
  single-digit ms on flash; the play journal already pays this per play on
  the same thread.
- `read() -> Option<(u64, Queue)>`: `None` when absent or damaged.
- `remove_if_sequence(sequence)`: delete only when the file still carries that
  sequence, under the lock file — a newer snapshot written during the drain
  survives.

Invariant: **when the file exists, its queue is at least as new as the DB's.**

### 2. `QueuePersister`: `queue_persister.rs` (new), shaped like `PlayRecorder`

```
struct QueuePersister {
    pending: Option<Sender<PendingSnapshot>>,   // sequence + Queue
    shutting_down: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    next_sequence: AtomicU64,
}
```

- `spawn(database_path, library.writer_handle())`.
- `persist(&Queue)`: `sequence = next_sequence.fetch_add(1)`, write the
  snapshot file **synchronously**, then `send` to the worker. Never touches
  the writer. `Ok` even when the worker is gone — the file is the promise.
- Worker loop: `recv()` (with a 1 s timeout while a snapshot is still
  uncommitted), drain the channel to the **latest** snapshot (twenty taps
  during a scan become one write), then commit it through
  `queue_persistence::save(db, &queue)` — the unchanged read-modify-write of
  `ui.session.v1`, desktop fields untouched — acquiring the writer with
  **`try_lock` + bounded backoff (250/500/1000 ms) that checks
  `shutting_down` between steps**, the `with_busy_retries` shape from
  `play_recorder_retry.rs` applied to the mutex itself. When it gives up
  (scan still running) the snapshot stays as "latest" and is retried on the
  next wake-up or timeout. On commit: `remove_if_sequence(sequence)`.
- `Drop`: raise `shutting_down`, drop the sender, join. Because the worker
  never blocks in `lock()`, join returns within ≤ 250 ms even mid-scan, so
  `ReprisePlaybackService.onDestroy` → `coreSession.close()` stays quick; an
  uncommitted snapshot stays in the file for the next start.
- `#[cfg(test)] flush()`: blocks until the worker has committed the latest
  sequence (or `shutting_down`), for tests that inspect the DB directly.

### 3. `SessionInner::persist_queue` delegates

`self.queue.persist(queue)` — all 14 call sites (`playback_session.rs` ×5,
`stream_events.rs`, `history.rs` ×2, `queue_boundary.rs` ×5,
`trash_boundary.rs`) change behaviour through this one function; none is
edited. `SessionInner` gains `queue: QueuePersister`, spawned in
`AndroidPlaybackSession::new` next to `PlayRecorder::spawn`. The
`library.writer()` call in `persist_queue` disappears.

### 4. Restore prefers the file

`queue_persistence::restore` (startup, `library.reader()`): read the snapshot
file first; if present, its queue replaces the one from `ui.session.v1`
(track metadata is still resolved through the reader exactly as today). The
session then hands that queue to the persister once more, so the DB catches up
as soon as the writer is free and the file goes away. A damaged file is
ignored — the DB copy is the fallback, never a crash.

## Tasks

1. `queue_snapshot_file.rs`: `write` / `read` / `remove_if_sequence` with
   temp + fsync + rename and the lock file; unit tests for atomic replace,
   damage (truncated, wrong version tag, garbage), and the sequence check.
2. `queue_persister.rs`: struct, `spawn`, `persist`, worker loop with
   coalescing and `try_lock` backoff, `Drop`, `#[cfg(test)] flush()`.
3. Wire into `SessionInner` / `AndroidPlaybackSession::new`; `persist_queue`
   delegates; remove its `library.writer()` call.
4. `queue_persistence::restore` reads the file first; the session re-persists
   the restored queue.
5. Tests (all `reprise-android-ffi`, no JVM):
   - `playback_writer_lock_tests.rs`: the two red tests go green **unchanged**
     (budget 300 ms) — this is the acceptance criterion.
   - new `queue_persister_tests.rs`:
     - a snapshot written while the writer is held is committed after the
       holder releases it (`flush()`, then a fresh reader sees it);
     - twenty snapshots during one hold produce one DB write (assert the DB
       holds the last queue and the file is gone after `flush()`);
     - a newer snapshot written during the drain survives
       `remove_if_sequence`;
     - `Drop` while the writer is held returns within 500 ms and leaves the
       file;
     - a fresh session after that restores the file's queue, not the DB's;
     - a damaged file is skipped and the DB queue restored.
   - `queue_persistence_boundary_tests.rs`: the six existing tests keep
     passing; `queue_saves_leave_unrelated_desktop_session_fields_untouched`
     calls `flush()` before reading `ui.session.v1` and stays the proof that
     the drain still does read-modify-write.
6. Gate: `cargo test -p reprise-android-ffi`, `cargo clippy -p
   reprise-android-ffi --all-targets`, `cargo fmt --check`; confirm
   `android/app/src/main/java/uniffi/reprise_android_ffi/reprise_android_ffi.kt`
   is unchanged after `scripts/android-build.sh`'s binding step (or the
   equivalent uniffi generate) — no Kotlin diff.
7. Commit message states the hypothesis that held: transport calls took the
   writer synchronously while the scan holds it for the whole walk; the
   snapshot file plus drain thread is the fix.

## Risks

- Kill between `fetch_add` and the file write: microseconds; the same window
  the play journal accepts.
- A snapshot from a newer app version read by an older one: version tag
  mismatch counts as damage, DB copy wins.
- The worker's 1 s retry timer during a long scan: one `try_lock` per second,
  negligible.

## Follow-ups (not this plan)

- **Play journal `Drop` joins a thread parked in `writer.lock()`** — the same
  `try_lock` + backoff shape as here; today `coreSession.close()` in
  `onDestroy` can hang for the scan's duration.
- **The scan holds the writer for the whole SAF walk** (backlog item 2):
  still blocks `importTrackAnalysis` (seen waiting in the ANR traces), play
  count drains, and the journal `Drop` above. Fix lives in `reprise-core`'s
  scanner API (walk without the writer, write in batches).

## Parallelität

Cannot be cut. Tasks 1–5 all change or depend on `playback_session.rs`
(`SessionInner`, `persist_queue`, `restore`) and are verified by one test
module that needs all of them; the two new files are only meaningful once
wired, and a "file module only" strand would finish in minutes and then wait
for the rest — zero wall-clock gain for one extra merge. Single strand, no
`strands:` key.
