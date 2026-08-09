# Android Now Playing scene verification

Date: 2026-08-09

Implementation under test: P1-P7 through `43cb6b5ef3`

This record follows the eight Wave D checks in `now-playing-scene.md`. It keeps
deterministic JVM evidence separate from emulator or physical-device evidence.
The focused automated suite is run with:

```bash
export ANDROID_HOME=/home/marvin/.local/share/android-sdk
export CARGO_BUILD_JOBS=4
scripts/verify-now-playing-scene.sh
```

The harness removes prior unit-test results, runs the nine named suites, checks
that every XML file is newer than the run marker, and verifies the exact total.
On 2026-08-09 it reported 29 fresh tests with zero failures, errors, or skips.

## Results

### 1. Pause

Automated: **passed**.

- `SceneStateTest.pause_stands_completely_still` proves that repeated advances
  at the paused frame do not change either angle, any band, energy, transient,
  or revision.
- `SceneDriverTest.pause_produces_one_frame_then_no_more_revision_or_invalidation`
  proves that the runtime stops scheduling after adopting the paused frame.
- `NowPlayingSceneVerificationTest.paused_fog_and_corona_raster_is_pixel_identical_three_seconds_later`
  renders the combined fog and corona before and after 60 repeated paused
  frames (three seconds at 20 Hz) and compares every pixel.

The automated raster is pixel-identical. No physical-device screenshot is
claimed by this record.

### 2. Repeat the same song from the start

Automated: **passed**.

- `SceneStateTest.replay_is_bit_identical_for_single_steps_and_irregular_redraw_jumps`
  proves redraw cadence cannot change the final signal state.
- `NowPlayingSceneVerificationTest.thirty_second_angle_trace_is_identical_across_two_replays`
  records both fog angles for all 600 frames of two independent 30-second runs
  and compares their raw floating-point bits.

### 3. Verse and breakdown screenshots

Automated numeric check: **passed**.

`SceneStateTest.verse_and_breakdown_have_clearly_different_numeric_energy`
proves that the breakdown level is at least twice the verse level.

Outstanding physical-device task: capture one screenshot in a quiet verse and
one in a breakdown from the same analysed track, then confirm that the visual
difference is clear. The numeric check is not presented as those screenshots.

### 4. Greyscale cover

Automated: **passed**.

`CoverFogBitmapTest.greyscale_artwork_stays_greyscale_in_both_fog_layers`
checks representative pixels in both prepared fog layers and proves that red,
green, and blue remain equal. A grey fog is therefore an accepted result.

### 5. Very bright cover

Outstanding physical-device task: play a track with a very bright cover in the
played view and verify the title and both times remain legible. If they do not,
strengthen the top and bottom scrims; do not change the type.

No completion is claimed for this check.

### 6. Track without analysis

Automated: **passed for the data and UI paths**.

- `SceneStateTest.empty_analysis_stays_at_rest_and_never_throws` proves empty
  frames stay at the resting state.
- `MainActivityVisualizerTest.spectrogram_read_uses_the_analysis_lane_and_treats_missing_data_as_ordinary`
  proves the loader delivers `null` without turning absence into an error.
- `MainActivityVisualizerTest.scene_toggle_persistence_idle_controls_and_transition_hit_testing`
  opens the scene with the injected port's ordinary missing-analysis result
  while an already-playing snapshot remains active.

The loader is asynchronous and the screen does not wait for its answer. No
physical-device audio-start measurement is claimed.

### 7. Rapid transition in both directions

Automated functional checks: **passed**.

`MainActivityVisualizerTest.scene_toggle_persistence_idle_controls_and_transition_hit_testing`
switches Player to Visualizer and back, verifies both stored choices, and
operates the same pause semantics node at transition progress 0, 0.5, and 1.
It also verifies the four-second faded state and tap-to-restore behavior.

Outstanding device or emulator task: toggle rapidly several times while
observing frames and confirm there is no visible stutter or double draw. Unit
tests establish the one-Canvas state and hit path, not perceived smoothness.

### 8. Battery per hour in both states

Outstanding physical-device task: measure battery use per hour for the played
view and Visualizer under the same track, brightness, volume, connectivity, and
measurement duration. The played view must be clearly thriftier.

Outstanding emulator task: on `pixel10xl_api37`, record frame callbacks per
second and `dumpsys gfxinfo` for both states. This is performance evidence only,
not a battery result. `adb devices -l` returned no attached target on 2026-08-09,
so neither emulator measurement was run.

## Broader gates observed in this run

- The regenerated UniFFI/native Android build completed successfully after the
  P5 appearance-setting change.
- The complete Android unit suite produced 48 fresh XML suites and 202 tests,
  with zero failures, errors, or skips.
- The complete Rust workspace passed with isolated writable XDG directories.
- `cargo audit --no-fetch` reported only accepted `RUSTSEC-2024-0436` and the
  separately settled `RUSTSEC-2026-0244`; no new advisory appeared.
- Repository-wide formatting and strict Clippy remain red only in untouched
  rebased system-date files. Architecture remains red only because the
  unchanged `reprise-core/src/library/settings.rs` is exactly 800 lines on both
  this checkout and `origin/dev`. P5-owned formatting, strict Android FFI
  Clippy, diff, and edited-file size checks passed.
