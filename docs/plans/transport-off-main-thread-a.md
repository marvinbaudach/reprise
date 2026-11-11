---
slug: transport-off-main-thread-a
worktree: /home/marvin/Projects/reprise-transport-off-main-thread-a
branch: feature/transport-off-main-thread-a
phase: planned
codex_session:
created: 2026-11-11
---
# Strand A: play, skip, pause, seek and stop never wait on a stuck source

Strand A of `docs/plans/transport-off-main-thread.md` (closes #1327). Read the
mother plan first for the decisions and the cut. Follow-up of #1323 (fixed by
#1329). Line numbers are from `origin/dev` at `003ae49fd3`.

## Problem

A local file on a mount that has gone away blocks two kernel calls. A dead hard
NFS or SMB mount is the usual case.

- `filesrc`'s `open()` runs synchronously inside `set_state(PAUSED)`, through
  `urisourcebin` in READY_TO_PAUSED (`gsturisourcebin.c`, `setup_source`, 1.28.7).
- Any `read()` blocks too. A `Null` on a pipeline whose source sits in `read()`
  waits for that read.

Every call below runs on the GLib main context, or holds the playbin lock while
it waits, so the whole window freezes:

| Path | Call | Waits on |
|---|---|---|
| `Player::try_play` (`player.rs` 372–400) | `set_state(Null)` on the playing pipeline, then `set_state(Playing)`, or `segment::start_segment`'s `set_state(Paused)` (`segment.rs` 459) | a source in `read()`; the new source's `open()` |
| `Player::stop` (593–605) | `set_state(Null)` under the lock | a source in `read()` |
| `Player::abort_crossfade` (312–331, called from play, stop and seek) | `secondary.set_state(Null)` | the ramp thread's `set_state(Playing)`, which holds the secondary's state lock while it sits in `open()`; a source in `read()` |
| `Player::toggle_pause` (488–503) | `set_state(Paused/Playing)` | the element's state lock, held by a start stuck in `open()` |
| `Player::seek_to` (508–536) | `seek_simple` | the state lock (`gst_element_send_event` takes it); a flushing seek also waits for the stream lock that a source in `read()` holds |
| `SegmentGate::complete_start` (`segment.rs` 282–329, from the bus watch at `player_pipeline.rs` 437) | `seek_simple`, `set_state(Playing)` | as above |
| `CrossfadeEngine::promote` (`crossfade.rs` 523–593, ramp thread) | `old.set_state(Null)` **under the playbin lock** (580) | a source in `read()`; every main-context path that wants the lock waits with it |
| `CrossfadeEngine::run` (361) | `secondary.set_state(Playing)` on the ramp thread | the secondary's `open()`. `await_preroll`'s bound never starts, `crossfading` stays up, and the outgoing track's end-of-stream stays swallowed: playback dead-ends at the end of the outgoing track |
| `give_way`, `Preroll::Superseded`, the ramp's abort, the lost re-check in `promote` (482, 369, 401, 562) | `secondary.set_state(Null)` inline | as above. `give_way` never gets as far as releasing the guard |

`gst_element_call_async`, which #1329 uses to begin a restarted start, runs on an
unbounded GLib thread pool. Two calls on the same element have no order, so it
cannot keep a `stop` or a pause behind a start.

## What the frontend relies on

Read from the code on 2026-11-08. The frontend has not changed since.

- **One synchronous failure path.** Every local start goes through
  `PlayerController::present_track`
  (`reprise-gnome/src/ui/playback/player_controller.rs`). On `Err` from `play` it
  calls `handle_unplayable_track` (`playback_faults.rs`), which marks the track
  missing if its file is gone, shows a toast and calls `skip_after_failure`. On
  `Ok` it runs side effects that must not happen for a file that never plays:
  - `begin_scrobble` ("playing now")
  - `note_playback_started` (play history)
  - `notify_current_track_changed`
  - MPRIS Playing
  - `feed_next`
- **The bus path reaches the same handler.** A bus ERROR becomes
  `PlayerEvent::Error` (kind Other). In queue mode, `player_event_handling.rs`
  calls the same `handle_unplayable_track`. `stale_events.rs` drops an `Error` or
  `Position` whose generation is older than `current_generation()`. **The
  generation must therefore be bumped before `play` returns.** Otherwise a
  start's own error is dropped as stale, and a missing file never skips.
- **The skip guard counts both channels.** `skip_after_failure` is limited to the
  queue length, so asynchronous failures cannot introduce a skip loop.
- **`toggle_pause` is only called from Playing or Paused.** After `stop`, the
  frontend starts again with `play(item)`. The returned state is used by the
  podcast path and by the sleep timer, where `Ok(Paused)` confirms its pause.
- **`seek_to`'s result is used.** `try_seek_with_feedback` turns `Err` into a
  one-shot retry (`seek_start.rs`, `pending_local_seek`), and into
  `initial_seek_finished(false)` on the external-media resume. `Ok` emits MPRIS
  `Seeked`.
- **The trait needs no change.** The `PlaybackBackend` trait
  (`reprise-core/src/playback.rs`) promises nothing synchronous; it says event
  delivery is asynchronous to the calls.

## Design

### D1. One ordered command queue per pipeline

New module `crates/reprise-platform-linux/src/player/pipeline_queue.rs`.

**The model.** Every pipeline the player builds owns one queue: the start
pipeline, the idle pipeline after `stop`, a watchdog restart and a crossfade
secondary. The queue is a dedicated, named thread (`reprise-pipeline-queue`) fed
by a channel. **The main context never calls `set_state`, and never sends a seek,
on a pipeline itself; it submits a job.**

**One type for element and queue.** A `Pipeline` value holds the element and its
queue together. The playbin slot becomes `Arc<Mutex<Pipeline>>`, and so does the
crossfade `IncomingSlot`. No code can reach a pipeline without its queue.

**API.**
- `submit(job)` queues a job and returns.
- `ask(job)` returns an answer handle, whose `wait(bound)` gives `Some(result)`
  or `None` once the bound has passed. A job runs to its end whether or not
  anyone still waits for it.

**Running jobs.**
- Jobs run in submission order, one at a time, on the pipeline's thread.
- A job takes no player lock. The segment gate's mutex is the only exception:
  `complete_start`'s bookkeeping already takes it.

**Retiring a pipeline.** `retire(self)` consumes the pipeline.
- Jobs that have not begun are dropped. A job already running finishes.
- Then the queue takes the pipeline to `Null` and drops it, and the thread ends.
- This is the reaper, now ordered behind the pipeline's own work. A `stop` right
  after a start can never see that start land after its `Null` (the #1329
  residual).
- Fold `reaper.rs` into this module, or reduce it to the retirement half. Keep:
  - `reaper::in_flight`, which counts retired pipelines that have not reached
    `Null`, under the same name, so the #1329 tests keep their meaning;
  - the `SLOW_NULL` warning.

**Edge cases.**
- A `Pipeline` dropped without `retire` is retired: the closed channel counts as
  the retirement.
- A job submitted after retirement is dropped.
- If the thread cannot be spawned, jobs run inline with a warning, as the
  reaper's fallback does today.

**The bus watch goes first.** Whoever retires a pipeline that has a bus watch
removes the watch before calling `retire`, as `PipelineSwap::retire` does today.
Nothing a retired pipeline posts, such as a late error from a stuck start, reaches
the frontend.

### D2. Every start is a fresh pipeline; the main context waits briefly for its answer

`play`, `play_uri` and `play_live_uri` all go through `play_resolved_uri`:

1. `reset_transition()` and `start_watchdog.disarm()`, as today.
2. Build a fresh pipeline configured for the start, through the `PipelineSwap`
   builder: URI, download buffering, track gain, spectrum flag, bus watch and
   queue. If the pipeline cannot be built, `play` fails and nothing is swapped.
3. Under the playbin lock:
   - carry the user volume over;
   - swap the fresh pipeline in;
   - write the start record (D6);
   - **bump the stream generation**. This happens once per start, success or
     failure; nothing of a failed start is ever delivered, so nothing is
     mislabelled.

   A CUE start calls `gate.begin_start` first, as `start_segment` does today.
4. Retire the previous pipeline (D1), outside the lock. The old track stops at
   once, as today.
5. `ask` the fresh pipeline's queue for the start: `Playing`, or `Paused` for a
   CUE segment. Wait up to `START_ANSWER_WAIT` (250 ms) for `set_state`'s answer.
   - **`Err`.** A missing file fails `open()` within a millisecond. Retire the
     failed pipeline after removing its bus watch, swap in an idle pipeline, emit
     nothing and return `Err`. The frontend's synchronous failure path runs
     exactly as today. The failed pipeline's bus error is never delivered: it
     cannot be dispatched while `play` holds the main context, and the watch is
     gone before `play` returns.
   - **`Ok`, or no answer in time** (a source blocked in `open()`). Emit
     `StateChanged(Playing)` for a whole file; a CUE start's comes from
     `complete_start` (D5). Arm the watchdog when `covers` holds, and return `Ok`.
     A late failure arrives on the bus under the new generation and takes the bus
     path.
6. **The wedged-pipeline recovery goes.** No instance ever sees a second start,
   so `rebuild_playbin` and the retry-once in `play_resolved_uri` have nothing
   left to heal.
   - Remove both, and rewrite the `## Wedged-pipeline recovery` doc on `play` to
     say why.
   - `rebuild_tests.rs` becomes a test that a start after a failed start plays,
     at the user's volume.
7. The watchdog's `Progress::starting` begins with `awaiting_start = true`. The
   first start is now issued from another thread too, so a look that finds a
   fresh pipeline still in `Null` must wait, not stand down.

**Why 250 ms.** It must stay well under the freeze tests' `MAX_GAP` (500 ms),
including one pipeline build. A healthy start answers in about 10 ms (#1327's
probe), and a missing file in under 1 ms. Task 8 measures the answer time under
the soak load; that measurement decides whether the bound holds.

### D3. `stop` retires

`stop` does the following, in order:
1. `reset_transition()` and `disarm()`.
2. Under the lock, swap in an idle pipeline (no URI) and clear the start record.
3. Retire the old pipeline, after removing its bus watch.
4. Emit `StateChanged(Stopped)` and return `Ok`.

A late error from a stopped start can no longer skip to the next track, because
the stopped pipeline's bus watch is gone. If no idle pipeline can be built, `stop`
submits `Null` to the current pipeline's queue instead and logs a warning.

### D4. `toggle_pause` follows the state the user asked for

- Each pipeline records the state last asked of it: `Idle`, `Playing` or
  `Paused`. The record is written synchronously, by the start and by
  `toggle_pause`, under the slot lock.
- `toggle_pause` flips that record, not `current_state()`, which lags behind
  queued jobs. It submits the change without waiting, emits `StateChanged` and
  returns the new state, as today.
- On a pipeline with no start (`Idle`) it does nothing and returns `Stopped`.
  Today it would restart the last URI without bumping the generation, and nothing
  calls it that way.

### D5. `seek_to` and the CUE completion go through the queue

**`seek_to`.** `abort_crossfade()` and `segments.seek_target_ms` stay
synchronous. The seek is `ask`ed and waited for up to `START_ANSWER_WAIT`:
- answered `true`: return `Ok`;
- refused: the job calls `segments.seek_refused()`, and `seek_to` returns `Err`;
- no answer in time: return `Err`, with the seek still queued. The frontend then
  arms its one-shot retry; the same target twice is harmless.

The job calls `seek_refused()` itself, so a late refusal still clears the flag.

**`complete_start`.** It keeps its synchronous checks: prerolled, and a start
pending. It submits its tail as one job to the queue of the pipeline whose bus
delivered the `ASYNC_DONE`; `attach_bus_watch` gets that queue for this purpose.
The tail is one of:
- the seek;
- or entering the state the user last asked for (D4), which is Playing unless
  they paused during the start, and emitting `StateChanged` from the job.

**The seek dedupe still applies.** The seek dedupe from #1332 sits on the audio
filter that `build_audio_filter` builds for every pipeline, so a seek sent from a
queue thread is deduplicated like any other.

### D6. The start record

- The player keeps what each start was in shared state: `uri`, `live`, `gain_db`
  and `segment`.
- `play` writes it under the lock, and `set_current_gain_db` keeps it current.
- `restart_hung_start` carries the start over from the record. It no longer reads
  properties of the hung pipeline: `carry_start` reads `uri` from a `playbin3`
  that may be inside its own stuck state change.
- It submits the restarted start to the fresh pipeline's queue instead of using
  `call_async`.

### D7. Crossfade

- The secondary is built with its queue. Its start is submitted without waiting,
  so `await_preroll`'s bound applies even when its `open()` blocks.
- These paths retire the secondary instead of setting it to `Null` inline:
  `give_way`, `Preroll::Superseded`, the ramp's abort, and the lost re-check in
  `promote`.
- `Player::abort_crossfade` retires the secondary.
- `promote` swaps under the lock as today, then swaps the bus-watch guard, then
  retires the outgoing pipeline **after the lock is released**.
- `crossfade.rs` is 720 lines. If the change takes it past 800, move `promote` and
  `give_way` into a cohesive sibling under `crossfade/`.

### D8. What stays

These only write properties, and stay as they are:
- `set_volume`, `set_current_gain_db` and `set_spectrum_enabled`.
- `set_audio_effects`. Its `update_existing_audio_filter` only sets equalizer
  bands (checked: `player_effects.rs` 194–212).
- The position ticker, which clones the element out of the lock and queries
  outside it.

## Tasks (test-first)

1. **Shared stuck-source fixtures.**
   - Move these into `player/tests/stuck_source_support.rs`: `StuckFifo`,
     `wav_header`, `longest_gap`, `OpenerRelease`/`release_openers`, `make_fifo`,
     `open_descriptors_of` and the heartbeat constants. The two #1329 test files
     use them from there.
   - Add a variant whose writer first writes about one second of silent samples
     and then stalls, so a pipeline reaches `Playing` before its source blocks in
     `read()`.
2. **Failing tests first.** Write the tests of the next section and run them on
   the unchanged code. Record which fail and how (gap length, timeout). Every
   freeze test must fail before the fix.
3. **D1**, with its unit tests.
4. **D2 and D6**: the start, the start record, and the restart from the record.
5. **D3, D4 and D5.**
6. **D7.**
7. **Docs.** Update the module docs of:
   - `player.rs`, including `play`'s recovery section;
   - `pipeline_queue.rs` and `reaper.rs`;
   - `pipeline_swap.rs`;
   - `start_watchdog.rs`, which now issues its start from the pipeline's queue;
   - `crossfade.rs`'s header.
8. **Measure**, with an ignored test or a temporary probe removed before the
   commit. 50 runs each, median and p95, this branch against `dev`, interleaved:
   - the time `play` spends on the main context for a regular file;
   - the cost of one pipeline build;
   - `START_ANSWER_WAIT`'s margin: the answer time of a healthy start under the
     soak load (2 busy threads per core). Report p99 and the maximum.

   Report the numbers. There is no threshold, except that a p99 above 150 ms
   goes back to the orchestrator before landing.

## Tests

**Every freeze test:**
- asserts that a `glib::timeout_add` heartbeat on the main context never goes
  longer than `MAX_GAP` (500 ms) without firing;
- is bounded by a timeout, so a regression fails instead of hanging;
- at the end, releases every FIFO and waits for `reaper::in_flight::count()` to
  reach 0.

**Start and stop.**
- `play` of a FIFO nobody writes (`open()` blocks):
  - `play` returns within `START_ANSWER_WAIT` plus slack, with no gap;
  - then `stop`, with no gap;
  - after the release, the stopped pipeline never reached `Playing`, no
    `StateChanged(Playing)` followed `Stopped`, and **no `Error` event followed
    `Stopped`**. Release the FIFO with bytes that are not audio, so the late start
    fails.
- Skip, meaning `play` of a regular fixture, away from:
  - a start stuck in `read()` during preroll;
  - a track stuck in `read()` after it reached `Playing`.

  No gap, and the fixture reaches `Playing`.
- `stop` of a track stuck in `read()` after `Playing`: no gap.

**Pause and seek.**
- `toggle_pause` during a start stuck in `open()` returns `Paused` at once, with
  no gap. After the release with real samples, the pipeline settles in `Paused`,
  not `Playing`.
- `seek_to` on a track stuck in `read()` returns `Err` within the bound, with no
  gap.
- A CUE start paused before its preroll stays paused after `complete_start`.
  Extend the existing CUE start tests.

**A missing file.** `play` of a missing file still returns `Err` synchronously
(`tests.rs` ~471–495 stays as it is). It emits no `StateChanged(Playing)`, and no
`Error` event follows within a pump window.

**Crossfade.**
- A secondary whose `open()` blocks gives way within `PREROLL_BOUND`: the guard
  is down, and the outgoing track's end is reported as `TrackFinished`.
- A `play` during that fade shows no gap.
- `promote` retires the outgoing pipeline with the playbin lock free. A blocking
  pad probe on the outgoing pipeline makes its `Null` hang; `set_volume` and the
  heartbeat must keep going.

**Watchdog.**
- Extend the blocking-`open()` test past the **second** restart, with the
  heartbeat watched across both.
- A `stop` right after a restart's queued start leaves the stopped pipeline in
  `Null`.

**Queue unit tests.**
- Jobs run in order.
- Retirement drops jobs that have not begun.
- `Null` follows a running job.
- A job submitted after retirement is dropped.
- Dropping without `retire` retires.
- The thread ends.

**Mutation proof.** For the play, stop, pause, seek and promote freeze tests: put
the inline call back, see the test fail, then restore it. Report the results.

**Existing tests keep passing.** Adjust a test only to wait for queued work,
never to weaken an assertion. List every adjusted test with its reason.

## Verification scope (overrides AGENTS.md's whole-workspace gate for this run)

Prefix each cargo command with `heavy-run heavy --`:

- `cargo fmt --check`
- `cargo clippy -p reprise-platform-linux --all-targets -- -D warnings`
- `cargo test -p reprise-platform-linux`: the whole crate test binary, not a
  filtered run, because a filtered pass hides main-context races.
- `scripts/check-crate-layers.sh`: the panic-lint budget must not grow.

Do not run the workspace suite, or the Android, display or merge-readiness gates.
The orchestrator runs the full gate on a clean tree before landing.

## Known residuals (state them in the summary and the PR)

- **Frontend file access on the main context** is strand B's
  (`transport-off-main-thread-b.md`). This strand's FIFO tests cannot reproduce a
  blocking `stat()`.
- **A source that never returns keeps one queue thread and its pipeline** until
  the read returns. On an exclusive ALSA sink that also includes the device. At
  exit, a thread in uninterruptible sleep can hold the process.
- **A dead mount still costs about 24 s per track.** The watchdog begins a start
  stuck in `open()` over up to three times (4 + 8 + 12 s) before it reports the
  failure, and each attempt leaves one stuck thread. Nothing freezes, but a queue
  of tracks on a dead mount skips slowly.
- **The retired pipeline's `Null` and the next start run on two threads.** With
  `REPRISE_AUDIO_SINK` naming an exclusive device sink, the new start can find the
  device still open. That override is never used in production
  (`player_pipeline.rs` 284), and the default sink is shared.
- **A late failure looks like a success to the frontend.** A start that answers
  only after `START_ANSWER_WAIT` and then fails reports on the bus, so the
  frontend's `Ok` side effects (scrobble, history) run for it.
- **A seek that times out returns `Err` while it stays queued.**
- **Stale ticks remain possible (unchanged).** Position ticks and spectrum frames
  already under way can carry the previous generation.

## UX rules

No rule changes (grill decision). PLAY-26 stays as it is: the watchdog's
start-over and skip do not change. FB-6 and PLAY-4a/5a/5b keep their behaviour,
because a fast failure stays synchronous.

## File ownership

- `crates/reprise-platform-linux/src/player.rs`
- `crates/reprise-platform-linux/src/player/**`
- `crates/reprise-platform-linux/src/crossfade.rs`
- `crates/reprise-platform-linux/src/crossfade/**`
- `crates/reprise-platform-linux/src/player_pipeline.rs`
- this strand file

Touch nothing in `reprise-gnome`, `reprise-core` or `docs/ux-rules.md`.
