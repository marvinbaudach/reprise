# Handoff — the Now Playing scene crawls because the UI thinks playback is paused

Device measurements taken 2026-08-22 on a Pixel 10 Pro XL (GrapheneOS,
120 Hz panel), `org.reprise` 0.1.25 release build, `versionCode=25`,
`flags=0x0` (not debuggable). Every window below was 10–12 s long and had its
playback state read from `dumpsys media_session` **at the start and at the end**
of the window, so no arm is a paused arm in disguise.

## The number

| UI state | Visualizer | Frames / 10 s | Janky | p50 | p90 |
|---|---|---|---|---|---|
| in sync | cover | 664, repeat 656 | 0.15 %, 0 % | 8 ms | 10 ms |
| in sync | **spectrum** | **1216 → 121.6 fps** | 0.08 % | 11 ms | 13 ms |
| desynced | cover | 155, repeat 157 | 0 % | 10 ms | 11 ms |
| desynced | spectrum | 190 / 12 s | 48 % | 15 ms | 19 ms |

A factor of **7.7** between the two UI states, and the visualizer mode barely
moves it. With the state in sync the spectrum reaches **121.6 fps** — the bars
are not expensive. Frame *times* are healthy in every arm; what changes is how
many frames the app asks for.

## Why the frames stop coming

`PlaybackUiState.visualizerActive` (`android/app/src/main/java/de/reprise/spike/PlaybackUiState.kt:35-36`)
is derived from the **UI's** copy of the playback state:

```kotlin
internal val PlaybackUiState.visualizerActive: Boolean
    get() = state.hasPlayIntent          // PLAYING || BUFFERING
```

`DriveScene` reads it and, when it is false, puts a fixed delay in front of the
vsync callback (`SceneDriver.kt:16, 247-254`):

```kotlin
if (!visualizerActive || frames.frameCount == 0 && frameSink == null) {
    delay(PAUSED_SCENE_FRAME_INTERVAL_MS)   // = 50L
}
withFrameNanos { if (driver.tick()) drawRevision += 1 }
```

50 ms plus the wait for the next vsync is the measured ~16 fps. The throttle is
correct behaviour for a paused screen; the defect is that the UI reaches that
state while music is playing.

## Evidence that the UI state, not the renderer, is stale

1. **Frozen time label.** The Now Playing label sat at `2:08` across screenshots
   a minute apart while `dumpsys media_session` reported the same session at
   `224138 ms` and then `242136 ms` and climbing. At the start of the session
   the two agreed (`1:15` / `75845 ms`), so the divergence appeared during the
   session rather than being a stale-session artefact.
2. **Stale track.** After an automatic track transition, a screenshot showed
   `Into the Earth · 4:47 · −0:24` with a **Play** button, while the service was
   playing the following track at `33512 → 45743 ms`. The UI was a whole track
   behind.
3. **Dead play button in that state.** Taps landed (the same coordinates paused
   playback in a healthy state, confirmed via `media_session`), but the button
   did not start anything the UI reflected.
4. **A restart repairs it.** `am force-stop org.reprise` + relaunch produced a
   correct screen (`Soulless Existence · 0:06 · pause icon`), and the frame rate
   went from 155 frames / 10 s to 664 frames / 10 s on the same track and the
   same visualizer mode.

## Side findings, all real, none of them the cause

- **No frame rate is requested unless the spectrum is on.**
  `Modifier.preferredFrameRate(FrameRateCategory.High)` is attached only when
  `visualizerOpacity > 0f && playing`
  (`NowPlayingScene.kt:69-81, 144-148`). In cover mode nothing is requested and
  `dumpsys display` shows `frameRateOverride {uid=10262 frameRateHz=60.000004}`
  on a panel whose modes list 120 Hz. Turning the spectrum on is what lifted the
  measured rate to 121.6 fps.
- **`scene()` allocates a boxed float list per drawn frame.**
  `AndroidVisualEngine::scene` (`crates/reprise-android-ffi/src/visualizer.rs:397-408`)
  is called from inside the Compose draw phase
  (`NowPlayingScene.kt:234-241`), rebuilds up to
  `64 * (16 + 6 + 2) + 4 = 1540` shapes (`crates/reprise-core/src/visuals/modes/bars.rs:46`)
  and returns them across UniFFI as `List<Float>` (`VisualizerScene.kt:24, 109`),
  i.e. boxed `java.lang.Float` per scalar. Measured with `adb logcat --pid`:
  **67 MB freed** in one 12 s spectrum window against **0 MB** in the identical
  cover window. Costs p50 8 ms → 11 ms. Worth fixing, not the stutter.
