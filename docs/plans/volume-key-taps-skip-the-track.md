---
slug: volume-key-taps-skip-the-track
worktree: /home/marvin/Projects/reprise-volume-key-taps-skip-the-track
branch: feature/volume-key-taps-skip-the-track
phase: refactored
codex_session:
created: 2026-09-17
---
# Two taps on the volume keys skip the track

Rocking the volume keys — **up then down** — skips to the next track, **down
then up** to the previous one, **while the screen is off or another app is in
front**, only while Reprise is playing. A single tap is a volume step
everywhere; in an open Reprise activity the keys are stock, hold-to-ramp
included.

Successor of `volume-keys-skip-tracks-with-the-screen-off.md` (#974, reverted
in #979). That plan's gesture was the *hold*, and the device run of 2026-09-16
showed the platform never delivers a hold with the screen off: one
`ACTION_DOWN`, one `ACTION_UP`, no key repeats, whatever the hold length. The
`SET_VOLUME_KEY_LONG_PRESS_LISTENER` route was checked against the AOSP source
afterwards and is dead for the same reason since Android 11 (§ "What the
platform delivers" below). What the platform *does* deliver with the screen off
is one callback per tap, with a usable timestamp — so the gesture becomes taps.

Everything below the decision object is #974's code. `git show 772f8075` is the
template; the tasks say which parts come back verbatim, which change, and which
stay dead.

## What the platform delivers (measured, not assumed)

From `docs/plans/volume-keys-device-run-2026-09-16.txt` and
`media3-remote-volume-findings.md`, Pixel 10 Pro XL, physical keys:

- **Takeover works.** `DeviceInfo(PLAYBACK_TYPE_REMOTE)` plus the five
  device-volume commands and the singular `isCommandAvailable` override put the
  session on `volumeType=REMOTE` while playing and back to `LOCAL` when paused;
  `onDeviceInfoChanged` reaches the session through `addListener`. No Media3
  warning on the re-entrant `publishDeviceVolume`.
- **Screen off: exactly one callback per press.** `ACTION_DOWN repeatCount=0` →
  `Adjusting … by ±1`; `ACTION_UP` → `Adjusting by 0`, dropped by
  `VolumeProviderCompat` before Media3. Sender `pkg=android, uid=1000`. A hold
  is indistinguishable from a tap. Eleven taps in window C arrived cleanly, one
  callback each; DOWN-to-DOWN spacing of the quick ones **309, 437, 490 ms**,
  key held 141–185 ms.
- **Screen on, any app in front: repeats.** First repeat 240–258 ms after DOWN,
  then every 48–52 ms, each one a callback. A hold in another app is a ramp
  at the player.
- **The skip's own work takes ~200 ms on the application thread** (restore →
  `next()` → tick). #974's over-skip came from measuring a repeat gap across
  that work. No decision below may depend on a gap that spans the skip.
- **Fastest human tap-to-tap gap seen: 156 ms** (spike 5, deliberate fast
  triple-tap).
- `adb shell input keyevent 24` produces nothing; every measurement needs the
  physical keys.

### Why `SET_VOLUME_KEY_LONG_PRESS_LISTENER` is not the answer

`android16-release`: the permission is `signature|privileged|development`
(grantable by `pm grant`) and `MediaSessionManager.setOnVolumeKeyLongPressListener`
is `@SystemApi`, so reflection would reach it. But since Android 11
`MediaSessionService.KeyEventHandler` recognises a long press only as *"a
DOWN with repeat count 1 and FLAG_LONG_PRESS"* — the InputDispatcher's repeat,
which exists only when a window receives the key. Android 8–10 had a timer
(`MSG_VOLUME_INITIAL_DOWN`) and that is the era Skip Track / Next Track were
written in; Poweramp's forum records the break on Android 11 Pixels. With a
listener registered, a screen-off hold is dispatched as a tap on UP. Not a
spike worth running.

## Decisions settled in the grill (2026-09-17)

1. **The gesture is the rock: up-then-down = next, down-then-up = previous**,
   second tap within `rockMaxMs` of the first, decided at the second callback,
   no timer. Nothing else produces two *opposite* callbacks that close:
   repeats are same-direction, so a hold in another app can never look like
   it, and quick stepping is same-direction too. Net volume effect is zero by
   construction except at the limits; the restore covers those. The first tap
   names the direction, as the hold did in #974. The same-direction double-tap
   was rejected: the measured quick stepping (309–490 ms DOWN-to-DOWN) sits
   60 ms from a deliberate double-tap (156–250 ms), and a hold in another app
   puts its first repeat at ~250 ms, inside any window that catches one — it
   would have needed a timer against repeats and a calibration before the
   constant could be trusted. It stays the fallback in Risks, not a variant
   in the code.
2. **The session is remote while playing; the foreground flag blocks only the
   skip** (#974 decision 2, unchanged). In an open Reprise activity every
   callback is a step; hold-to-ramp survives there by construction. The lock
   screen with the screen on is not the foreground: the gesture works there.
3. **Remote follows play-when-ready, not `isPlaying`.** #974 flipped
   `DeviceInfo` on `onIsPlayingChanged`, which drops to `LOCAL` during the
   BUFFERING the skip itself causes and flips back a few hundred ms later —
   visible in the log as REMOTE/LOCAL churn on every track change, and a
   window in which a second gesture goes stock. `playWhenReady && playbackState
   in {READY, BUFFERING}` holds REMOTE across the track change; pause, IDLE and
   ENDED still return to `LOCAL` (#974 decision 5 kept in spirit). Device-run
   step 11 checks it.
4. **Apply every step at once; the skip restores** to the volume before the
   first tap (#974 decision 3). The first tap's step happens audibly; the
   second callback is decided as the skip before any step, so the restore
   undoes exactly one step, behind the track change.
5. **One skip per gesture, then a fresh start.** No swallow state: after the
   skip the next callback begins a new sequence. Edge: tap up, then *hold*
   down within the window in another app → skip, then the hold's repeats ramp
   down — the stock behaviour of a hold. Accepted; device-run step 4 executes
   it once and notes how it reads.
6. **`rockMaxMs = 500`, calibrated by the device run.** Loose on purpose so the
   deliberate rock always lands; the second direction is the discriminator,
   not the gap. The one false trigger is the correction — up, too loud,
   down again — and step 2 of the run measures it *uncoached*. Pass: the
   fastest natural correction exceeds `ROCK_MAX_MS` by at least 150 ms.
   Otherwise the constant moves below the fastest correction; if corrections
   reach into the deliberate-rock range of step 1, the plan stops and the
   branch is not landed.
7. **A switch in Settings › Audio, on by default** — "Skip tracks with the
   volume keys", subtitle naming the gesture and the conditions. Off means
   `DeviceInfo` stays `LOCAL` and nothing runs — today's behaviour exactly.
   New setting key `playback.volume_key_skip_gesture_enabled`; the old
   `playback.volume_key_track_switch_enabled` is left dead in DBs that have it
   (a key-value row, no migration) so the one device that ran 0.1.139 does not
   inherit a value with different semantics. Side effect, accepted as in
   #974: while Reprise plays with the switch on, SystemUI draws the session's
   slider instead of the stock one.
8. **A haptic tick on the skip only** (#974 decision 9).
9. **The device run is the landing gate and is binding this time.**
   Robolectric cannot see the platform; #806 and #974 both landed green and
   were reverted. `land.sh` runs after the protocol below has been walked and
   its write-up is **committed in the worktree** (`land.sh` exits 2 on any
   untracked file; the logcat excerpt goes in as `.md` or `.txt`, never
   `.log`, which `.gitignore` swallows) — never before. A green gate and a
   clean review do not shorten it.
10. **One strand** (see Parallelität).

## Task 1 — `RemoteVolumeGesture`, the decision as a pure object

Replaces #974's `RemoteVolumeHold.kt`. Same shape: injected `now`, injected
`isForeground`, one method, a sealed result:

```kotlin
internal enum class VolumeDirection { UP, DOWN }

internal sealed interface RemoteVolumeAction {
    data class Step(val direction: VolumeDirection) : RemoteVolumeAction
    /** Restore STREAM_MUSIC to [restoreVolume], then skip in [direction], then tick. */
    data class Skip(val direction: VolumeDirection, val restoreVolume: Int) : RemoteVolumeAction
}

internal class RemoteVolumeGesture(
    private val rockMaxMs: Long,
    private val isForeground: () -> Boolean,
    private val now: () -> Long,
) {
    fun onAdjust(direction: VolumeDirection, currentVolume: Int): RemoteVolumeAction
}
```

Rules, in order: foreground → `Step`, state cleared. Previous callback exists,
opposite direction, `now - lastAt <= rockMaxMs` → `Skip(direction = first
tap's direction, restoreVolume = volume before the first tap)`, state cleared.
Otherwise `Step`, and this callback becomes the new "first" (direction, time,
`currentVolume`). `currentVolume` is the live index *before* this call's step,
as in #974.

`ROCK_MAX_MS = 500` lives in `CoreControlledPlayer`'s companion with a comment
naming the measured numbers (quick single taps 309–490 ms apart; the second
direction is the discriminator, not the gap). Decision 6 says how the device
run may lower it.

Tests (plain JUnit, fake clock, `CoreControlledPlayerTest`'s sibling
`RemoteVolumeGestureTest.kt`): up/down within window → `Skip(UP, restore =
before first)`; down/up → `Skip(DOWN, …)`; same direction twice → two `Step`s;
opposite after the window → two `Step`s; foreground → `Step` always and clears
state (an up in the foreground followed by a down in the background is not a
rock); after a `Skip` the next callback is a first tap; a repeat train
(50 ms, same direction) → all `Step`s; restore uses the volume before the
*first* tap, not the second.

## Task 2 — wire it into `CoreControlledPlayer`

Recover from `772f8075`, with three changes:

- `RemoteVolumeHold` → `RemoteVolumeGesture`; `REPEAT_GAP_MAX_MS` /
  `LEAD_IN_MAX_MS` → `ROCK_MAX_MS`. `adjustDeviceVolume` loses the `Swallow`
  branch.
- Decision 3: the `init` listener uses `onPlayWhenReadyChanged` and
  `onPlaybackStateChanged`, and `getDeviceInfo()` reports REMOTE when
  `wrappedPlayer.playWhenReady && playbackState in (STATE_READY,
  STATE_BUFFERING) && commands.volumeKeySkipGestureEnabled()`.
- A `Log.d("VolumeKeys", …)` per callback in debug builds only
  (`BuildConfig.DEBUG`): direction, gap to the previous callback, volume,
  resulting action. The device run reads its numbers from this line.

Everything else verbatim: the five commands, the singular
`isCommandAvailable`, `getDeviceVolume`/`isDeviceMuted` from `AudioManager`,
`setDeviceVolume` forwarding, the listener set, `refreshDeviceInfo()`,
`publishDeviceVolume()`, `FLAG_SHOW_UI` on steps and flags 0 on the restore,
`@Suppress("DEPRECATION")` for the flag-less overloads. The class-level
`@OptIn(UnstableApi::class)` from #978 stays. `Commands` gains
`isActivityInForeground()`, `volumeKeySkipGestureEnabled()`, `hapticTick()` as
in #974 (renamed setting accessor).

## Task 3 — service, activity, settings surface

All from `772f8075`, renamed where the setting is named:

- `ReprisePlaybackService`: `controlledPlayer`, `activityInForeground`,
  the cached switch value read on `onCreate` and `reloadPlaybackSettings()`
  (followed by `refreshDeviceInfo()`), `setActivityInForeground`,
  `hapticTick()` (VIBRATE is already in the manifest).
- `MainActivity`: `setActivityInForeground(true/false)` from onResume/onPause
  and on bind (`lifecycle.currentState.isAtLeast(RESUMED)`); the settings
  callback `setVolumeKeySkipGestureEnabled` threaded through
  `MainActivitySurface`, `BrowseScreen`, `LibraryScreen`, `SettingsNavigation`,
  `PlaybackSettingsScreen`, `PlaybackSettingsState` exactly as #974 threaded
  `setVolumeKeyTrackSwitchEnabled`.
- Settings › Audio switch beside Gapless. Title "Skip tracks with the volume
  keys"; subtitle: "Tap up then down for the next track, down then up
  for the previous one. Works with the screen off or another app in front,
  while playing." Strings follow the file's existing pattern.

## Task 4 — the setting in Core and the FFI

Recover `772f8075`'s Rust hunks under the new key:
`crates/reprise-core/src/library/settings.rs` (`VOLUME_KEY_SKIP_GESTURE_ENABLED_KEY`,
default `true`, `get_/set_…_in`), `settings_api.rs`, `settings_tests.rs`;
`crates/reprise-android-ffi/src/playback_settings.rs` field
`volume_key_skip_gesture_enabled` and its test. `unwrap_or(true)` on the read
mirrors the gapless twin (noted in the #974 review, kept). No GNOME surface.

## Task 5 — tests and gate

- `CoreControlledPlayerTest.kt` (Robolectric): recovered from #974 and
  adjusted — REMOTE while `playWhenReady && READY`, REMOTE while BUFFERING,
  LOCAL when paused / IDLE / switch off; `isCommandAvailable(COMMAND_ADJUST_DEVICE_VOLUME)`
  true; a rock restores the volume, calls `next()` / `previousInQueueOrder()`,
  ticks once; a single step calls `adjustStreamVolume` with `FLAG_SHOW_UI`
  and never ticks; foreground → steps only; `setDeviceVolume` forwards.
- `PlaybackServiceLifetimeTest.kt`, `SettingsContentTest.kt`: the two lines
  #974 added, renamed. `LibraryScreenStateTest.mediaSessionTransportReturnsToCore`
  moves back to `CoreControlledPlayerTest` as #974 had it.
- Mutation checks, red-then-green: drop the restore → the volume test fails;
  swap the skip direction → the rock test fails; make `isCommandAvailable`
  consult the wrapped player → the command test fails.
- Gate: `scripts/check-android-suite.sh`, `cargo test --workspace`, clippy
  `-D warnings`, fmt. Note `MainActivityMusicPathsTest` flaked once on a
  byte-identical tree during #979's gate; a single red there is a rerun, not a
  finding.

## Verification — the device run (decision 9, mandatory before landing)

Pixel 10 Pro XL, **physical keys pressed by a human** — the agent takes the
lock, builds, installs, reads `dumpsys` and the log, and writes the numbers
down; every press is the user's, announced step by step. `device-lock acquire
--wait 300 volume-keys "device run <pr>"` is held across build, install,
presses and log pull. Build:
`ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a
scripts/android-build.sh` then `:app:assembleDebug`. `adb logcat -G 16M`,
unfiltered to a file. Known start: `adb shell cmd audio set-volume 3 10`.
Before every step: `state=PLAYING(3)` and `volumeType=REMOTE` in
`dumpsys media_session`.

1. Screen off: up, down within 500 ms → next track, `get-volume 3` = 10, one
   tick. Down, up → previous. Five each; record the DOWN-to-DOWN gap of every
   rock from the `VolumeKeys` line.
2. Screen off, the false-positive arm, **uncoached**: playing at a
   comfortable level, raise the volume past comfortable and correct it back
   down the way you normally would, without thinking about any window. Ten
   corrections; record every up→down gap from the `VolumeKeys` line. Pass
   criterion in decision 6. Then the control: single taps → one step each,
   no skip; up, then down after a deliberate pause (> 1 s) → two steps.
3. Reprise activity in front: up-down → two steps, no skip; hold → ramp.
4. Another app in front, playing: up-down → skip; hold → ramp, no skip; tap
   up then hold down within the window → skip then ramp (decision 5), note
   how it reads.
5. Paused: keys stock, `volumeType=LOCAL`.
5a. The rails: at volume 25 (max) up-down → skip, volume still 25; at 0
    down-up → skip, volume still 0.
5b. Lock screen with the screen on (not the Reprise activity): up-down →
    skip (decision 2's claim).
6. Volume panel slider drag → volume moves, no skip.
7. After every skip: volume from before the gesture, not one or two steps off.
8. Tick on the skip and on nothing else.
9. Switch off, playing, screen off: stock, `LOCAL`. On again: back to 1.
10. Last track, repeat off, screen off, up-down: `next()` no-ops; note whether
    tick + restore reads as broken.
11. Across a skip: `dumpsys media_session` never shows `LOCAL` while
    `playWhenReady` (decision 3).
12. Leave the phone with working keys (playback stopped → `LOCAL`) before
    releasing the lock.

Calibration verdict from 1 and 2 as in decision 6. Write every number, the
per-step table and the logcat excerpts into
`docs/plans/volume-key-taps-device-run-<date>.md` and **commit it in the
worktree** before `land.sh` (decision 9).

## Risks

- **The gesture is unusual.** Mitigation is the subtitle and the tick; if
  the device run says it feels wrong, the same-direction double-tap (rejected
  in decision 1) is the fallback with its timer and calibration burden — a
  new plan, not a reason to land nothing.
- **Bluetooth headsets with their own volume keys** reach the same provider
  when the screen is off; an up-down within 500 ms on the headset rocks too.
  Not measured; noted for the first report.
- **Remote slider while playing.** With the switch on, SystemUI draws the
  session's slider instead of the stock one while Reprise plays (as in #974,
  accepted in that grill). The switch is the exit.
- **A bug in the takeover can kill the volume keys** while playing. Every
  branch of `adjustDeviceVolume` calls `adjustStreamVolume` or
  `setStreamVolume`; force-stop restores the keys; `LOCAL` when not playing
  keeps the window small.
- **ROM-level gestures** (LineageOS and others) fire for whichever player is
  running; reports of "it skips with the switch off" are the ROM.
- **Repeat cadence differs on other devices** — irrelevant here: the rock
  never reads a same-direction gap.

## Parallelität

One strand. The cut attempted: strand A = Core setting + FFI field (tasks 4),
strand B = Kotlin gesture, wiring, surface, tests (tasks 1–3, 5). B's
`PlaybackSettingsState` and `ReprisePlaybackService` read the FFI field
`volumeKeySkipGestureEnabled` that only A's generated bindings provide, so B
cannot compile — let alone go green — before A has merged, and A alone is
eleven lines. No disjoint file group carries a strand of its own.

- **Strand: the whole plan.** Owns `android/app/src/main/java/io/github/marvinbaudach/reprise/**`,
  `android/app/src/test/java/io/github/marvinbaudach/reprise/**`,
  `crates/reprise-core/src/library/settings*.rs`,
  `crates/reprise-android-ffi/src/playback_settings*.rs`, `docs/plans/volume-key-taps-*`.
- Merge order: n/a. Post-merge cross-checks: none — the device run is the
  cross-check, and it happens before the merge (decision 9).
