---
slug: android-now-playing-desync-throttles-the-scene-b
worktree: /home/marvin/Projects/reprise-android-now-playing-desync-throttles-the-scene-b
branch: feature/android-now-playing-desync-throttles-the-scene-b
phase: shipped
codex_session:
created: 2026-08-22
---

# Strand B — the scene clock stops asking the desktop

Mother plan: `docs/plans/android-now-playing-desync-throttles-the-scene.md`.
Measurements and symptom record:
`docs/plans/android-now-playing-desync-throttles-the-scene.HANDOFF.md`.

Read against `origin/dev` @ `1515487599`, **rebased onto a landed strand A**.
Every line number below comes from `origin/dev`; strand A does not touch these
files, so they hold.

## Preconditions

**Strand A must be landed before this strand is started.** Its acceptance is a
frame-rate measurement on the phone, and on a branch where the UI's playback
state still goes stale that measurement returns the 155-frame arm no matter what
this strand does. Do not start here on an un-landed A.

**Strand C must not be started before this strand's first commit is in.** Task
B-1 moves the function C rewrites.

## Purpose

The frame loop throttles to 50 ms per frame — measured ~16 fps — for two reasons
that meet in one line (`SceneDriver.kt:247-250`):

```kotlin
if (!visualizerActive || frames.frameCount == 0 && frameSink == null) {
    delay(PAUSED_SCENE_FRAME_INTERVAL_MS)   // 50 ms
}
withFrameNanos { if (driver.tick()) drawRevision += 1 }
```

1. `visualizerActive` is the **UI's belief** about playback
   (`PlaybackUiState.kt:35-36`). Strand A makes that belief true; this strand
   makes the throttle stop depending on it being true.
2. `frames` is the **desktop's** spectrogram. `rememberSpectrogram`
   (`NowPlayingScene.kt:373-385`) falls back to
   `SpectrogramFrames(24, 20, ByteArray(0))` — `frameCount == 0` — for every
   track no Reprise desktop ever analysed, and the live on-device analysis that
   would fill the gap is switched on only while the spectrum bars are visible
   (`NowPlayingScene.kt:122-123`, `enabled = visualizerOpacity > 0f`).

A phone that never met a desktop therefore gets the throttled scene by
construction. After this strand the scene runs on what the app can hear, and the
stored spectrogram is an accelerator, never a precondition.

**Explicitly out of scope, by user decision:** no `FrameRateCategory` request is
added for cover mode. Cover measured 664 frames / 10 s at 0 % jank with the
platform's 60 Hz override, and `High` would buy little for a slowly drifting
image. `High` stays exclusive to the bars —
`requestedVisualizerFrameRateCategory` (`NowPlayingScene.kt:74-81`) and
`NowPlayingFrameRateTest.kt` are unchanged, and must stay green as they are.

## File ownership

```
android/app/src/main/java/de/reprise/spike/SceneDriver.kt
android/app/src/main/java/de/reprise/spike/NowPlayingScene.kt
scripts/android-scene-framerate.sh                            (new)
```

plus their tests under `android/app/src/test/java/de/reprise/spike/`:
`SceneDriverTest.kt`, `DriveSceneComposeTest.kt`, `ScenePowerGateTest.kt`,
`VisualizerSceneDriverTest.kt`, `NowPlayingSceneVerificationTest.kt`,
`NowPlayingFrameRateTest.kt`, and anything new you add.

Strand A leaves `PlaybackUiState.kt` with a second, position-free
`LibraryPlayback` record for the whole library tree. That separation keeps the
500 ms position tick out of the list; a later scene rebuild must not merge the
two records back together.

`NowPlayingScene.kt` is at 624 of its 800 allowed lines and task B-1 takes ~25
out of it. Keep it that way: new code goes into a new file.

**One deliberate exception:** task B-1 moves a function **into**
`VisualizerScene.kt`, which is strand C's file. That is the single commit in
which you touch it. You do not touch it again.

## What is **not** yours

- `MainActivity.kt`, `ReprisePlaybackService.kt`, `Media3PlaybackPort.kt`,
  `NowPlayingState.kt`, `PlaybackUiState.kt`, `MobileSurfaceViewModel.kt`,
  `NowPlayingSheet.kt` — strand A, already landed. Do not "improve" them.
- `crates/reprise-android-ffi/**` — strand C.
- `VisualizerScene.kt` after task B-1 — strand C.
- Everything else in the repo.

## Test discipline

First the test, then the run that sees it fail, then the implementation. A test
green on its first run has measured nothing. `BUILD SUCCESSFUL` is not proof:
the verdict lives in `android/app/build/test-results/testDebugUnitTest/*.xml`.
Every Gradle call gets `JAVA_HOME=/usr/lib/jvm/java-21-openjdk`.

