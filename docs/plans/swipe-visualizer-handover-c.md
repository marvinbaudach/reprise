---
slug: swipe-visualizer-handover-c
worktree: /home/marvin/Projects/reprise-swipe-visualizer-handover-c
branch: feature/swipe-visualizer-handover-c
phase: planned
codex_session:
created: 2026-10-09
---
# Strand C: stored frames wait for the adopted shape (#1197)

Mother plan: `docs/plans/swipe-visualizer-handover.md`. Strands A (#1226) and B (#1228) are
shipped. This strand closes the gap the device acceptance found on 2026-10-09
(`gh issue view 1197 --comments`, the last two comments; evidence in
`~/.local/share/reprise-device-run-20261008-1197/`).

## Problem

After a swipe flip, the incoming panel is live within ~20 ms. `NowPlayingScene` calls
`noteTrackChanged` and then `adoptShape` on its fresh engine. The new stream is still
BUFFERING, so there is no PCM yet. `SceneDriver.tick` therefore feeds the incoming
track's stored frames at position 0 (`fallbackBands`, `SceneDriver.kt:85, 104-124`)
through `onFrame` → `engine.ingestBands` (`NowPlayingScene.kt:546`).

`AndroidVisualEngine::ingest_bands` (`visualizer.rs:271-291`) returns early only on
`has_live_audio`. It ingests the stored frame over the adopted shape and clears
`awaiting_stream_after_reset`. `AdoptedShapeHold` is consulted only in the PCM path
(`visualizer.rs:532`).

On the device this shows for ~170–200 ms, until the first PCM arrives:
- a quiet intro (Interlude) draws an empty card at the floor, which violates goal 1;
- a loud song (A Dead Current) draws the smooth stored-frame "hill", which violates goal 2.

## Decision

The fix lives in Rust, in `reprise-android-ffi`. Kotlin is unchanged.

The reason is that `ingest_bands` is the one place that knows both facts at once: a shape
was adopted, and no PCM has spoken since. Gating in Kotlin would duplicate the hold
state the engine already owns.

**Rule.** While an adopted shape is held, no live PCM has arrived since the adoption, and
less than `ADOPTED_SHAPE_STORED_FRAME_GRACE` has passed since `adopt_shape`,
`ingest_bands` leaves the display alone:
- it does not ingest;
- it does not clear `awaiting_stream_after_reset`;
- it does not change `has_analysis`.

The engine keeps ticking, so the adopted shape and its caps behave exactly as during the
PCM-path hold.

**What ends the hold:**
- First live PCM: the existing PCM-path hold (`should_hold`) takes over unchanged, and
  `has_live_audio` then blocks stored frames as before.
- The grace expires: stored frames take over, so a device without PCM still falls back
  as before.
- A pause (`set_playing(false)`): this clears the stored-frame grace, so a paused panel
  shows its normal paused projection.

**Grace value.** `ADOPTED_SHAPE_STORED_FRAME_GRACE = 500 ms`, equal to
`LIVE_AUDIO_STALE_AFTER`. The measured first-PCM delay is ~200 ms; 500 ms covers slower
buffering. A device without PCM gets the old behaviour 0.5 s late.

**A stream reset after the adoption does not end the grace.** `reconcile_stream_generation`
and `reset_live_presentation` clear `AdoptedShapeHold`. If the grace lived in the hold's
phase, a reset arriving after `adopt_shape` and before the first PCM would let the stored
frames back through, which is the same bug in a different order. The grace is therefore
its own deadline, `stored_frame_grace_until: Option<Duration>`:
- `adopt_shape` sets it to `now + grace`;
- it is cleared only by the first live PCM, by `set_playing(false)`, and by its own expiry;
- a stream reset leaves it alone.

**No JVM test (grill 2026-10-09).** The Kotlin tests drive a fake engine
(`RecordingSceneEngine`) and cannot see a Rust-side gate, so a JVM test would pass before
and after the fix. The Rust tests are the failing proof, and the device acceptance is the
end-to-end check.

**Timestamp (superseded by the deadline above; kept for the hold's PCM path).** `AdoptedShapeHold::begin(now)` records when the adoption happened.
`AdoptedShapeHold::blocks_stored_frame(now)` answers the rule. It stays true only while the
phase is still `AwaitingSignal` (no PCM has arrived) and the grace has not passed.

## Tasks (test first)

1. **Failing Rust tests.** Put them in a new `crates/reprise-android-ffi/src/visualizer_stored_frame_hold_tests.rs`,
   registered like the sibling test modules, and use the existing test clock:
   - `ac_29_stored_frames_wait_for_the_adopted_shape_until_the_stream_speaks`:
     `set_playing(true)`, `note_track_changed`, `adopt_shape(loud)`, then
     `ingest_bands(near-floor)` and a tick. The scene still shows the adopted shape and
     not the floor.
   - `ac_29_stored_frames_take_over_once_the_grace_expires`: the same setup, then advance
     the clock past the grace and `ingest_bands(near-floor)` → the scene follows the
     stored frame.
   - `stored_frames_without_an_adoption_ingest_at_once`: guards the regression. Without
     `adopt_shape`, `ingest_bands` behaves as today.
   - `a_pause_ends_the_stored_frame_grace`: adopt, `set_playing(false)`, then
     `ingest_bands` → the frame is ingested as today.
   - `ac_29_a_stream_reset_after_the_adoption_keeps_stored_frames_waiting`: adopt, then
     `reset_audio_stream`, then `ingest_bands(near-floor)` within the grace → the adopted
     shape is still drawn.
   - `live_pcm_after_the_adoption_keeps_the_pcm_hold_rules`: adopt, push PCM, tick.
     `should_hold` governs as before, and later stored frames are ignored through
     `has_live_audio`.
   Run them and see them fail. Only the regression guards may pass already.
2. **Implement.** The grace deadline (field + named constant, placed where it keeps `visualizer.rs` under 800 lines, e.g. a small sibling module or `adopted_shape_hold.rs`), with
   `blocks_stored_frame(now)` answering the rule. In
   `visualizer.rs`:
   - `ingest_bands` gets an early return after `expire_stale_live_audio` and the
     `has_live_audio` check;
   - `adopt_shape` passes `now`;
   - `set_playing(false)` clears the grace.

   `visualizer.rs` is at 792 lines. The net change must keep it under 800. If it does
   not, move the logic into `adopted_shape_hold.rs`, not the comments.
3. **AC-29 text.** Append one sentence to the 2026-10-08 swipe amendment in
   `docs/ux-rules.md`: the new song's stored frames do not replace the handed-over shape
   before its own audio speaks, bounded by 0.5 s. Add the new test names to the test list.
4. **Gates.** Run the full battery from AGENTS.md, including the Android JVM suite
   (`scripts/check-android-suite.sh`), because the bindings are regenerated.

## Post-merge

- **Device acceptance**, under `device-lock`, the same 2×2 as on 2026-10-09:
  - Pairs: AFTER THE SILENCE → Interlude, then Interlude → A Dead Current.
  - Arms: the baseline is the then-current `dev` with this strand reverted; the fix arm
    carries it. Both use the same versionCode.
  - Sheets at 33 ms.
  - Pass: no empty card and no hill at settle on the fix arm, both still present on the
    baseline arm.
- Then close #1197.

## Parallelität

Not cut. All of it is one Rust change in `adopted_shape_hold.rs` and `visualizer.rs` with
its tests, and one AC-29 sentence. There is no disjoint second file group worth its own
worktree. Single strand, slug `swipe-visualizer-handover-c`.

## Outcome

Implemented on 2026-10-09. The first red run passed the four regression guards and failed
the two AC-29 cases that prove an immediate stored frame and a post-reset stored frame
replaced the adopted `0.8` shape with the `0.01` frame. The independent 500 ms deadline now
survives stream resets and is cleared by analyzed live PCM, a pause, or expiry; all six tests
pass without a JVM test.
