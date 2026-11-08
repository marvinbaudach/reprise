---
slug: start-watchdog-restart-off-main-thread
worktree: /home/marvin/Projects/reprise-start-watchdog-restart-off-main-thread
branch: feature/start-watchdog-restart-off-main-thread
phase: refactored
codex_session:
created: 2026-11-04
---
# The start watchdog restarts a hung start without blocking the main context

Closes #1323 and #1324. Single strand: every task touches
`crates/reprise-platform-linux/src/player/start_watchdog.rs` and `player.rs`,
so there is no disjoint file group.

## Problem

`start_watchdog::restart` runs `playbin.set_state(Null)` on the default main
context, while `Look::at_poll` holds the playbin lock (`start_watchdog.rs`,
`fn at_poll` / `fn restart`). If the file source is blocked inside a kernel read
(a dead NFS/SMB mount, a stalled FIFO), `Null` waits for that read to return.
The main context waits with it, so the UI freezes, and every other path that
wants the playbin lock blocks too. The watchdog fires exactly when a start has
stalled, so it can turn a silent track into a frozen app.

`Player::rebuild_playbin` (`player.rs`, around lines 425–485) has the same
shape: it sets the old element to `Null` under the lock before it swaps.

## Design

1. **A reaper for retired pipelines.** Add a small function, for example
   `retire_pipeline(element: gst::Element)` in a new sibling module
   `crates/reprise-platform-linux/src/player/reaper.rs`. It spawns a named
   thread (`reprise-pipeline-reaper`) that calls `set_state(Null)`, logs the
   result at `debug` (or at `warn` if it took longer than one second), and drops
   the element. If thread spawning fails, it falls back to doing the same
   inline and logs a warning. Never `unwrap`/`expect`: the never-crash lints
   apply.
   - The reaper must never take the playbin lock and must not hold any `Arc`
     the player locks.
2. **`rebuild_playbin` hands the old element to the reaper** instead of
   `set_state(Null)` under the lock. Everything else about the swap stays:
   build the new element with `build_playbin`, set spectrum messages, attach the
   new bus watch, carry `user_volume`, swap under the lock, replace the
   `BusWatchGuard`. Replacing the guard removes the old bus watch, so the stuck
   pipeline's late messages never reach the handler. Make sure the guard swap
   happens before the old element can post anything that matters, and that the
   old element is moved out of the lock scope before it goes to the reaper.
3. **The watchdog restarts by rebuilding, not by `Null` → target in place.**
   - The watchdog only holds `Arc`s, so give it a restart callback that the
     `Player` builds at construction. For example, `Arc<dyn Fn(u64, gst::State)
     -> Result<(), RestartError> + Send + Sync>`, taking the watched generation
     and the target state.
   - The callback re-applies everything a fresh element needs: `uri`, track
     gain, download-buffering flags, volume and spectrum flag. These are the
     same calls `try_play` makes (`player.rs` around 365–375). It then sets the
     new element to the target state.
   - Extract a shared helper so `rebuild_playbin` and the watchdog restart do
     not duplicate the swap sequence. Keep `player.rs` under 800 lines: if it
     grows past that, move the swap into a cohesive sibling module.
   - **Locking:** `at_poll` takes the lock only to observe. It releases the lock
     before calling the restart callback. The callback re-takes the lock and
     re-checks `stream_generation == generation` before it swaps. If a newer
     `play` slipped in, it stands down without touching anything. This replaces
     the old "lock held from look to restart" guarantee with a re-check under
     the lock, the same pattern `CrossfadeEngine::promote` uses
     (`crossfade.rs` around 556–580).
   - **The segment gate:** a CUE segment start's pending start must survive
     (`segment.rs`, `SegmentHandle`). Do not call `segments.reset()` on this
     path. The new element gets its own segment boundary probe from
     `build_playbin` on the same shared gate, so the next preroll still seeks
     to the segment start.
   - **Stream generation:** the restart does **not** bump it. It is the same
     start, and the watchdog's own `same_stream` check depends on that.
   - Update the module doc comment of `start_watchdog.rs`, which today says the
     start "is begun over into `Paused`" from `Null`.
4. **#1324 cleanup.**
   - Drop the unused `_segment` parameter of `start_watchdog::covers`: at the
     definition, at the caller in `player.rs`
     (`start_watchdog::covers(uri, live, segment)`), and in the unit test
     `only_a_local_file_whole_or_a_cue_segment_is_covered`.
   - **`disarm()`: decide from the code, then document.** Check whether
     `Player::stop` and `Drop` bump `stream_generation`.
     - If they do not, `disarm()` is the only guard after a stop and on drop.
       Keep it, and state at its definition exactly which cases it alone
       covers and which the generation check covers.
     - If they do, remove `disarm()` and its call sites.
     - Either way, add a unit or integration test that **fails when the
       stream-generation check in `at_poll` / `judge` is removed**. Prove that
       by mutation: remove the check, run the test, see it fail, restore the
       check. Report the mutation result in your summary.