---

## Task B-1 — move `drawPlayedVisualizer` where its buffer lives

**Goal:** the function that decodes the scene buffer sits in the file that owns
the scene buffer format, so strand C can change that format without reaching into
this strand's file. **No behaviour change whatsoever.**

**Files:**
- Modify: `android/app/src/main/java/de/reprise/spike/NowPlayingScene.kt`
  (`:591-612` moves out; `:64-65`, `:234-244` adapt)
- Modify: `android/app/src/main/java/de/reprise/spike/VisualizerScene.kt`
  (receives it) — **the only time this strand touches this file**
- Test: whichever of `VisualizerScenePixelsTest.kt` /
  `NowPlayingSceneVerificationTest.kt` currently exercises it moves with it

### What moves

`drawPlayedVisualizer` (`NowPlayingScene.kt:591-612`) and nothing else. Its
dependencies today:

| Dependency | Where it lives | How the move handles it |
|---|---|---|
| `COVER_SIZE_DP` | `NowPlayingScene.kt:64`, private | becomes a `side: Float` parameter |
| `COVER_RADIUS_DP` | `NowPlayingScene.kt:65`, private | becomes a `radius: Float` parameter |
| `playedCoverRect` | `NowPlayingScene.kt:614`, internal | stays; same package, visible |
| `drawCoverShadow`, `CoverShadowBitmap` | same package | stay; visible |
| `AmbientTrueBlack` | same package | stays; visible |

New signature in `VisualizerScene.kt`:

```kotlin
internal fun DrawScope.drawPlayedVisualizer(
    buffer: List<Float>,
    center: Offset,
    side: Float,
    radius: Float,
    shadow: CoverShadowBitmap?,
    opacity: Float = 1f,
)
```

and the call site (`NowPlayingScene.kt:235-243`) passes
`side = COVER_SIZE_DP.dp.toPx()`, `radius = COVER_RADIUS_DP.dp.toPx()` — the two
values the body computed for itself at `:597` and `:599`. The two private
constants stay private in `NowPlayingScene.kt`; `VisualizerScene.kt` must not
learn the cover's dimensions.

### Proof that nothing changed

This commit is a refactor, so the proof is that the **existing** pixel tests pass
unchanged. Do not write a new test for it; do run:

```sh
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/verify-now-playing-scene.sh > $LOG/b1-scene.log 2>&1
```

and quote the suite/test totals in the report. If a test file had to move with
the function, say so — a moved test that still asserts the same pixels is fine, a
weakened assertion is not.

Commit this **alone**, as `refactor: …`, before anything else in this strand.

---

## Task B-2 — the live analysis engine runs while the screen is visible

**Goal:** the app's own ears are open whenever the Now Playing screen is up, not
only while the bars are drawn. This is the user's decision 1, and it is what
makes a desktop-less phone able to drive the scene at all.

**Files:**
- Modify: `android/app/src/main/java/de/reprise/spike/NowPlayingScene.kt`
  (`:122-127`, `:234-244`, `:289-309`)
- Test: `android/app/src/test/java/de/reprise/spike/VisualizerSceneDriverTest.kt`
  (or a new test file)

### Step 1: write the failing test

- `theSceneEngineExistsWhileTheScreenIsUpWithTheCoverShowing` — compose
  `NowPlayingScene` with `visualizerOpacity = 0f` through a recording
  `VisualSceneEngineFactory` and assert an engine was created and a frame sink
  reached `DriveScene`.
- **The regression guard, and it is the important one:**
  `theCoverArmDoesNotBuildASceneItNeverDraws` — same composition, and assert the
  engine's `scene(...)` was **not** called. Today `visualEngine.scene(...)` is an
  *argument* at `NowPlayingScene.kt:236-239`, so it is evaluated before
  `drawPlayedVisualizer` reaches its `if (opacity <= 0f) return` at `:601`. With
  the engine always present and no guard, cover mode would start building 1540
  shapes per frame and boxing them into a `List<Float>` — the exact cost strand C
  exists to remove, newly paid in the arm that measured 0 MB of GC garbage.

### Step 2: implement

- `rememberVisualSceneEngine` (`:290-309`) loses its `enabled` parameter; the
  engine is created whenever `NowPlayingScene` is composed. `remember(factory, enabled)`
  at `:297` becomes `remember(factory)`. Disposal is unchanged — the existing
  `DisposableEffect(engine) { onDispose { engine?.close() } }` (`:298-300`)
  releases the PCM lease when the screen goes away.
