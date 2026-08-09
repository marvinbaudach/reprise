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

## The emulator run of 2026-08-09, 20:20-21:00

The outstanding device tasks were carried as far as an emulator can carry them.
Setup, so the numbers can be read for what they are:

- `pixel10xl_api37`, cold boot, headless (`-no-window`), **software GPU**
  (`-gpu swiftshader_indirect`). Screen 1344x2992.
- App built from `1bf03376ea` (`scripts/android-build.sh`, then
  `assembleDebug`), installed as `org.reprise`.
- Library: three tracks pushed to `/sdcard/Music/Reprise`, each beside a
  `.reprise-analysis` sidecar encoded from this machine's desktop database, so
  the phone imports real spectrogram and waveform data rather than a synthetic
  fixture. The spectral seek bar rendering its colours is the visible proof
  that the import path ran.
  - `The Shadowing` (Evergloam) — chosen because its spectrogram has a genuine
    quiet passage at 154-166 s (mean band byte 27) and a breakdown at
    168-182 s (mean band byte 145), a factor of 5.3.
  - `Dreh auf!` (We Butter The Bread With Butter) — chosen for the brightest
    cover in the library that also carries analysis: mean luma 213/255 with a
    10th percentile of 204, so there is no dark region anywhere in the image.
  - `Wasteland` (Stray View) — a second analysed track, used for queue moves.

**What the software GPU does to the numbers.** Frame times are one to two
orders of magnitude off a real device and every frame counts as janky. The
absolute figures below are therefore worthless as performance results. The
*ratio* between the two states was measured twice under identical conditions
and is stable, so the direction it shows is evidence; the magnitude is not.

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

The automated raster is pixel-identical.

Emulator, 2026-08-09: **confirmed on the running app**. Paused mid-track in the
played view, two full-screen captures four seconds apart are pixel-identical
across the whole surface below the status bar — the difference bounding box is
empty, not merely small. The status bar is excluded because the system clock
ticks there; nothing the app draws moves.

No physical-device screenshot is claimed by this record.

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

Emulator, 2026-08-09: the screenshots were taken, on `The Shadowing`, at 163 s
(the quiet passage) and 180 s (the breakdown). The answer differs by state.

**Visualizer: passed, and not narrowly.** Mean luma over the scene rises from
67.2 to 108.0 — a 61 % lift — and the corona ring grows visibly in both radius
and brightness while the core shape swells. Nobody would call the two frames
similar.

**Played view: does not pass.** Over the same two moments the fog region moves
but does not brighten: mean luma 60.7 against 61.8, a 2 % difference, and the
band immediately around the cover 65.6 against 65.8. An amplified difference
image shows the change is drift — the fields have rotated to new positions —
not a response to a five-fold change in energy. By the brief's own wording,
"if they look similar the scaling is too flat", and here they look similar.

Whether that is a defect depends on a decision this record cannot take: the
played view carries a cover, a title and an artist line that must stay legible,
so a fog that swings with the music may be unwanted there by design. What the
measurement establishes is that today the played view's fog is effectively not
energy-driven to the eye. The owner should say which of the two it is.

### 4. Greyscale cover

Automated: **passed**.

`CoverFogBitmapTest.greyscale_artwork_stays_greyscale_in_both_fog_layers`
checks representative pixels in both prepared fog layers and proves that red,
green, and blue remain equal. A grey fog is therefore an accepted result.

### 5. Very bright cover

Emulator, 2026-08-09: **failed for the title, passed for the times.**

`Dreh auf!` was played in the played view — the brightest analysed cover in the
library, mean luma 213/255 with a 10th percentile of 204, so the fog it
produces is near-white everywhere. Measured contrast of the drawn glyphs
against their own local background, WCAG relative luminance:

| element | contrast | verdict |
| --- | --- | --- |
| title `Dreh auf!` | **2.32:1** | below 3:1, the floor for large text |
| artist line | **1.28:1** | barely visible at all |
| elapsed time `1:50` | 12.42:1 | fine |
| remaining time `−1:11` | 8.28:1 | fine |

