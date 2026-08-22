---
slug: android-now-playing-desync-throttles-the-scene-a
worktree: /home/marvin/Projects/reprise-android-now-playing-desync-throttles-the-scene-a
branch: feature/android-now-playing-desync-throttles-the-scene-a
phase: shipped
codex_session:
created: 2026-08-22
---

# Strand A — the UI follows the service

Mother plan: `docs/plans/android-now-playing-desync-throttles-the-scene.md`.
Measurements and symptom record:
`docs/plans/android-now-playing-desync-throttles-the-scene.HANDOFF.md`.

Read against `origin/dev` @ `1515487599`. Every line number below comes from that
state.

## Purpose

The Now Playing screen's copy of the playback state goes stale while music
plays: the time label froze at `2:08` while the session ran on past `4:02`; after
an automatic track change the screen showed the previous track with a Play
button while the service played the next one; the play button did nothing; and
`am force-stop` + relaunch repaired all of it. That stale copy is what makes
`PlaybackUiState.visualizerActive` (`PlaybackUiState.kt:35-36`) false, which is
what takes the 50 ms throttle in `SceneDriver.kt:247-250` and collapses the
scene from 664 to 155 frames per 10 s.

**This strand does not touch the scene.** It makes the UI's state true. Strand B
measures the frame rate afterwards; measuring it before this strand lands would
be measuring the lie.

**This strand must land before strand B is started.**

## What is broken, precisely

The channel from the service to Compose is a single nullable callback slot at
every hop, push-only, with no reconciliation and no supervision:

```
ExoPlayer Player.Listener
  → Media3PlaybackPort.eventBridge          (Media3PlaybackPort.kt:35, 246)
  → Rust SessionInner::handle_event
  → ReprisePlaybackService.coreListener     (ReprisePlaybackService.kt:46-64)
  → ReprisePlaybackService.observer         (ReprisePlaybackService.kt:31)   ← breaks here
  → MainActivity.playbackState              (MainActivity.kt:170, 180-186)
```

1. `observer` is one slot, not a list. `observer?.invoke(snapshot)`
   (`ReprisePlaybackService.kt:50`) drops the snapshot **silently** when it is
   null, while `latestPlaybackSnapshot` two lines above keeps the truth. Two
   fields, one of them a lie.
2. `MainActivity.onStop()` nulls all three observers unconditionally
   (`MainActivity.kt:440-451`). The only re-arm is a fresh `onServiceConnected`
   after `onStart`'s `bindService` (`MainActivity.kt:424-430`, `176-195`).
   Nothing else in the file ever re-attaches.
3. When that callback does not arrive, `observer` stays null forever **and**
   `MainActivity.playbackService` stays null, so `runPlaybackCommand`
   (`MainActivity.kt:695-709`) returns early with "playback is still connecting"
   — the dead play button — and `playbackState` is frozen at its last snapshot.
4. Independently, `SeekPositionState.acceptSnapshot` (`NowPlayingState.kt:80-81`)
   returns `this` for every snapshot while `isDragging` is true, and the only
   path that clears `isDragging` is `Slider`'s `onValueChangeFinished`
   (`NowPlayingSheet.kt:346-348`). Compose's `Slider` does **not** call that on a
   cancelled gesture, so a cancelled drag freezes the position readout for the
   rest of the track while the rest of the UI stays correct — which is exactly
   the first symptom of the measured session.
5. Independently again, `Media3PlaybackPort.positionTicker`
   (`Media3PlaybackPort.kt:42-55`) returns **without rescheduling itself**
   whenever `player.isPlaying` reads false at the moment it runs, and the only
   thing that restarts it is `onIsPlayingChanged(true)`
   (`Media3PlaybackPort.kt:57-64`). A momentary false with no later true stops
   position events while the published state stays `PLAYING`.

**Not verified, and deliberately not guessed at:** the platform-level reason a
`bindService()` would not redeliver `onServiceConnected`. That is
Binder/ActivityManager behaviour, not expressed in this code. Task A-3 therefore
does not try to explain it — it makes that failure loud and recoverable instead
of silent and permanent.

## File ownership

These files are yours completely, together with their tests under
`android/app/src/test/java/de/reprise/spike/`:

