---
slug: android-now-playing-desync-throttles-the-scene
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-22
strands: a,b,c
merge_order: a,b,c
---

# The Now Playing scene follows the music, with or without a desktop (mother plan)

This plan claims **no** branch: `worktree:` and `branch:` stay empty. The work is
built in three strand files, each with its own worktree, its own status block and
its own file ownership:

| Strand | File | Content |
|---|---|---|
| A | `android-now-playing-desync-throttles-the-scene-a.md` | The UI follows the service — the state channel, the position ticker, the frozen seek head |
| B | `android-now-playing-desync-throttles-the-scene-b.md` | The scene clock stops asking the desktop — the throttle, the live engine gate, the measurement script |
| C | `android-now-playing-desync-throttles-the-scene-c.md` | The per-frame cost of the bars — the boxed buffer, the per-frame brush, the critical section |

Codex is given **one** strand file at a time. Each one is readable alone and
repeats the test discipline in full. This mother plan is for the human who starts
the strands and merges them.

**They run one after another: A, then B, then C.** The reasons are in
[Parallelism](#parallelism); the short version is that B's acceptance cannot go
green before A lands, and C's first file does not exist until B's first commit
has moved it.

Measurements, the full symptom record and the reproduction protocol:
`docs/plans/android-now-playing-desync-throttles-the-scene.HANDOFF.md`.

Read against `origin/dev` @ `1515487599`. Every line reference comes from that
state; whoever cannot find one is on a different base.

---

## What is wrong

The Android Now Playing screen animates at ~16 fps while music plays and looks
badly stuttery. Measured on a Pixel 10 Pro XL (GrapheneOS, 120 Hz panel),
`org.reprise` 0.1.25 release build, playback state read from
`dumpsys media_session` at **both ends** of every window:

| UI state | Visualizer | Frames / 10 s | Janky | p50 | p90 |
|---|---|---|---|---|---|
| in sync | cover | 664, repeat 656 | 0.15 %, 0 % | 8 ms | 10 ms |
| in sync | **spectrum** | **1216 → 121.6 fps** | 0.08 % | 11 ms | 13 ms |
| desynced | cover | 155, repeat 157 | 0 % | 10 ms | 11 ms |
| desynced | spectrum | 190 / 12 s | 48 % | 15 ms | 19 ms |

A factor of **7.7** between the two UI states, and the visualizer mode barely
moves it. The renderer is not the problem: frame *times* are healthy in every arm
and the spectrum reaches 121.6 fps when the state is right. What collapses is how
many frames the app **asks for**, and it collapses for two independent reasons
that meet in one line (`SceneDriver.kt:247-250`):

```kotlin
if (!visualizerActive || frames.frameCount == 0 && frameSink == null) {
    delay(PAUSED_SCENE_FRAME_INTERVAL_MS)   // 50 ms → measured ~16 fps
}
withFrameNanos { if (driver.tick()) drawRevision += 1 }
```

1. **`visualizerActive` is a lie.** It is derived from the UI's own copy of the
   playback state (`PlaybackUiState.kt:35-36`), and that copy goes stale. The
   time label froze at `2:08` while the session ran on past `4:02`; after an
   automatic track change the screen still showed the previous track with a Play
   button while the service played the next one; in that state the play button
   did nothing. `am force-stop` + relaunch repaired it and the frame count went
   from 155 to 664 under identical conditions.
2. **`frames` is the desktop's spectrogram.** It comes from
   `readSpectrogram(trackId)` (`NowPlayingScene.kt:373-385`) and falls back to
   `SpectrogramFrames(24, 20, ByteArray(0))` — `frameCount == 0` — for any track
   the desktop never analysed. On-device live analysis exists (`LivePcmAudio.kt`
   → `ingest_pcm_i16`, `visualizer.rs:274-332`) but is switched on only while the
   spectrum bars are visible (`NowPlayingScene.kt:122-123`,
   `enabled = visualizerOpacity > 0f`).

So a phone that never met a Reprise desktop gets the throttled scene by
construction. That contradicts the standing decision that the mobile app is
complete on its own and that visualisation is fed live from the playback stream.

## The root cause behind symptom 1

The state channel from the service to Compose is a **single nullable callback
slot at every hop**, push-only, with no reconciliation and no supervision. There
is no `Flow`/`StateFlow` and no polling anywhere in the path.

```
ExoPlayer Player.Listener
  → Media3PlaybackPort.eventBridge          (Media3PlaybackPort.kt:35, 246)
  → Rust SessionInner::handle_event
  → ReprisePlaybackService.coreListener     (ReprisePlaybackService.kt:46-64)
  → ReprisePlaybackService.observer         (ReprisePlaybackService.kt:31)   ← breaks here
  → MainActivity.playbackState              (MainActivity.kt:170, 180-186)
```

- `observer` is one slot, not a list: `observer?.invoke(snapshot)`
  (`ReprisePlaybackService.kt:50`) drops the snapshot **silently** when it is
  null, while `latestPlaybackSnapshot` in the same method keeps the truth.
- `MainActivity.onStop()` nulls it unconditionally through `detachObserver()`
  (`MainActivity.kt:440-451`). The **only** re-arm is a fresh
  `onServiceConnected` after `onStart`'s `bindService` (`MainActivity.kt:424-430`,
  `176-195`); nothing else in the file ever calls `attachObserver` again.