- **`Brush.radialGradient` is constructed in the draw phase**, once per glow
  shape per frame (`VisualizerScene.kt:203-211`); bars emit one whenever
  `value > 0.10` (`bars.rs:93-106`), so up to 64 shader objects per frame.
- **`DriveScene`'s `LaunchedEffect` is keyed on `playback.positionMs`**
  (`SceneDriver.kt:236-245`), which changes every 500 ms
  (`Media3PlaybackPort.kt:24 POSITION_INTERVAL_MS = 500L`). The frame loop is
  cancelled and relaunched twice a second for no reason.
- **No temporal smoothing of bar heights.** `VisualEngine::ingest` overwrites
  `bands_current` (`crates/reprise-core/src/visuals/engine.rs:252`) and
  `refresh_display_bands` copies it straight to `display_bands`
  (`engine.rs:212-215`). One CAVA frame is produced per Media3 audio buffer
  (`visualizer.rs:105-127, 274-332`), so the drawn height is a step function of
  decoder buffer arrivals. Decoder in use during the session:
  `c2.android.opus.decoder`.
- **The audio thread drops band frames on lock contention.**
  `ingest_pcm_i16` takes `try_lock()` and returns `false` when the UI thread
  holds the state mutex (`visualizer.rs:313-318`), and `scene()` holds that same
  mutex across the whole 1540-shape build (`visualizer.rs:397-408`).

## How to reproduce the measurement

```sh
adb shell dumpsys media_session | grep -A8 "package=org.reprise" | grep -m1 "state="
adb shell dumpsys gfxinfo org.reprise reset
sleep 10
adb shell dumpsys gfxinfo org.reprise | grep -E "Total frames|Janky frames:|percentile"
adb shell dumpsys media_session | grep -A8 "package=org.reprise" | grep -m1 "state="
```

Two traps cost several invalid runs and are worth naming:

- With the notification shade pulled down the app renders nothing at all —
  `Total frames rendered: 0` while `media_session` still says `PLAYING`. Always
  take a screenshot inside the window and confirm the Now Playing screen is
  actually on top.
- A screenshot alone does not prove the UI is in sync. Compare the on-screen
  time label with the `media_session` position; that is the check that exposed
  this bug in the first place.

---

# Root cause, found after the measurements

The state channel from the service to Compose is a **single nullable callback
slot at every hop**, push-only, with no reconciliation and no supervision. There
is no `Flow`/`StateFlow` and no polling anywhere in the path.

```
ExoPlayer Player.Listener
  → Media3PlaybackPort.eventBridge          (Media3PlaybackPort.kt:35, 245-247)
  → Rust SessionInner::handle_event         (playback_session.rs:384, 428-451)
  → ReprisePlaybackService.coreListener     (ReprisePlaybackService.kt:46-59)
  → ReprisePlaybackService.observer         (ReprisePlaybackService.kt:31)   ← breaks here
  → MainActivity.playbackState              (MainActivity.kt:162, 172-178)
```

- `observer` is one slot, not a list: `observer?.invoke(snapshot)`
  (`ReprisePlaybackService.kt:50`) drops the snapshot **silently** when it is
  null, while `latestPlaybackSnapshot` in the same method keeps the truth.
- `MainActivity.onStop()` nulls it unconditionally via `detachObserver()`
  (`MainActivity.kt:432-443`). The **only** re-arm is a fresh
  `onServiceConnected` after `onStart`'s `bindService` (`MainActivity.kt:416-422`,
  `167-195`); nothing else in the file ever calls `attachObserver` again.
- When that callback does not arrive, `observer` stays null forever *and*
  `MainActivity.playbackService` stays null. Then `runPlaybackCommand`
  (`MainActivity.kt:684-698`) returns early with "playback is still connecting"
  — which is exactly the dead play button — and `playbackState` is frozen at the
  last snapshot it received, which is exactly the old track with a Play icon,
  which makes `visualizerActive` false, which takes the 50 ms throttle.
- `attachObserver` republishes `coreSession.snapshot()` immediately
  (`ReprisePlaybackService.kt:180-187`), so a *successful* re-attach is
  self-healing. The bug needs a re-bind that attaches nothing, not merely a race.
- `am force-stop` kills the process, so the relaunch rebuilds `onCreate` →
  `attachObserver` — consistent with a broken re-attach rather than corrupted
  state.