```
android/app/src/main/java/de/reprise/spike/MainActivity.kt
android/app/src/main/java/de/reprise/spike/ReprisePlaybackService.kt
android/app/src/main/java/de/reprise/spike/Media3PlaybackPort.kt
android/app/src/main/java/de/reprise/spike/NowPlayingState.kt
android/app/src/main/java/de/reprise/spike/PlaybackUiState.kt
android/app/src/main/java/de/reprise/spike/MobileSurfaceViewModel.kt
android/app/src/main/java/de/reprise/spike/NowPlayingSheet.kt
```

Inside these you may make any change the task needs, including files this plan
does not name by name, as long as they belong to this group. New test files are
welcome; `MainActivity.kt` is at 739 of its 800 allowed lines, so anything that
grows goes into a new file, not into it.

## What is **not** yours

- `SceneDriver.kt`, `NowPlayingScene.kt` — strand B. You do not change how the
  scene reads the state; you change what the state says.
- `crates/reprise-android-ffi/**`, `VisualizerScene.kt` — strand C.
- `android/app/build.gradle.kts` — see the note under Task A-1. If you truly
  cannot write a test without a new dependency, that is a **finding for the
  report**, not an edit.
- Everything else in the repo.

## Test discipline

First the test, then the run that sees it fail, then the implementation. A test
that is green on its first run has measured nothing and is to be discarded. The
red run belongs in the report, quoted.

Every task ends green and committed:

```sh
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  android/gradlew --project-dir android :app:testDebugUnitTest \
  --tests 'de.reprise.spike.<TheTest>' > $LOG/a-narrow.log 2>&1
ls -l android/app/build/test-results/testDebugUnitTest/   # fresh, or nothing ran
```

`BUILD SUCCESSFUL` is not proof — Gradle reports the task up-to-date, exits 0 and
runs nothing. The verdict lives in the XML files above.

---

## Task A-1 — the service publishes state instead of pushing it at one slot

**Goal:** `ReprisePlaybackService` has exactly one truth per channel, and it is
readable at any time by anyone who is listening. No snapshot can be dropped
because nobody was attached.

**Files:**
- Modify: `android/app/src/main/java/de/reprise/spike/ReprisePlaybackService.kt`
  (`:31-34`, `:46-64`, `:151`, `:180-206`, and the sleep-timer readers at
  `:208`, `:222`, `:228`)
- Test: new `android/app/src/test/java/de/reprise/spike/PlaybackSnapshotChannelTest.kt`

**Interfaces produced:**

```kotlin
internal val playbackSnapshots: StateFlow<AndroidPlaybackSnapshot?>
internal val settingsRevisions: StateFlow<Long>
internal val sleepTimerStates: StateFlow<SleepTimerUiState>
```

All three `attach*Observer`/`detach*Observer` pairs disappear.

**Why all three and not only playback:** they are the same three lines with the
same defect, `onStop` detaches all three together (`MainActivity.kt:441-443`),
and the collector plumbing in A-2 is written once. Converting one and leaving two
identical landmines in place would be worse than converting none.

### Step 1: write the failing test

In the new `PlaybackSnapshotChannelTest.kt`, using the Robolectric service
controller style of `PlaybackServiceLifetimeTest.kt`:

- `aSnapshotPublishedWithNobodyListeningIsStillReadableAfterwards` — build the
  service, feed `coreListener.onPlaybackChanged(snapshot)` with **no collector
  attached at all**, then assert `service.playbackSnapshots.value` is that
  snapshot. Against today's code this cannot even be written without the new
  property; write it against the intended API and let it fail to compile — that
  is a valid red, and it must be recorded as such.
- `aLateCollectorReceivesTheStateThatWasPublishedBeforeItArrived` — publish, then
  start collecting, and assert the first value seen is the published one. This is
  the reconcile that `attachObserver`'s `coreSession.snapshot()` replay
  (`:180-183`) does today by accident and that a `StateFlow` does by contract.

### Step 2: implement

- Replace `observer` (`:31`) and `latestPlaybackSnapshot` (`:34`) with a single
  `private val mutablePlaybackSnapshots = MutableStateFlow<AndroidPlaybackSnapshot?>(null)`
  and expose it as `StateFlow` via `asStateFlow()`. **One field, not two** — the
  bug was that two fields disagreed.