- Guard the draw, at `:234`:

  ```kotlin
  if (visualEngine != null && visualizerOpacity > 0f) {
      drawPlayedVisualizer(
          buffer = visualEngine.scene(...),
          ...
      )
  }
  ```

**Accepted consequence, and it must be named in the report:** decoded PCM is now
ingested (downmix, FFT, bass detection — `visualizer.rs:274-332`) whenever the
Now Playing screen is up, including in cover mode. The cover arm of the GC
measurement is therefore no longer expected to read 0 MB. The mother plan's
cross-check 5 compares against the *new* cover baseline, not against the 0 MB in
the handoff.

**Also accepted, and stated in the mother plan:** with a live sink present,
`SceneDriver.tick` takes its live branch (`SceneDriver.kt:79-85`) and
`fallbackBands` (`:96-118`) no longer runs, so the stored 24-band/20 Hz
spectrogram stops driving the scene. It keeps its role in the seek bar's colours
(`SpectralSeekTrack`, `NowPlayingSheet.kt:351`). **Do not delete the spectrogram
path** — `rememberSpectrogram`, `SceneDriver.fallbackBands` and their tests stay
correct and stay green.

---

## Task B-3 — the throttle asks the audio, not the UI

**Goal:** the cheap interval is reachable only when neither the audio nor the UI
says something is playing. The UI's belief may still *add* frames; it can no
longer *remove* them.

**Files:**
- Modify: `android/app/src/main/java/de/reprise/spike/SceneDriver.kt` (`:232-255`)
- Test: `android/app/src/test/java/de/reprise/spike/DriveSceneComposeTest.kt`

### Step 1: write the failing test

- `aStalePausedUiStateDoesNotThrottleWhileAudioIsFlowing` — drive `DriveScene`
  with a `PlaybackUiState` whose `state` is `PAUSED` and a `SceneFrameSink` whose
  `hasLiveAudio()` returns true, and assert the loop does not take
  `PAUSED_SCENE_FRAME_INTERVAL_MS`. This is the 155-frame arm, reproduced in a
  unit test.
- `noStoredSpectrogramDoesNotThrottleWhenTheSinkIsListening` — `frames.frameCount == 0`
  with a non-null sink, playing. Not throttled.
- **The battery guard, and it must stay green:**
  `aGenuinelyPausedScreenStillTakesTheCheapInterval` — UI paused *and*
  `hasLiveAudio()` false. Throttled. This is why the throttle exists; losing it
  is a failure of this strand, not a side effect.

Assert on the *interval*, not on a frame count: `DriveSceneComposeTest` already
has the seam for driving the frame clock. A test that counts frames on a real
clock is flaky and is not acceptable here.

### Step 2: implement

```kotlin
val sink = frameSink
…
do {
    val audible = sink?.hasLiveAudio() == true
    if (!(audible || visualizerActive) || frames.frameCount == 0 && frameSink == null) {
        delay(PAUSED_SCENE_FRAME_INTERVAL_MS)
    }
    withFrameNanos { if (driver.tick()) drawRevision += 1 }
} while (animationsEnabled)
```

Three notes on this shape:

- **Why `||` and not a replacement.** `hasLiveAudio()` is false whenever no PCM
  reaches `LivePcmBufferSink` — the first frames of a track, and any decode path
  that bypasses `TeeAudioProcessor`. Dropping the UI's belief entirely would trade
  one silent throttle for another. Keeping it as an *additional* reason to run
  means a stale "paused" can no longer throttle, and a stale "playing" can only
  cost frames, never lose them.
- **Cost.** `hasLiveAudio()` crosses UniFFI and takes the engine's state mutex
  (`visualizer.rs:362-369`) once per loop iteration, up to ~120/s. That is small
  beside `scene()`, which holds the same mutex across a 1540-shape build; strand
  C shortens that critical section.
- `visualizerActive` must stay in the `LaunchedEffect` keys — a change in the UI's
  belief still has to restart the loop.

---

## Task B-4 — the frame loop stops being torn down twice a second

**Goal:** the loop is not cancelled and relaunched on every position tick.

**Files:**
- Modify: `android/app/src/main/java/de/reprise/spike/SceneDriver.kt` (`:235-242`)
- Test: `android/app/src/test/java/de/reprise/spike/DriveSceneComposeTest.kt`

### Step 1: write the failing test

`aPositionTickDoesNotRestartTheFrameLoop` — recompose `DriveScene` with a changed
`playback.positionMs` and assert the loop was not cancelled (count loop entries,
or observe that `noteFramesWithheld` was not called for it).

### Step 2: implement

