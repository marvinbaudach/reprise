---
slug: holding-volume-skips-the-track
worktree: /home/marvin/Projects/reprise-holding-volume-skips-the-track
branch: feature/holding-volume-skips-the-track
phase: superseded
codex_session:
created: 2026-09-02
---
# Holding a volume key skips the track

> **Superseded on 2026-09-02 — the in-app scope was the wrong half.**
>
> This plan shipped as #806 and was reverted again the same day (see the revert
> PR). On the device the owner asked for the opposite gating: the gesture should
> work **with the screen off or while another app is in front**, and stay out of
> the way while a Reprise activity is on screen — because there you can simply
> tap "next".
>
> That inverts the cost. The foreground case implemented here is the only one
> Android gives an app for free; the case actually wanted needs one of the two
> routes this plan's design rejected. The chosen successor is **Media3 remote
> volume**, whose known price is that `VolumeProvider` delivers a bare direction
> delta with no key-up, so long press must be synthesised, and the system panel
> shows a remote slider while active.
>
> What survives and is worth reusing: the decision table, `VolumeKeyTrackSwitch`
> as a pure decision object, and the finding that the two "consume without side
> effect" branches need explicit tests — a mutation showed the suite stayed green
> without them.

Design: `docs/superpowers/specs/2026-09-01-android-volume-keys-track-switch-design.md`

## Goal

In the Reprise Android app, holding `VOLUME_UP` skips to the next track and
holding `VOLUME_DOWN` returns to the previous one. A short press still changes
the volume. In-app only — no `MediaSession` remote-volume trick, no
`AccessibilityService`.

## What exists today

- `MainActivity : ComponentActivity` (`MainActivity.kt:62`). Overrides only
  lifecycle methods (`onStart` 445, `onResume` 471, `onPause` 483, `onStop` 487,
  `onDestroy` 499) — no key handling anywhere in the module.
- Transport commands go through the `PlaybackControls` interface
  (`PlaybackControls.kt:19`); production implementation
  `ActivityPlaybackControls` (`ActivityPlaybackControls.kt:37`) with `next()`
  (48) and `previous()` (50).
- **The controls the UI uses are injected.** `onCreate` reads
  `application as? MainActivitySurfaceProvider` and takes
  `surface.playbackControls` (`MainActivity.kt:208-212`,
  `MainActivitySurface.kt:18`, handed to the composition at
  `MainActivity.kt:257`). The private `playbackControls` field
  (`MainActivity.kt:160`) is what `productionSurface()` puts in there
  (`MainActivity.kt:380`) — in production the same object, in tests a different
  one. `surface` is a **local variable inside `onCreate`**; nothing outside it
  can reach the injected controls today.
- Playback state: `playbackState = mutableStateOf(PlaybackUiState())`
  (`MainActivity.kt:178`), exposed as `internal val currentPlaybackState`
  (179-180). `PlaybackUiState.isPlaying` is `state == PLAYING`
  (`PlaybackUiState.kt:37-38`).
- The app never calls `setVolumeControlStream`; volume keys reach the media
  stream today only because an active `MediaSession` exists.
- Tests: Robolectric 4.16.1, JVM only, **no `androidTest` at all**. Pattern is
  `createAndroidComposeRule<MainActivity>()` with
  `application = ConfigurationTestApplication::class`
  (`MainActivityDockTest.kt:29-38`); that application supplies
  `ConfigurationTestPlaybackControls` (`MainActivityConfigurationTest.kt:435`),
  whose `next()`/`previous()` are `= Unit` and record nothing
  (`ConfigurationTestPlaybackControls.kt:38-39`).
- Gate: `scripts/check-android-suite.sh` → `:app:testDebugUnitTest` and
  `:app:assembleDebug`.

## Decisions settled in the grill

1. **The gate is `isPlaying`** — not "a track is loaded", not "always in the
   foreground". Hold-to-ramp therefore survives everywhere except during
   playback, which is where people reach for the volume least.
2. **One skip per hold.** Android's own `onKeyLongPress` fires once per press;
   no timer of our own, no risk of skipping eight tracks by accident.
3. **The short press is re-applied with `adjustStreamVolume(STREAM_MUSIC, …,
   FLAG_SHOW_UI)`** — deterministic, and correct by construction because we only
   intercept while music plays. No `setVolumeControlStream`; that would change
   behaviour outside this feature.
4. **The Robolectric test goes through `activity.dispatchKeyEvent` only.** No
   fallback to calling the overrides directly.
5. **The new class lives in the root package**, next to its relatives. A
   `playback/` package holding one file while `PlaybackControls.kt` and friends
   sit in the root would be a half-migration.