- When that callback does not arrive, `observer` stays null forever *and*
  `MainActivity.playbackService` stays null. Then `runPlaybackCommand`
  (`MainActivity.kt:695-709`) returns early with "playback is still connecting" —
  which is exactly the dead play button — and `playbackState` is frozen at the
  last snapshot it received, which is exactly the old track with a Play icon,
  which makes `visualizerActive` false, which takes the 50 ms throttle.
- `attachObserver` republishes `coreSession.snapshot()` immediately
  (`ReprisePlaybackService.kt:180-183`), so a *successful* re-attach is
  self-healing. The bug needs a re-bind that attaches nothing, not merely a race.

**Not verified:** the platform-level reason a `bindService()` call would not
redeliver `onServiceConnected`. That is Binder/ActivityManager behaviour and is
not expressed in this code. What *is* established is a single point of failure
that fits the symptom exactly — and the plan therefore does two things rather
than one: it removes the single slot, and it makes a bind that attaches nothing
loud and recoverable instead of silent and permanent.

There is a **second, independent freeze path**, and it matches the *first*
symptom of the session (time label stuck at 2:08 while the play/pause button
still correctly showed playing): `SeekPositionState.acceptSnapshot`
(`NowPlayingState.kt:80-81`) returns `this` unchanged for every incoming snapshot
while `isDragging` is true. A drag whose pointer-up is lost freezes the position
readout on its own, without touching the rest of the UI state. A third candidate
for the same symptom lives one hop lower: `Media3PlaybackPort.positionTicker`
(`Media3PlaybackPort.kt:42-55`) returns without rescheduling itself whenever
`player.isPlaying` is momentarily false, and is restarted only from
`onIsPlayingChanged` (`Media3PlaybackPort.kt:57-64`). All three are strand A's.

## What "done" means

- While audio is playing, the Now Playing scene runs at the display's rate on a
  phone that has **never** synced with a desktop, with the spectrum on *and* off.
- The UI's track, position and play/pause state follow the service across an
  automatic track transition and across an Activity rebind, and the play button
  works in every state the UI can reach.
- The stored spectrogram is an accelerator, never a precondition.
- A genuinely paused screen still takes the cheap interval — the battery
  behaviour this throttle exists for is not lost.
- Verified by the device protocol in the handoff, run through
  `scripts/android-scene-framerate.sh`: a `dumpsys gfxinfo` window with the
  playback state read at both ends *and* the on-screen time label compared
  against the `media_session` position, plus a control arm.

## Decisions taken with the user (2026-08-22)

The draft `…-desync-throttles-the-scene.draft.md` was grilled with the user and
is deleted by this plan. Settled, and binding for the strands:

1. **The live analysis engine runs while the Now Playing screen is visible** —
   not always, and not only while the spectrum bars are shown. Today it is gated
   on `visualizerOpacity > 0f` (`NowPlayingScene.kt:122-123`).