- `coreListener.onPlaybackChanged` (`:47-59`) writes `mutablePlaybackSnapshots.value = snapshot`
  in place of both the field assignment and `observer?.invoke(snapshot)`. Order
  and the `stopSelf()` branch are unchanged.
- The three sleep-timer readers that used `latestPlaybackSnapshot` (`:208`,
  `:222`, `:228`) read `playbackSnapshots.value`.
- Seed the flow where the session becomes available, so a collector that arrives
  before the first event still sees the truth: wherever `coreSession` is
  assigned, follow it with
  `mutablePlaybackSnapshots.value = coreSession?.snapshot()`. This replaces the
  replay that `attachObserver` performed at `:182`.
- `settingsObserver` (`:32`) becomes `MutableStateFlow<Long>(0)` incremented
  where the observer was invoked; `sleepTimerObserver` (`:33`) becomes
  `MutableStateFlow(sleepTimer.state())` written where the observer was invoked.
- `attachObserver`, `detachObserver`, `attachSettingsObserver`,
  `detachSettingsObserver`, `attachSleepTimerObserver`,
  `detachSleepTimerObserver` (`:180-206`) are **deleted**, together with the
  `observer = null` at `:151`. Anything that still calls them is A-2's job.

**Dependency note:** `MutableStateFlow` comes from `kotlinx-coroutines-core`,
already on the compile classpath — `SceneDriver.kt:14` imports
`kotlinx.coroutines.delay` today, with no declared dependency. Do **not** add
`kotlinx-coroutines-test`: the tests in this strand drive the real main looper
through Robolectric (`shadowOf(Looper.getMainLooper()).idle()` /
`.idleFor(...)`), the way `PlaybackServiceLifetimeTest.kt` already does, which is
both closer to the real wiring and free of new dependencies. If you convince
yourself a test genuinely cannot be written that way, stop and report it.

---

## Task A-2 — the screen collects the state, lifecycle-scoped

**Goal:** `MainActivity` has one source for the bound service and one collector
for its state, restarted by the lifecycle, so that becoming visible always
reconciles and never silently attaches nothing.

**Files:**
- Modify: `android/app/src/main/java/de/reprise/spike/MainActivity.kt`
  (`:168-203`, `:424-430`, `:440-451`, `:695-709`)
- Test: new `android/app/src/test/java/de/reprise/spike/MainActivityPlaybackChannelTest.kt`

### Step 1: write the failing test

Drive the real activity with `Robolectric.buildActivity(MainActivity::class.java)`,
the real `bindService` and the real service, as `PlaybackServiceLifetimeTest.kt`
does:

- `anAutomaticTrackTransitionReachesTheScreen` — activity created and started,
  service publishes track 1 playing, then publishes track 2 playing without any
  command from the UI. Assert the activity's playback state carries track 2's id,
  an advancing position and `visualizerActive == true`. This is symptom 2 from
  the handoff, at the seam the app uses.
- `stopAndStartRepublishesTheStateThatArrivedWhileTheScreenWasAway` — start,
  stop, publish a new snapshot **while stopped**, start again. Assert the
  activity shows the snapshot published while it was away. Today this passes only
  by way of `attachObserver`'s replay; after A-1 it is the `StateFlow` contract,
  and it is the test that pins the reconcile.
- `aCommandIssuedRightAfterRestartReachesTheService` — after the stop/start
  cycle, call the play/pause path and assert it did **not** produce
  "playback is still connecting". This is symptom 3.

### Step 2: implement

- Replace `private var playbackService: ReprisePlaybackService? = null` (`:168`)
  with `private val boundService = MutableStateFlow<ReprisePlaybackService?>(null)`.
  Everything that read `playbackService` — `connectedService` (`:155`), the
  equalizer path (`:646`), the settings reloads (`:675`, `:685`, `:691`) and
  `runPlaybackCommand` (`:702`) — reads `boundService.value`. One source, so the
  state and the command target can never disagree again.
- `onServiceConnected` (`:176-195`) shrinks to: publish the service into
  `boundService`, set `visualSceneEngineFactory.value`. **No attach calls.**