6. **The switch remembers which key's down it consumed.** The gate is evaluated
   **once per press, at the down event**. Otherwise playback starting between
   down and up makes `onKeyUp` swallow an event the system already acted on and
   apply a second volume step.
7. **One strand.**

## Tasks

### 1. Hoist the resolved surface controls into a field

Add `private var surfaceControls: PlaybackControls? = null` to `MainActivity`
and set it from the resolved `surface.playbackControls` right after the surface
is resolved (`MainActivity.kt:208-212`). Everything else in `onCreate` stays as
it is — `surface` keeps being passed into `setContent` unchanged.

The key handler reads that field. Reading the private `playbackControls` field
instead would bypass the injected fake in tests, and the Robolectric test in
task 5 would pass while proving nothing.

### 2. `VolumeKeyTrackSwitch` — the decision, free of Android

New file `android/app/src/main/java/de/reprise/spike/VolumeKeyTrackSwitch.kt`.
Pure Kotlin: no `android.view.KeyEvent`, no `android.media.AudioManager`, so its
tests need no Robolectric runner.

```kotlin
internal enum class VolumeKey { UP, DOWN }

internal sealed interface VolumeKeyAction {
    /** Consume the event and ask the framework to time a long press. */
    data object StartTracking : VolumeKeyAction
    data object SkipNext : VolumeKeyAction
    data object SkipPrevious : VolumeKeyAction
    /** Consume it and apply the one volume step the framework no longer applies. */
    data class AdjustVolume(val key: VolumeKey) : VolumeKeyAction
    /** Consume it and do nothing. */
    data object Ignore : VolumeKeyAction
    /** Do not consume it — the system handles the key. */
    data object Passthrough : VolumeKeyAction
}

internal class VolumeKeyTrackSwitch(private val isPlaying: () -> Boolean) {
    private var consumed: VolumeKey? = null

    fun onDown(key: VolumeKey, isFirstPress: Boolean): VolumeKeyAction
    fun onLongPress(key: VolumeKey): VolumeKeyAction
    fun onUp(key: VolumeKey, wasTracking: Boolean, wasCanceled: Boolean): VolumeKeyAction

    /** Drop a press the activity will never see the end of. */
    fun forget()
}
```

Rules:

| Call | Condition | Result |
| --- | --- | --- |
| `onDown` | first press, `isPlaying()` | remember `key`, `StartTracking` |
| `onDown` | first press, not playing | forget, `Passthrough` |
| `onDown` | repeat, `consumed == key` | `Ignore` — swallowing the repeat stream is what removes hold-to-ramp |
| `onDown` | repeat, `consumed != key` | `Passthrough` |
| `onLongPress` | `consumed == key` | `SkipNext` (UP) / `SkipPrevious` (DOWN) |
| `onLongPress` | `consumed != key` | `Passthrough` |
| `onUp` | `consumed == key`, tracking and not cancelled | forget, `AdjustVolume(key)` |
| `onUp` | `consumed == key`, cancelled or not tracking | forget, `Ignore` — the long press already fired |
| `onUp` | `consumed != key` | `Passthrough` — the system already handled the down, so we add nothing |

`isPlaying` is `{ currentPlaybackState.isPlaying }`. `wasTracking`/`wasCanceled`
come from the `KeyEvent`, which is the documented idiom
(`event.isTracking() && !event.isCanceled()`).

Both keys held at once is not a case worth machinery: the second down simply
replaces `consumed`, and the first key's up then falls through to
`Passthrough`. Write that down in a comment rather than defending against it.

### 3. Wire it into `MainActivity`

Three overrides next to the lifecycle ones. They translate `KeyEvent` →
`VolumeKey` plus booleans, call the switch, and execute the action:

- `onKeyDown` — `StartTracking` → `event.startTracking()`, return `true`.
- `onKeyLongPress` — `SkipNext` → `surfaceControls?.next()`, `SkipPrevious` →
  `surfaceControls?.previous()`, return `true`.
- `onKeyUp` — `AdjustVolume(key)` → `adjustStreamVolume(STREAM_MUSIC,
  ADJUST_RAISE`/`ADJUST_LOWER, FLAG_SHOW_UI)`, return `true`.
- `Ignore` → return `true`. `Passthrough` → return `super.onKeyX(...)`.

Any key code other than `KEYCODE_VOLUME_UP`/`KEYCODE_VOLUME_DOWN` goes straight
to `super` without touching the switch.

The existing `onPause` (`MainActivity.kt:483`) calls `switch.forget()` — a key
held while the app goes to the background never delivers its up event here.

`AudioManager` comes from `getSystemService(AudioManager::class.java)`, resolved
lazily; the module uses it nowhere else today.