**Second, independent freeze path**, and the one that matches the *first*
symptom of the session (time label stuck at 2:08 while the play/pause button
still correctly showed playing): `SeekPositionState.acceptSnapshot`
(`NowPlayingState.kt:80-81`) returns `this` unchanged for every incoming
snapshot while `isDragging` is true. A drag whose pointer-up is lost freezes the
position readout on its own, without touching the rest of the UI state.

Not verified: the platform-level reason a `bindService()` call would not
redeliver `onServiceConnected`. That is Binder/ActivityManager behaviour, not
expressed in this code. What is established is the single point of failure that
fits the symptom exactly.

# Grill decisions (2026-08-22)

The draft `docs/plans/android-now-playing-desync-throttles-the-scene.draft.md`
was grilled with the user. Settled:

1. **Live analysis engine runs while the Now Playing screen is visible** — not
   always, and not only when the spectrum bars are shown. Today it is gated on
   `visualizerOpacity > 0f` (`NowPlayingScene.kt:121-127`).
2. **Strand A rebuilds the channel as a `StateFlow` in the service** plus an
   explicit reconcile when the screen becomes visible — not a patched-up single
   slot.
3. **Cover mode keeps its 60 Hz.** No `FrameRateCategory` request is added for
   the fog: it measured 664 frames / 10 s at 0 % jank, and `High` would buy
   little for a slowly drifting image. `High` stays exclusive to the bars. The
   corresponding item in the draft's strand B is dropped.
4. **All three strands stay in this plan**, including the per-frame cost work.
5. **The B/C seam is resolved by a move:** strand B's first commit relocates
   `drawPlayedVisualizer` (`NowPlayingScene.kt:591-611`, 20 lines, depends on
   `COVER_SIZE_DP`, `COVER_RADIUS_DP`, `playedCoverRect`, `drawCoverShadow`,
   `AmbientTrueBlack`) into `VisualizerScene.kt` with no behaviour change. After
   that strand C owns the buffer type end to end. **Merge order A → B → C.**
6. **`scripts/android-scene-framerate.sh` is written and owned by strand B.** It
   must check the app is actually in the foreground (a pulled-down notification
   shade produced `Total frames rendered: 0` here), read the playback state at
   both ends of the window, compare the on-screen time label against the
   `media_session` position, and run a control arm.
7. Assistant's own cut, not a user decision: the `isDragging` freeze goes into
   strand A, same file group and same symptom class.

**Consequence made explicit and accepted:** once live analysis feeds the scene,
`SceneDriver` takes its live branch and `fallbackBands` no longer runs, so the
stored 24-band/20 Hz spectrogram stops driving the scene entirely. It keeps only
its role in the seek bar's colours.

# State of the work / what is left

The plan is written (2026-08-22). The draft is deleted; the grill decisions above
are carried into it verbatim.

- `docs/plans/android-now-playing-desync-throttles-the-scene.md` — mother plan,
  `strands: a,b,c`, `merge_order: a,b,c`, claims no branch.
- `…-a.md` — the UI follows the service: the `StateFlow` channel, the loud and
  recoverable bind, the position ticker, the cancelled-drag freeze.
- `…-b.md` — the scene clock: the move commit, the live engine while the screen
  is visible, the audio-driven throttle, the `positionMs` key, and
  `scripts/android-scene-framerate.sh`.
- `…-c.md` — the per-frame cost: the dropped-frame counter first, then the byte
  buffer, the draw-loop allocations, and the critical section.

Every line reference in the plan was re-read against `origin/dev` @ `1515487599`;
the references in *this* handoff were taken from a checkout 19 commits behind it
and with local modifications, so where the two disagree the plan is right.

Two things found while writing the plan that were not in the measurements:

- `crates/reprise-android-ffi/src/visualizer.rs` carries **uncommitted** work in
  the main checkout (+39/−10) that already puts `tick()`/`scene()` on `try_lock`
  and adds a `cached_scene` fallback — i.e. someone is already inside strand C's
  file. It is not landed and not part of the plan.
- Strand B's decision 1 has a trap: `visualEngine.scene(...)` is an *argument* at
  `NowPlayingScene.kt:236-239`, so it is evaluated before `drawPlayedVisualizer`
  reaches its `if (opacity <= 0f) return`. Enabling the engine in cover mode
  without guarding the draw call would start building 1540 shapes per frame in
  the arm that measured 0 MB of GC garbage. The plan carries this as B-2's
  regression guard.

**Next step:** `/code docs/plans/android-now-playing-desync-throttles-the-scene-a.md`,
then B, then C — one strand at a time, in that order.

The user's phone was left playing "Soulless Existence" with the visualizer back
on cover mode, which is where it started.