- `onServiceDisconnected` (`:197-202`) clears `boundService`, resets the factory,
  and leaves `playbackState` alone — see the note below.
- One collector, started once in `onCreate`:

  ```kotlin
  lifecycleScope.launch {
      repeatOnLifecycle(Lifecycle.State.STARTED) {
          boundService.flatMapLatest { service ->
              service?.playbackSnapshots ?: flowOf(null)
          }.collect { snapshot ->
              if (snapshot != null) {
                  playbackState.value = snapshot.toUiState()
                      .copy(sleepTimer = playbackState.value.sleepTimer)
              }
          }
      }
  }
  ```

  plus the two sibling collectors for `settingsRevisions` and `sleepTimerStates`,
  in the same `repeatOnLifecycle` block. `runOnUiThread` disappears: the collector
  already runs on the main dispatcher.

- `onStop` (`:440-451`) keeps only `boundService.value = null`, the factory reset
  and the `unbindService` — no `detach*` calls, because they no longer exist.
  `repeatOnLifecycle` cancels the collectors on its own and restarts them on the
  next `onStart`, which is where the reconcile happens.

**On `onServiceDisconnected` no longer blanking `playbackState`:** today it sets
`PlaybackUiState()` (`:200`), which is what makes a lost connection look like
"nothing is playing" rather than "we lost the service". Keep the last known state
and let the reconnect replace it; a blank state here is indistinguishable from
the bug being fixed. Pin this in a test:
`losingTheServiceDoesNotClaimNothingIsPlaying`.

---

## Task A-3 — a bind that attaches nothing becomes loud and recoverable

**Goal:** the failure mode nobody could see becomes greppable, and it repairs
itself instead of needing `am force-stop`.

**Files:**
- Modify: `android/app/src/main/java/de/reprise/spike/MainActivity.kt` (`:424-430`)
- Test: extend `MainActivityPlaybackChannelTest.kt`

### Step 1: write the failing test

- `aBindThatNeverConnectsIsRetriedAndLogged` — start the activity in a
  configuration where `onServiceConnected` does not arrive (Robolectric's
  `ShadowApplication.setComponentNameAndServiceForBindService`/
  `declareActionUnbindable` control this; pick the seam that lets you withhold
  the callback while `bindService` still returns true). Advance the main looper
  past the watchdog window with `shadowOf(Looper.getMainLooper()).idleFor(...)`.
  Assert a second `bindService` was issued.
- `aRefusedBindIsRecorded` — make `bindService` return false and assert the app
  logs it. Today `playbackBound = bindService(...)` (`:429`) records the refusal
  in a field nothing ever reads.

### Step 2: implement

In `onStart`, after `bindService`:

- If it returned `false`: `Log.w(TAG, …)` with a distinct, greppable phrase.
- Otherwise launch a lifecycle-scoped watchdog that waits a bounded window
  (name the constant — `PLAYBACK_BIND_WATCHDOG_MS`, 2 s is generous for a local
  bind) and, if `boundService.value` is still null, logs `Log.w` with the same
  distinct phrase and issues `bindService` **once** more. One retry, not a loop:
  a permanent platform refusal must not turn into a spin.

The log phrase is the deliverable as much as the retry is — this class of bug was
invisible for the whole session that found it, and the next one has to be
greppable in `adb logcat`.

---

## Task A-4 — the position ticker reschedules from play intent

**Goal:** position events do not stop while the published state says the music is
playing.

**Files:**
- Modify: `android/app/src/main/java/de/reprise/spike/Media3PlaybackPort.kt`
  (`:42-55`, `:57-64`)
- Test: `android/app/src/test/java/de/reprise/spike/Media3PlaybackPortTest.kt`

### Step 1: write the failing test

`aMomentaryNotPlayingDoesNotStopPositionEventsForGood` — with the existing fake
player of `Media3PlaybackPortTest.kt`, start playback so the ticker runs, let one
tick observe `isPlaying == false` **without** a following
`onIsPlayingChanged(true)`, advance the looper past several
`POSITION_INTERVAL_MS` and assert position events resume.

### Step 2: implement