### 4. Unit tests for the switch

`android/app/src/test/java/de/reprise/spike/VolumeKeyTrackSwitchTest.kt`, plain
JUnit, no Robolectric:

- playing, first press → `StartTracking`; repeat of the same key → `Ignore`
- long press UP → `SkipNext`; long press DOWN → `SkipPrevious`
- up event, tracking and not cancelled → `AdjustVolume` with the pressed key
- up event after a long press (cancelled) → `Ignore`
- not playing → `Passthrough` from all three entry points
- **the race from decision 6:** down while not playing → `Passthrough`; then
  make `isPlaying()` return true and send the up event → still `Passthrough`,
  no second volume step
- `forget()` between down and up → the up event is `Passthrough`

### 5. Make the test fake record, and add the Robolectric test

`ConfigurationTestPlaybackControls` (`ConfigurationTestPlaybackControls.kt:38-39`)
gets `val transportCommands = mutableListOf<String>()` and records
`next()`/`previous()` instead of `= Unit`. Additive; no existing test reads
those methods.

New Robolectric test beside `MainActivityDockTest.kt`, same `@Config` and
`createAndroidComposeRule<MainActivity>()`, driving **only**
`compose.activity.dispatchKeyEvent(...)`:

- publish a playing track (`application.service.publish(m9bSnapshot(1))`, then
  `shadowOf(Looper.getMainLooper()).idle()`), send down → long-press down → up
  for `KEYCODE_VOLUME_UP`, assert `application.controls.transportCommands`
  recorded `next()`; the same for `VOLUME_DOWN` → `previous()`
- a short press (down, up, no long press) records nothing
- with nothing playing, the same long-press sequence records nothing

The long press is produced by a second `ACTION_DOWN` with `repeatCount > 0` and
`FLAG_LONG_PRESS`. `KeyEvent.dispatch()` calls `onKeyLongPress` when
`isLongPress() && state.isTracking(event)`, and the `DispatcherState` comes from
the decor view, so all three events must go through the same
`activity.dispatchKeyEvent`. This is real framework code under Robolectric.

If the long press genuinely cannot be produced this way, **stop and report it**
rather than calling the overrides directly — that fallback was considered and
rejected in the grill.

### 6. Gate

`scripts/check-android-suite.sh` (runs `:app:testDebugUnitTest` and
`:app:assembleDebug`). Its `ANDROID_TEST_FLOOR=334`
(`scripts/check-android-suite.sh:9`) is a **floor**, checked as
`executed < ANDROID_TEST_FLOOR` (line 107). New tests only raise the executed
count, so **do not edit the gate script**. If the floor is ever missed, that
means tests stopped running — report it, do not lower the number.

## Verification

The gate proves the decision table and the wiring. It does **not** prove that a
real device delivers volume keys to `Activity.onKeyDown`: the module has no
instrumented tests and Robolectric's events are our own.

**Manual, on a device, after the branch builds** (scrcpy / mobile-mcp):

1. Play a track, hold `VOLUME_UP` → next track. Hold `VOLUME_DOWN` → previous.
2. Short-press either key while playing → one volume step and the system volume
   panel appears.
3. Pause, then hold and short-press both keys → normal volume behaviour,
   **including hold-to-ramp**. This is the decision-1 check.
4. Screen off with playback running → keys change volume as usual; the gesture
   is deliberately out of scope there.

## Risks

- **Hold-to-ramp is gone in-app while playing.** Accepted in the design. Step 3
  above is what keeps it from leaking into the paused and idle cases.
- **Consuming `onKeyDown` suppresses the system's own volume handling**, so a
  bug in the gate makes the volume keys feel broken rather than merely
  unhelpful. Hence the `Passthrough` tests.
- **`onKeyLongPress` only fires if `startTracking()` ran on the down event** and
  that down was not consumed elsewhere. If the Robolectric test sees no long
  press, check that first.
- The change touches a shared test fake (`ConfigurationTestPlaybackControls`)
  that other tests compile against. Purely additive.

## Parallelität

**No cut. One strand.**

The only conceivable split — A: `VolumeKeyTrackSwitch.kt` plus its unit test
(two new files, no existing ones); B: the `MainActivity` wiring, the test fake
and the Robolectric test — has disjoint file sets but is a dependency, not
parallelism: B does not compile without A's API. Two worktrees, two Codex runs
and a merge seam would cost more than the quarter hour of wall clock they could
win on a change this size.

Merge order: n/a. Post-merge cross-checks: none — every verification step reads
files this strand owns. The manual device run belongs to the branch either way
and is listed under Verification.