## Tests (test-first)

- **The freeze regression (#1323).** In the platform-linux test suite, next to
  the existing watchdog tests (`player/tests/` or wherever `play_26_*` live),
  build a source that blocks in a kernel read:
  - a FIFO (`nix`/`libc` `mkfifo`, or `std::process::Command::new("mkfifo")`,
    whichever the crate already allows in dev-dependencies) inside a
    `tempfile::TempDir`;
  - a writer thread that opens it, writes a valid WAV header plus a few hundred
    bytes, and then **keeps the fd open without writing**, so the reader
    blocks in `read()`.
  - Start it through a production `Player` (or the narrowest harness the
    existing watchdog tests use) with a short watchdog deadline. Let the
    watchdog fire, and assert that a heartbeat `glib::timeout_add` on the same
    main context keeps firing during the restart, with no gap longer than about
    500 ms.
  - The test must fail on the current code. Run it before the fix and record
    the failure. At the end it must close the writer fd, so no thread or fd
    leaks, and stay bounded by a timeout so a regression hangs nothing.
  - If a FIFO turns out not to reach the READY-with-pending state the watchdog
    covers, use a custom blocking source instead (an `appsrc` whose
    `need-data` blocks on a channel is not a kernel read, but it still blocks
    the streaming thread the same way). Say which one you used and why.
- **Restart re-check.** A test where a newer `play` lands between the
  watchdog's look and its restart: the restart must stand down and leave the
  newer start alone.
- **A CUE segment restart still seeks to its segment start** after the
  rebuild. Extend the existing CUE watchdog test if there is one
  (`play_26_*` / the #1319 tests), rather than writing a parallel one.
- Existing tests must keep passing, especially
  `start_watchdog_soak` (ignored, do not run it), `play_26_*`,
  `stream_generation_tests::*` and the crossfade promotion tests.

## Verification scope (overrides AGENTS.md's whole-workspace gate for this run)

Run, with `heavy-run heavy --` in front of each cargo command:
- `cargo fmt --check`
- `cargo clippy -p reprise-platform-linux --all-targets -- -D warnings`
- `cargo test -p reprise-platform-linux`
- `scripts/check-crate-layers.sh` (the panic-lint budget must not grow)

Do not run the Android, display or merge-readiness gates. The orchestrator
runs the full gate before landing.

## Known residuals

- `try_play` and `stop()` still run `set_state` on the main thread under the
  playbin lock, so a start or stop on a dead mount still freezes. This is
  pre-existing; the orchestrator files the follow-up issue. It also reaches the
  restart's new pipeline: a `play` or `stop` that lands while the restart's
  asynchronous start is blocked in `open()` waits on that element's state lock
  on the main thread.
- The crossfade promote still sets the outgoing pipeline to `Null` under the
  lock.
- An EQ change made between `build()` and the swap is lost: the new pipeline was
  built from the effects as they were, and is not rebuilt.
- `HARNESS_WATCHDOG_DEADLINE` (8 s) means the test harness never reaches the
  watchdog on its own, so only the dedicated watchdog tests and
  `play_22_flac_cue_tracks_start_reliably_cold_and_after_a_whole_file`, which
  asks for the production deadline, exercise the production restart path.
- A `stop` that takes the playbin lock right after the restart queued its
  asynchronous start can see that start land after its `Null`. The window is the
  time the element's thread takes to begin, and that revived start is not watched.

## UX rules

PLAY-26 (proposed in #1325) describes what the user sees: start over, then
skip. Behaviour does not change, only the thread the cleanup runs on. If
`docs/ux-rules.md` PLAY-26 text mentions *how* the restart happens (from
`Null` in place), adjust that sentence. Otherwise leave the rulebook alone.

## Parallelität

This plan cannot be cut. Tasks 2–4 all change `player.rs` and
`player/start_watchdog.rs`, so there is no disjoint file group. One strand.

Owned files: `crates/reprise-platform-linux/src/player.rs`,
`crates/reprise-platform-linux/src/player/**`,
`crates/reprise-platform-linux/src/player_pipeline.rs` (only if the swap
helper needs it), and this plan file. `docs/ux-rules.md` is owned only for the
PLAY-26 sentence named above.
