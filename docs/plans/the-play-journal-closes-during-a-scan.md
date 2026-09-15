---
slug: the-play-journal-closes-during-a-scan
worktree: /home/marvin/Projects/reprise-the-play-journal-closes-during-a-scan
branch: feature/the-play-journal-closes-during-a-scan
phase: planned
codex_session:
created: 2026-09-15
---
# The play journal closes during a scan

## Problem

`coreSession.close()` in `ReprisePlaybackService.onDestroy` drops
`AndroidPlaybackSession`, which drops `PlayRecorder`, whose `Drop`
(`play_recorder.rs:177-196`) raises `shutting_down`, drops the sender and
**joins the worker thread**. That thread parks in `Mutex::lock()` on the
library writer at two places:

- `play_recorder_writer.rs:30-32` — inside `with_shared_writer_retries`, the
  journaled path. `with_busy_retries` only retries on `SQLITE_BUSY`; the mutex
  `lock()` never returns an error, so the existing backoff never sees the hang.
- `play_recorder.rs:217` — the unjournaled fallback loop (journal could not be
  opened) takes `writer.lock()` directly per play.

A library scan holds the writer for the whole SAF walk (`lib.rs:173`,
follow-up plan). A play recorded during a scan parks the worker for minutes;
`onDestroy` then blocks for as long, and Android reports an ANR ("executing
service"). #969 fixed the identical shape for the queue with `try_lock` +
bounded backoff that checks `shutting_down` between steps (`queue_persister.rs`
`try_commit`, `RETRY_BACKOFFS = 250/500/1000 ms`); the journal never got it.

## Goal

`PlayRecorder::drop` returns within one backoff step (≤ 250 ms) while another
thread holds the writer, and no play is lost by the change: journaled plays
stay in the journal and are counted on the next drain or open (already covered
by `an_unapplied_journal_entry_is_counted_on_the_next_open`).

Kotlin does not change; no uniffi surface change.

## Decisions (grilled 2026-09-15)

1. **Journaled path: `try_lock` replaces `lock()`; `WouldBlock` counts as
   busy.** `with_shared_writer_retries` maps `TryLockError::WouldBlock` to a
   retryable `SharedWriteError::WriterBusy`, keeps the existing
   `retry_after` schedule (250/500/1000 ms, 4 attempts) and the existing
   `shutting_down` check between sleeps. Giving up leaves the entry in the
   journal — exactly today's contract for `SQLITE_BUSY`. `Poisoned` stays
   non-retryable (already `WriterPoisoned => false`).
2. **Retry while the journal is non-empty.** Today a play that gave up waits
   for the *next* play or the next start. During a long scan that can be the
   whole scan. The worker's `for play in queued` becomes `recv_timeout(1 s)`
   while `journal.front().is_some()`, plain `recv()` when the journal is
   empty — the `RETRY_WAKEUP` shape from `queue_persister.rs`. One `try_lock`
   per second during a scan.
3. **Unjournaled fallback: bounded wait, then warn-and-drop.** This path has
   no durable copy, so giving up loses the play. Today's blocking `lock()`
   eventually writes it — but `onDestroy` has a 5 s deadline regardless, so
   "eventually" already meant "never, plus an ANR" during a scan. The fallback
   uses the same `try_lock` + `retry_after` schedule (≈ 1.75 s total) and then
   drops with a `tracing::warn!` naming the track. It only runs when the
   journal file could not be opened at all, which is already a degraded mode
   with its own warning.
4. **Shared helper, not a copy.** `queue_persister.rs::try_commit` and the
   journal writer get one helper: `writer_backoff.rs` with
   `pub(crate) fn try_lock_writer(writer: &Mutex<Db>) -> Result<Option<MutexGuard<Db>>, Poisoned>`
   (`None` = busy) — small enough that the two callers keep their own
   retry loops (`with_busy_retries` vs. `drain_snapshots`) rather than
   forcing one loop shape on both.
5. **Tests go into a new file.** `play_recorder.rs` is at 774 lines with its
   tests inline; the new tests live in `play_recorder_shutdown_tests.rs`
   (registered with `#[path]` like the other siblings).

## Tasks

1. `writer_backoff.rs`: `try_lock_writer`; `queue_persister::try_commit`
   uses it (behaviour unchanged, its tests stay green).
2. `play_recorder_writer.rs`: `with_shared_writer_retries` takes the writer
   via `try_lock_writer`; new `SharedWriteError::WriterBusy` is busy for the
   retry predicate. Update its inline tests.
3. `play_recorder.rs::write_queued_plays`: unjournaled loop uses the same
   helper with `with_busy_retries` and drops with a warning on give-up; the
   journaled loop uses `recv_timeout(RETRY_WAKEUP)` while the journal is
   non-empty and re-drains on timeout.
4. `play_recorder_shutdown_tests.rs`:
   - **drop while the writer is held returns within 500 ms** and the journal
     still holds the play (control arm: with the writer free, drop commits
     and the journal is empty);
   - a play recorded while the writer is held is counted once the holder
     releases, without another play arriving (the 1 s wake-up), asserted via
     a fresh reader within ~3 s — no sleeps in the fix path, a bounded wait
     loop in the test;
   - unjournaled mode (unopenable journal path, like
     `an_unopenable_journal_still_counts_plays`): a play during a hold is
     dropped with a warning after the schedule and drop still returns quickly.
5. Gate: `cargo test -p reprise-android-ffi`, `cargo clippy -p
   reprise-android-ffi --all-targets -- -D warnings`, `cargo fmt --check`;
   no Kotlin diff.
6. Commit message names the hypothesis: `Drop` joined a worker parked in
   `writer.lock()` behind the scan; `try_lock` with the bounded schedule is
   the fix.

## Risks

- A scan longer than the retry schedule still delays play counts until the
  1 s wake-up finds the writer free — intended; the journal is the promise.
- The unjournaled drop after ≈ 1.75 s is a real (rare, already-degraded) loss;
  decision 3 states why it is accepted.

## Follow-ups (not this plan)

- The scan holds the writer for the whole SAF walk — separate plan
  (`reprise-core` scanner lease), planned next. Decided in the grill: this
  plan lands first and on its own, because it is correct by construction for
  any long writer hold, not only the scan, and its `try_lock_writer` helper is
  the waiter side that plan needs anyway.

## Parallelität

Cannot be cut. Tasks 1–4 all change the play recorder's writer path and are
verified by one test module; the shared helper (task 1) is a precondition for
2 and 3. Single strand.