Remove `playback.positionMs` from the `LaunchedEffect` key list
(`SceneDriver.kt:240`). It changes every 500 ms
(`Media3PlaybackPort.kt:24 POSITION_INTERVAL_MS = 500L`) and tears the loop down
twice a second for nothing: the position already reaches the driver through
`positionSample` and `SideEffect { source.update(positionSample) }`
(`SceneDriver.kt:216-226`), which is unaffected by the loop's lifetime.

Leave the other keys alone. `frameSink` in particular must stay: after task B-2
it is remembered on the engine (`NowPlayingScene.kt:128`) and so is stable across
recompositions anyway.

---

## Task B-5 — the measurement script

**Goal:** the device protocol from the handoff exists as a script that refuses to
produce an invalid run, because two traps cost several invalid runs when this was
measured by hand.

**Files:**
- New: `scripts/android-scene-framerate.sh`

This is the one task in this plan that needs a phone. Write the script; the human
runs it. It is not part of any automated gate.

The script takes a window length (default 10 s) and a label for the arm, and:

1. **Refuses to measure a screen that is not on top.** With the notification shade
   pulled down the app renders nothing at all —
   `dumpsys gfxinfo org.reprise` reported `Total frames rendered: 0` while
   `media_session` still said `PLAYING`. Check the resumed activity, and take a
   screenshot inside the window as evidence.
2. **Reads the playback state at both ends of the window**, from
   `dumpsys media_session`, and fails the run if the two ends disagree or if
   either is not `PLAYING`. No arm may be a paused arm in disguise.
3. **Compares the on-screen time label with the `media_session` position.** This
   is the check that exposed the bug in the first place; a screenshot alone
   proves nothing. If the two diverge by more than a window's worth of playback,
   the run is reported as **desynced** and its frame numbers are labelled as
   such rather than silently averaged in.
4. **Runs a control arm.** Same track, same window length, the other visualizer
   mode. A number without its control arm is not evidence.
5. Reports: `Total frames rendered`, `Janky frames`, the 50th/90th/95th
   percentiles, and the GC bytes freed in the window from
   `adb logcat --pid $(adb shell pidof org.reprise)`, per arm, as one table.

The measurement itself, for reference:

```sh
adb shell dumpsys media_session | grep -A8 "package=org.reprise" | grep -m1 "state="
adb shell dumpsys gfxinfo org.reprise reset
sleep 10
adb shell dumpsys gfxinfo org.reprise | grep -E "Total frames|Janky frames:|percentile"
adb shell dumpsys media_session | grep -A8 "package=org.reprise" | grep -m1 "state="
```

Two hard rules for the script itself, both learned the expensive way in this
repo:

- **Never read a verdict through a pipe.** `script | tail` reports `tail`'s exit
  status, which is always 0. Write to a file, then grep the file.
- Set `set -euo pipefail`, and make every failed precondition exit non-zero with
  a sentence saying which precondition failed. A run that silently measures the
  wrong thing is worse than no run.

Numbers to beat, from the handoff, on the same phone:

| UI state | Visualizer | Frames / 10 s | Janky |
|---|---|---|---|
| in sync | cover | 664 / 656 | 0.15 % / 0 % |
| in sync | spectrum | 1216 | 0.08 % |
| desynced | cover | 155 / 157 | 0 % |
| desynced | spectrum | 190 / 12 s | 48 % |

---

## Acceptance for this strand

```sh
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  android/gradlew --project-dir android :app:testDebugUnitTest \
  --tests 'de.reprise.spike.SceneDriverTest' \
  --tests 'de.reprise.spike.DriveSceneComposeTest' \
  --tests 'de.reprise.spike.ScenePowerGateTest' \
  --tests 'de.reprise.spike.VisualizerScene*' \
  --tests 'de.reprise.spike.NowPlaying*' > $LOG/b-suite.log 2>&1

JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/verify-now-playing-scene.sh > $LOG/b-scene.log 2>&1

JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/check-android-suite.sh > $LOG/b-android.log 2>&1
grep -E '^suites=' $LOG/b-android.log        # failures=0 errors=0 verdict=fresh

shellcheck scripts/android-scene-framerate.sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

The device measurement is run by the human, on the branch, before the merge:
a track with **no** stored spectrogram, playing, cover **and** spectrum, each
with its control arm and its state verified at both ends. Expected: both arms
leave the ~16 fps band; cover lands near the platform's 60 Hz cap, spectrum near
the panel's 120 Hz.

## For the report

- The red run of every new test, quoted.
- B-1's proof that the pixel suite is unchanged, with its totals.
- The new cover-arm GC baseline created by B-2, so cross-check 5 is not read
  against the handoff's 0 MB.
- The device table, both arms, with the state readings at both ends of each
  window.