The two times sit on the black area under the fog and are never at risk. The
title and the artist line sit directly on the fog, and against a cover this
bright the white type all but disappears; the artist line is the worse of the
two, and the brief does not even name it.

The remedy the brief prescribes applies: **strengthen the top and bottom
scrims, do not change the type.** That change is not made by this record.

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

Emulator, 2026-08-09: **survived the burst; smoothness remains unjudged.**

Six round trips Player → Visualizer → Player were driven back to back, each leg
0.8 s apart, so every transition began while the previous 320 ms animation had
only just finished. Afterwards:

- the app was in the played view with the full control set present and
  playback still running at 1:51 of 3:44 — no stuck intermediate state,
- `logcat` over the burst holds no `FATAL`, no `ANR`, and no error line from
  the app's own process (the `MediaPlayerWrapper: Timeout while waiting for
  metadata to sync` lines come from pid 998, the system's Bluetooth media
  session, and appear with the app idle as well),
- the pause target still works: tapping it flipped the control to `Play`.

What cannot be claimed: **there is no perceived-smoothness result.** At 7 to 14
frames per second on a software GPU, stutter and double draw are not
distinguishable from the renderer itself. That judgement still needs a real
device.

### 8. Battery per hour in both states

Emulator, 2026-08-09: **the emulator half is done and points the right way.**

Both states measured over 30 s on the same track from the same position
(`The Shadowing`, seeked to ~20 s), `dumpsys gfxinfo org.reprise reset` before
each window:

| state | frames in 30 s | frames/s | 50th | 90th |
| --- | --- | --- | --- | --- |
| played view | 434 | 14.5 | 150 ms | 150 ms |
| Visualizer | 216 | 7.2 | 250 ms | 300 ms |

A first pass on a different track gave 362 / 213 frames and the same two median
frame times, so the ratio is reproducible: the played view issues **twice** the
frames at **0.6×** the cost each. The played view is the thriftier of the two,
which is the direction the brief demands.

It is not the answer the brief asks for. Every frame here is janky and the GPU
percentiles are absurd, because this is `swiftshader` and not a GPU; the
absolute numbers say nothing about a phone.

Outstanding physical-device task, unchanged: battery use per hour for both
states under the same track, brightness, volume, connectivity, and duration.

## Found while looking, not asked for by the eight checks

**The cover fog shows the straight edges of its own texture.** In the played
view, two large translucent quadrilaterals with visibly straight sides sweep
across the whole screen behind the cover. They are the fog: `CoverFogBitmap`
box-blurs the artwork into a **fixed 256 px square** (`WIDE_BLUR_RADIUS_PX` 18,
`TIGHT_BLUR_RADIUS_PX` 10, two passes) and every frame only scales and rotates
that finished texture. A box blur inside a fixed canvas cannot soften the
canvas border, so the square's own edge survives the blur, and scaling it up
to screen size turns that edge into a long straight seam. Rotating it makes the
seam sweep.

This is not an emulator artifact — the blur is plain CPU arithmetic in
`CoverFogBitmap.kt`, identical on any device. It is what makes the played view
read as overlapping panels rather than the soft fields the brief describes. A
transparent margin around the source square, or a radial alpha falloff before
the blur, would remove it.

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

## What is still open after the emulator run

| check | state |
| --- | --- |
| 1 pause | closed — automated and confirmed on the running app |
| 2 replay | closed — automated |
| 3 verse vs breakdown | Visualizer closed; **played view fails the "must differ clearly" wording**, decision needed |
| 4 greyscale cover | closed — automated |
| 5 very bright cover | **fails** — title 2.32:1, artist line 1.28:1; scrims need strengthening |
| 6 track without analysis | closed — automated |
| 7 rapid transition | functionally closed; perceived smoothness needs a real device |
| 8 both states | direction confirmed on the emulator; **battery per hour needs a real device** |

Two things genuinely need hardware and nothing else will do: the battery figure
and the smoothness judgement. Everything else on the list has an answer now.