2. **Strand A rebuilds the channel as a `StateFlow` in the service**, plus an
   explicit reconcile when the screen becomes visible. Not a patched-up single
   slot.
3. **Cover mode keeps its 60 Hz.** No `FrameRateCategory` request is added for
   the fog: it measured 664 frames / 10 s at 0 % jank, and `High` would buy
   little for a slowly drifting image. `High` stays exclusive to the bars. The
   corresponding item in the draft's strand B is **dropped**.
4. **All three strands stay in this plan**, including the per-frame cost work.
5. **The B/C seam is resolved by a move:** strand B's first commit relocates
   `drawPlayedVisualizer` (`NowPlayingScene.kt:591-612`) into `VisualizerScene.kt`
   with no behaviour change. After that strand C owns the buffer type end to end.
   **Merge order A → B → C.**
6. **`scripts/android-scene-framerate.sh` is written and owned by strand B.**
7. Assistant's own cut, not a user decision: the `isDragging` freeze goes into
   strand A, same file group and same symptom class.

**Consequence made explicit and accepted:** once live analysis feeds the scene,
`SceneDriver` takes its live branch and `fallbackBands` (`SceneDriver.kt:96-118`)
no longer runs, so the stored 24-band/20 Hz spectrogram stops driving the scene
entirely. It keeps only its role in the seek bar's colours
(`SpectralSeekTrack`, `NowPlayingSheet.kt:351`). No strand may delete the
spectrogram path; it must stay correct for that use and for its tests.

## Global constraints

