---
slug: the-pause-survives-a-skip-a
worktree: /home/marvin/Projects/reprise-the-pause-survives-a-skip-a
branch: feature/the-pause-survives-a-skip-a
phase: reviewed
codex_session:
created: 2026-09-02
---
# Strand A — Android: the pause survives a skip

Owns `crates/reprise-android-ffi/**`. Touch nothing else. The mother plan is
`the-pause-survives-a-skip.md`; read it first — it carries the decisions, the
verification arms and the traps.

## Tasks

1. **Stop `adopt_current` deciding the play state.**
   `crates/reprise-android-ffi/src/playback_session.rs:185-197` hard-codes
   `snapshot.state = Playing` at line 194. Pass the intended state in instead.
   Its callers want different things:
   - `set_tracks` (182) and `previous_in_queue_order` (570) and `move_playhead`
     (676), via `adopt_current_for_play_intent` (199-202) — "whatever the user
     already wanted";
   - the stream-event sites (`playback_session/stream_events.rs:74`, `:86`,
     `:126`) — genuinely playing, because the backend has already advanced.

   Read the pre-change state before it is overwritten. `move_playhead` holds the
   locked snapshot from line 673, so the value is available at that point.

2. **Do not start the backend on a paused skip.**
   `move_playhead` (668-686) calls `start_current()` at line 682. Skip it when
   the carried intent is `Paused`.

   Do not add machinery for resuming afterwards — it exists.
   `toggle_pause` (517-528) already handles
   `state == Paused && !current_loaded && queue.current().is_some()` by setting
   Playing and calling `start_current()`, and `adopt_current` already sets
   `current_loaded = false` (line 193). Verify that this is still true before you
   rely on it, and say so in your summary.

3. **Seed the duration from the library.**
   `adopt_current` sets `snapshot.duration_ms = 0` (line 192), and the only other
   writer is `PlayerEvent::Position` (`stream_events.rs:57-64`), which never
   fires while paused because the position ticker is gated on `player.isPlaying`
   (`Media3PlaybackPort.kt:78-94`, line 80). Set it from the queue's current
   track instead — the value is `Track.duration_ms`
   (`crates/reprise-core/src/models.rs:64`), already carried into the FFI layer as
   `TrackRow.duration_ms` (`crates/reprise-android-ffi/src/browse.rs:100`).

   Keep `PlayerEvent::Position` authoritative once it arrives: the player's own
   duration must still win over the library's when they disagree.

4. **Publish once, and never a false Playing.**
   Today `notify()` fires with `Playing` before the backend is called at all. A
   paused skip must not emit a Playing state on the way through — a flicker in
   the notification or on MPRIS is a user-visible defect in its own right.

5. **Apply the same rule to `previous_in_queue_order`** (line 570-574). It has the
   same two halves as `move_playhead`. The mother plan's "one rule for all three
   doors" covers direction as well.

## Tests

`crates/reprise-android-ffi/src/playback_tests.rs`. Every new test must fail
against the current code — run it there first and report in your summary that you
saw it fail.

- A paused skip issues no start to the backend and leaves the snapshot `Paused`.
- A paused skip leaves `duration_ms` at the library value, not 0.
- Pressing play after a paused skip starts the track (the `toggle_pause` restore
  path) and reaches `Playing`.
- A **playing** skip is unchanged — still starts, still `Playing`. This is the
  regression the change most plausibly causes.
- End-of-track auto-advance still continues playing.
- The same set for `previous_in_queue_order`.

These existing tests assume today's behaviour and will need revisiting rather
than deleting: line 222 `core_queue_owns_gapless_advance_and_manual_next_previous`,
line 289 `snapshot_counts_automatic_advances_but_not_manual_skips`, line 335
`core_queue_starts_the_next_track_when_media3_reports_a_plain_end`.

Nothing in the repo currently pins "a track change while paused stays paused",
in Rust or in Kotlin.

## Gate

```
cargo test -p reprise-android-ffi
ANDROID_HOME=/home/marvin/.local/share/android-sdk ./scripts/check-android-suite.sh
```

Read the suite's verdict line, not the exit status of a pipe. A test count lower
than the baseline means something stopped running.