`positionTicker` (`:42-55`) stops emitting while the player is not playing, but
keeps rescheduling itself as long as the port believes playback is intended — the
same play intent the port already tracks in `lastState` (`:39`). Cancellation
stays where it belongs: `release()` (`:194`) already removes the callbacks, and
`onIsPlayingChanged(false)` may stop it, but only via a path that a later `true`
is not the *only* way out of.

Keep the existing guarantee that nothing is emitted while genuinely paused —
there must still be a test that a paused player produces no position events.

---

## Task A-5 — a cancelled drag gives the seek head back

**Goal:** the position readout cannot stay frozen because a gesture ended in a
way `onValueChangeFinished` does not report.

**Files:**
- Modify: `android/app/src/main/java/de/reprise/spike/NowPlayingSheet.kt`
  (`:335-352`)
- Read, do not change without cause: `NowPlayingState.kt:80-88`,
  `MobileSurfaceViewModel.kt:292-312`
- Test: `android/app/src/test/java/de/reprise/spike/NowPlayingGesturesTest.kt`

### Step 1: write the failing test

- `aCancelledSeekGestureReturnsTheHeadToThePlaybackPosition` — compose the sheet,
  begin a drag, emit a cancel through the slider's `MutableInteractionSource`,
  then publish an advancing snapshot and assert the displayed position follows
  it.
- Guard, and it must stay green: `aStillFingerKeepsTheHead` — begin a drag,
  publish snapshots without any further drag update or cancel, assert the head
  stays where the finger left it. This is the reason a timeout backstop was
  **rejected**: a finger held still on the slider emits nothing, and a
  time-based release would steal the head from a live gesture.

### Step 2: implement

Give the `Slider` (`:342-352`) an explicit `interactionSource` and collect it:

```kotlin
val interactionSource = remember { MutableInteractionSource() }
LaunchedEffect(interactionSource, trackId) {
    interactionSource.interactions.collect { interaction ->
        if (interaction is DragInteraction.Cancel || interaction is PressInteraction.Cancel) {
            surfaceState.releaseScrub(trackId)
        }
    }
}
```

`releaseScrub` (`MobileSurfaceViewModel.kt:307-312`) only clears ownership; the
seek happens at the call site in `onValueChangeFinished` (`:347`). So the cancel
path releases the head **without** seeking — which is what "cancelled" means.

`SeekPositionState` itself is not changed. Its contract is right; what was
missing was a caller for `release()` on the path that had none.

---

## Task A-6 — the guard: the state survives a recreation while playing

**Goal:** the class of bug this strand fixes stays fixed.

**Files:**
- Test only: `android/app/src/test/java/de/reprise/spike/MainActivityPlaybackChannelTest.kt`

`theUiStateSurvivesAnActivityRecreationWhilePlaying` — with the service playing,
recreate the activity the way `MainActivityPlayViewStabilityTest.kt:167` does
(`recreateAt(qualifiers)`), and assert the new activity shows the *current*
track, an advancing position and `visualizerActive == true` — not a blank state
and not the pre-recreation snapshot.

---

## Acceptance for this strand

```sh
# the two suites this strand owns most of
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  android/gradlew --project-dir android :app:testDebugUnitTest \
  --tests 'de.reprise.spike.MainActivity*' \
  --tests 'de.reprise.spike.PlaybackS*' \
  --tests 'de.reprise.spike.Media3PlaybackPortTest' \
  --tests 'de.reprise.spike.NowPlayingGesturesTest' > $LOG/a-suite.log 2>&1

# the full gate — the verdict, not the exit code
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/check-android-suite.sh > $LOG/a-android.log 2>&1
grep -E '^suites=' $LOG/a-android.log        # failures=0 errors=0 verdict=fresh

# the scene suite must not have moved
JAVA_HOME=/usr/lib/jvm/java-21-openjdk \
  scripts/verify-now-playing-scene.sh > $LOG/a-scene.log 2>&1

cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

**No frame-rate measurement in this strand.** The device protocol belongs to
strand B, which owns the script. What this strand must show in its report is the
three symptom tests going from red to green, quoted with their failing run.

## For the report

- The red run of every new test, quoted.
- The distinct log phrase chosen in A-3, so the next person can grep for it.
- Any file you had to touch outside the ownership list, and why.
- Whether `onServiceDisconnected` no longer blanking the state broke anything you
  did not expect.