- Chat/answers in German, everything in the repo (code, comments, commit
  messages, test names, these plans' code identifiers) in English.
- Commit format: `<type>: <description>`, types
  `feat|fix|refactor|docs|test|chore|perf|ci`.
- Files stay under 800 lines (`check-architecture.sh` enforces it). Two files in
  play are already close: `MainActivity.kt` (739) and `NowPlayingScene.kt` (624).
  Growth goes into a new file, not into these.
- **TDD is binding:** first the test, then the run that sees it fail, then the
  implementation. A test that passes without the implementation has measured
  nothing and is to be discarded. The red run belongs in the report.
- **The `Files:` list of a task is a starting point, not a fence** — but the
  strand's *ownership* section is a fence. Needing a file outside it is a finding
  for the report, not a change.
- No device, no `adb`, no emulator inside a task, with exactly one exception:
  strand B's measurement task, which is run by the human on the phone.

## Test commands

`$LOG` is a working directory outside the repo, e.g. `/tmp/reprise-nowplaying`.
Never read a whole log back; answer the question with `grep`/`wc`.

```sh
# Rust bridge (strand C)
TMPDIR=/tmp cargo test --locked -p reprise-android-ffi > $LOG/ffi.log 2>&1
grep -c '^test result: FAILED' $LOG/ffi.log          # must be 0

# Rust core regression (strand C touches nothing here, but reads it)
cargo test --locked -p reprise-core > $LOG/core.log 2>&1
grep -c '^test result: FAILED' $LOG/core.log         # must be 0

# Android, full gate (regenerates the UniFFI bindings)
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/check-android-suite.sh > $LOG/android.log 2>&1
grep -E '^suites=' $LOG/android.log                  # failures=0 errors=0 verdict=fresh

# Android, narrow run while working
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  android/gradlew --project-dir android :app:testDebugUnitTest \
  --tests 'de.reprise.spike.SceneDriverTest' > $LOG/android-narrow.log 2>&1
ls -l android/app/build/test-results/testDebugUnitTest/   # fresh? otherwise nothing ran

# The scene suite, which all three strands must keep green
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/verify-now-playing-scene.sh > $LOG/scene.log 2>&1
```

Hard environment facts, measured and not negotiable:

- The Android suite needs **JDK 21**. The system default here is JDK 26, and
  JDK 26 kills Robolectric. Set `JAVA_HOME` before *every* Gradle call.
- The FFI tests depend on `readdir` order; the suite is green with `TMPDIR=/tmp`.
- `BUILD SUCCESSFUL` is not proof: Gradle reports `:app:testDebugUnitTest` as
  up-to-date, exits 0 and runs nothing. The verdict lives in
  `android/app/build/test-results/testDebugUnitTest/*.xml`, whose freshness
  `scripts/check-android-suite.sh:33-77` checks.
- `cargo test --exact` runs into the void easily. Evaluate with
  `grep -c '^test result: FAILED'` on the log file, not by looking at the last
  line.
- **Kotlin does not compile before the Rust side stands.** The UniFFI bindings
  under `android/app/src/main/java/uniffi/` are generated and gitignored;
  `scripts/check-android-suite.sh` deletes and regenerates them from
  `libreprise_android_ffi.so`. Strand C changes `scene()`'s signature, so its
  Kotlin half is only compilable after its Rust half — within the same strand,
  in that order.
- Formalities at the end of every strand: `cargo fmt --all`,
  `cargo clippy --all-targets --all-features -- -D warnings`.

## Parallelism

Three strands, cut by file ownership, run **sequentially**:
`merge_order: a,b,c`.

| Strand | Owns |
|---|---|
| A | `MainActivity.kt`, `ReprisePlaybackService.kt`, `Media3PlaybackPort.kt`, `NowPlayingState.kt`, `PlaybackUiState.kt`, `MobileSurfaceViewModel.kt`, `NowPlayingSheet.kt` + their tests |
| B | `SceneDriver.kt`, `NowPlayingScene.kt`, `scripts/android-scene-framerate.sh` + their tests; **plus the one move commit** into `VisualizerScene.kt` |
| C | `crates/reprise-android-ffi/src/visualizer.rs`, `VisualizerScene.kt` + their tests; **plus the single `drawPlayedVisualizer` call site** in `NowPlayingScene.kt` |

### Why not in parallel

- **B after A.** B's acceptance is "the scene runs at display rate while
  playing". Measured on a branch where the UI state still goes stale, that
  measurement is a lie — it is the exact measurement that produced the 155-frame
  arm. B cannot be judged before A has landed.
- **C after B.** After B's first commit `drawPlayedVisualizer` no longer lives in
  `NowPlayingScene.kt`. C changes the buffer type that function consumes. Running
  them side by side puts both strands in the same 20 lines during a move — the
  one kind of conflict git resolves worst.

### The two named seams

1. `drawPlayedVisualizer` — resolved by decision 5. B's **first** commit moves it
   from `NowPlayingScene.kt:591-612` into `VisualizerScene.kt` with no behaviour
   change, and B does not touch it again. C owns it afterwards.
2. The call site `drawPlayedVisualizer(buffer = visualEngine.scene(...), …)` at
   `NowPlayingScene.kt:234-244` stays in B's file. C is explicitly allowed to
   change **that call and nothing else** in `NowPlayingScene.kt`, because C
   changes the type flowing through it. C names that edit in its report.

## Post-merge cross-checks

None of these can run inside a single strand. They are run by the human on the
phone, after C has landed, with `scripts/android-scene-framerate.sh`:

1. The device protocol end to end on a track with **no** stored spectrogram:
   playing, spectrum off, then on. Expect the display rate in both, with the
   state verified in sync at both ends of each window.
2. The same on a track **with** a stored spectrogram, to prove the accelerator
   still helps and nothing regressed.
3. Automatic track transition while the Now Playing screen is open: the UI
   follows, and the frame rate does not drop afterwards. This reads strand A's
   files and strand B's behaviour at once.
4. Paused screen still throttles: the battery guard survived the change.
5. GC bytes per 12 s window with the spectrum on, against the cover control arm.
   Baseline to beat: 67 MB freed with the spectrum, 0 MB with the cover.

## Two warnings for whoever starts this

- **The main checkout carries uncommitted work in `crates/reprise-android-ffi/src/visualizer.rs`**
  (+39/−10 against `HEAD` on 2026-08-22) that already attacks strand C's lock
  contention — it puts `tick()` and `scene()` on `try_lock` and adds a
  `cached_scene` fallback. It is not landed and not part of this plan. Strand C
  branches from `origin/dev`, ignores it, and the human decides afterwards which
  of the two survives. Do not merge them blind.
- Local `dev` in the main checkout is 19 commits behind `origin/dev`. Every
  strand branches from `origin/dev`, never from the checkout's `dev`.
