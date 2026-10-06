---
slug: volume-keys-skip-tracks-with-the-screen-off
worktree: /home/marvin/Projects/reprise-volume-keys-skip-tracks-with-the-screen-off
branch: feature/volume-keys-skip-tracks-with-the-screen-off
phase: reverted
codex_session:
created: 2026-09-02
---
# Volume keys skip tracks with the screen off

Holding a volume key skips the track **while the screen is off or another app is
in front**. In an open Reprise activity the keys behave as they always have,
hold-to-ramp included. A short press changes the volume everywhere.

This is the inverse of #806, which shipped the foreground half and was reverted
the same day as #810.

Findings that produced this plan: `docs/plans/media3-remote-volume-findings.md`.
Original design, whose rejection of this route is now the cost to design around:
`docs/superpowers/specs/2026-09-01-android-volume-keys-track-switch-design.md`.

**Reverted 2026-09-16.** Landed as #974 (`772f8075`) with decision 6 — device
run before landing — deliberately overridden; the device run afterwards showed
the headline case cannot work: with the screen off the media-session service
delivers one `ACTION_DOWN` (`pkg=android, uid=1000`) and an `ACTION_UP` with
`direction=0` per hold, no key repeats at all, for holds from 1.8 s to 8.5 s
(§ 6.1 in the handoff). Key repeats are synthesised only for a focused window,
and with the device non-interactive there is none. The one surviving path
(another app in front) over-skipped 2–3 tracks per hold because the skip's own
~200 ms of work on the player thread exceeded `repeatGapMaxMs = 100`. Verdict,
per-step table and the logcat excerpts:
`HANDOFF-2026-09-16-volume-keys-device-run.md` and
`volume-keys-device-run-2026-09-16.txt`. The revert keeps the plan text as
landed so the decisions stay readable; the findings document's claim that
"screen off behaves exactly like screen on" is contradicted by this run and
should be treated as an artefact until a logcat reproduces it.

Amended 2026-09-15, after task 1 stopped on the tap-versus-hold trade and a
survey of other players (section below): the hold signal is the repeat gap,
the skip restores the stray steps, there is a settings switch, and the skip
gives a haptic tick. Decisions 3 and 10, tasks 2–4, verification and risks
carry the change; everything else stands as grilled.

## The premise this plan does not assume

Measured three times on a Pixel 10 Pro XL. **The third run overturned the first
two**, so the premise below is not the one this plan was drafted against.

- The takeover **works**. Reporting `PLAYBACK_TYPE_REMOTE` plus the
  device-volume commands puts the platform session on `volumeType=REMOTE,
  controlType=ABSOLUTE, max=25`.
- **The platform delivers, and holding repeats.** With `state=PLAYING(3)`
  verified and *physical* keys pressed, `MediaSessionService` logged
  `dispatchVolumeKeyEvent … repeatCount=0…8` plus `Adjusting <our session> by -1`
  for each, and SystemUI drew the **remote** slider. **First repeat ~254 ms after
  key-down, then ~50 ms.** Reproduced on a second hold.
- **The adjust never reached our player, and the cause is in our own wrapper.**
  `ForwardingPlayer.isCommandAvailable(int)` does not consult
  `getAvailableCommands()`; it asks the wrapped ExoPlayer, which plays to local
  output and answers `false` for `COMMAND_ADJUST_DEVICE_VOLUME`. Media3's volume
  provider guards on exactly that method and returns early, silently
  (`MediaSessionLegacyStub$4.lambda$onAdjustVolume$1`, disassembled). All three
  spikes overrode the plural form and never the singular one.
- So spikes 1 and 2's *"no callback ever arrives"* was measured at the player
  only — nobody had logged `MediaSessionService`. The route was never the thing
  the design rejected it for.
- **Synthetic `adb shell input keyevent 24` is not a substitute.** It produces no
  `dispatchVolumeKeyEvent` and no volume change at all, while a physical press
  produces both. Every measurement on this path needs real buttons.

The open question is therefore no longer whether the route delivers. It is
whether the one-line override closes the gap, and at what cadence the callbacks
then arrive **at the player**: `onAdjustVolume` posts to the application looper
before calling in, so the player-side spacing need not mirror the framework's
~50 ms. **Task 1 answers that and is still allowed to end this plan.**

Full measurements, including the bytecode: `docs/plans/media3-remote-volume-findings.md`.

## Decisions settled in the grill

1. **This plan carries the gate and the feature.** Two of the gate's three
   outcomes delete tasks 2–5; that is accepted, so the design is argued now
   while the measurements are fresh rather than rebuilt later.
2. **The session is remote; a foreground flag blocks only the skip.** Not a
   `DeviceInfo` that follows the foreground — that would rebuild the volume
   provider on every app switch. In an open activity every callback still
   applies a volume step, so hold-to-ramp survives there by construction.
3. **Apply every step at once; the skip restores them.** No delay on a short
   press and no stray volume after a skip. Spike 5 showed the hold is only
   unambiguous at its third callback, so two steps are already applied when
   the skip fires; the skip puts `STREAM_MUSIC` back to where it stood before
   the hold's key-down. The undo coincides with the track change and hides
   behind it. *(2026-09-15; replaces "apply one step and keep it", which rested
   on a second callback that measurement found inseparable from a fast
   triple-tap.)*
4. **Exactly one skip per hold.** Further callbacks in the same hold are
   swallowed until a gap longer than `repeatGapMaxMs` ends it. A second skip
   would be the accidental eight-track skip #806 designed against.
5. **`REMOTE` only while playback is actually running**, `LOCAL` otherwise. This
   keeps the window in which a bug can kill the volume keys as small as
   possible — spike 1 left the phone with no volume at all, and that window is
   the whole risk.
6. **No `land.sh` before the device run is done.** #806 landed on a green gate
   alone and was reverted the same day.
7. **One strand.**
8. **A switch, on by default.** Settings › Audio, beside Gapless: "Hold volume
   keys to skip tracks", subtitle naming the conditions (screen off or app in
   the background, only while playing). Off means `DeviceInfo` stays `LOCAL`
   and nothing below runs — today's behaviour exactly. The 2026-09-01 spec
   argued against a switch because there was no playback settings page; there
   is one now, and the remote slider plus the lost hold-to-ramp in the
   background are worth an exit.
9. **A haptic tick on the skip.** From the service, only on the skip path,
   never on a step or a swallow. With the screen off it is the only
   confirmation before the next track becomes audible. No other player
   documents feedback here; this is our choice, not a convention.
10. **The hold signal is the repeat gap, not a count.** One constant,
    `repeatGapMaxMs ≈ 100`: a callback that close to the previous
    same-direction callback is a key repeat (measured 48–50 ms); a human tap
    is never closer than 156 ms. The ~250 ms lead-in is *not* a signal — it is
    exactly the value spike 5 found unsafe.

## What other players do (survey, 2026-09-15)

Asked because the user wanted the gesture modelled on existing players. The
answer is that there is nothing to copy:

- **No surveyed player does this through a public API.** The one demonstrated
  screen-off path in the wild is the privileged
  `SET_VOLUME_KEY_LONG_PRESS_LISTENER` grant, obtainable only via `adb shell pm
  grant`. Poweramp ships it ("for very power users only" — maxmp) and its forum
  documents it breaking on Android 11 Pixels, across MIUI updates and against
  LineageOS's built-in equivalent. Button Mapper needs the same grant despite
  being an AccessibilityService app — which independently confirms the
  2026-09-01 spec's rejection of that route. Symfonium refused the feature over
  the adb step; Neutron says stock Android delivers no long press to apps;
  Auxio (#1064) redirects users to ROM settings. Musicolet, BlackPlayer, Pulsar,
  Retro, Gramophone (source checked): nothing.
- **Scope convention matches decision 2.** Where the gesture exists it is a
  background/screen-off gesture; GoneMAD's foreground-only variant is called
  unreliable by its own developer.
- **Threshold, stray-step handling and feedback are undocumented everywhere.**
  Decisions 3, 9 and 10 are ours.
- **Expect misattributed bug reports.** ROM-level implementations fire for
  whichever player is running, and users file the result against the app
  (Auxio #1064). A report that "holding skips even with the switch off" is
  probably the ROM.

Sources are in the session that produced this amendment; the load-bearing ones
are forum.powerampapp.com topics 16929 and 21716, Auxio issue #1064,
support.symfonium.app/t/3392 and setup.buttonmapper.app.

## Task 1 — the decisive measurement (gate for everything below)

Spike, not shipped code: `feature/volume-keys-remote-spike` in
`/home/marvin/Projects/reprise-volume-keys-remote-spike`, which already carries
the instrumentation **and** the `isCommandAvailable` override.

Protocol, each precondition **verified before** the next step, never assumed:

1. **Build for the device explicitly:** `ANDROID_TARGET=aarch64-linux-android
   ANDROID_ABI=arm64-v8a scripts/android-build.sh`, then `:app:assembleDebug`. A
   plain `assembleDebug` links only the host bindings; the APK then carries no
   `lib/arm64-v8a/libreprise_android_ffi.so` and dies on launch.
2. **Hold the device lock across the whole run** — install, play, press, log
   pull. `scripts/android-build.sh` and friends call `adb` without taking it.
3. `adb logcat -G 16M`, then capture **unfiltered** to a file and grep
   afterwards. `dispatchVolumeKeyEvent` and `Adjusting …` come from the system
   media-session service, not from our PID; a `-s VolSpike` filter drops exactly
   the lines that show the framework→player gap closed.
4. Known starting volume: `adb shell cmd audio set-volume 3 10`. Spike 1 left
   `STREAM_MUSIC` muted at 0, and a muted stream is one variable too many.
5. Every adjustment is forwarded to `STREAM_MUSIC` with `FLAG_SHOW_UI`. Spike 1
   did not, and the phone's volume keys went dead. A measurement spike must
   leave the device usable.
6. Start playback. **Confirm `state=PLAYING(3)` in `dumpsys media_session`.**
   Its absence is what voided an earlier run.
7. Screen on: one short press, then a **~500 ms** hold — not two seconds. With
   the override in place the adjusts now really reach `adjustStreamVolume`, and
   at ~50 ms against `maxVolume=25` a one-second hold walks the stream end to
   end. Six repeats already prove the cadence.
8. Screen off: confirm `PLAYING` again, then one short press, then a short hold.
9. Force-stop or reinstall the normal build before releasing the device: the
   spike takes the volume keys hostage while it plays, and that survives
   unplugging.

Record per press: how many `increaseDeviceVolume` / `decreaseDeviceVolume` calls
arrive **on the player**, and the interval between them. The line that shows the
fix taking effect is `isCommandAvailable command=26 available=true` at press
time, immediately followed by an adjust.

### Outcome, 2026-09-03 — the route carries

Ran with the override installed. **16 `decreaseDeviceVolume` callbacks on
`CoreControlledPlayer` from one physical hold**, `isCommandAvailable command=26
available=true` before each. Cadence at the player: **first repeat ~234 ms after
key-down, then ~50 ms** — identical to the framework's, so the looper hop costs
nothing and the caution about bunching below is settled, not pending.

The ~50 ms is what makes `Swallow` load-bearing — a one-second hold is 16
calls, 14 of them swallowed once the third has skipped. The ~234 ms lead-in is
the gap decision 10 deliberately does not key on.

**And that gap is where the original decision 3 broke.** A second run measured
repeated taps at 156–227 ms apart against a hold's first repeat at 250 ms — a
23 ms window, on one hand, one session. No release timeout can separate a fast
triple-tap from a hold, so keying the skip to the second call would skip on
tapping. The robust signal is the *second* gap (48–50 ms, an order of
magnitude below any tap), i.e. the **third** call ~300 ms after key-down, at
the price of two stray volume steps instead of one. **Decided 2026-09-15:** the
third call it is, and the skip restores both steps (decisions 3 and 10).
Screen-off and the short press are both measured and both fine; see "Spike 5"
in the findings.

**Still unmeasured: the SystemUI slider drag** — the basis for "a drag must
never skip". No shell path reaches it; it needs a finger on the real slider.

Details, including three corrections these runs force:
`docs/plans/media3-remote-volume-findings.md`, "Spike 4" and "Spike 5".

**Stop conditions.**

- **Repeated calls at the player while holding** → the route carries. Continue
  to task 2, and derive `repeatGapMaxMs` from the measured **player-side**
  interval, not from the framework's ~50 ms.
- **Exactly one call per press, hold or not** → there is no hold signal. Stop.
  Reopen the choice between an `AccessibilityService` and dropping the feature.
  Do not synthesise a hold from a single call.
- **Still nothing at the player, with `dispatchVolumeKeyEvent` in the log** →
  the override is not the whole story. Stop and disassemble further before
  proposing a cause: on this route, two hypotheses that were reasoned instead of
  measured were both wrong.

Lumpy player-side intervals are **not** a failure. `onAdjustVolume` posts to the
application looper before calling in, so bunching is a property of the route
that task 2 has to design around — it changes which constants task 2 gets, not
whether it is buildable.

## Task 2 — `RemoteVolumeHold`, the decision as a pure object

**Precondition, on the device, before writing a line:** screen off, then
`dumpsys media_session` shows our session at `state=PLAYING(3)`. Spike 5 saw
`PAUSED(2)` after `input keyevent 26` and left it unexplained. Nothing in the
code pauses on screen-off (`MainActivity.onStop` only unbinds; the service keeps
playing) and spike 5 measured holds with the screen already off, so the
expectation is that the synthetic power key caused it — but the findings say
check, so check, and write down which it was. If playback really stops when the
screen goes off, stop here: the feature has no window.

New file beside `CoreControlledPlayer.kt`. Plain Kotlin, no Android types, so
its tests need no Robolectric.

```kotlin
internal enum class VolumeDirection { UP, DOWN }

internal sealed interface RemoteVolumeAction {
    /** Apply one step to STREAM_MUSIC. */
    data class Step(val direction: VolumeDirection) : RemoteVolumeAction
    /** Restore STREAM_MUSIC to [restoreVolume], then skip, then tick. */
    data class Skip(val direction: VolumeDirection, val restoreVolume: Int) : RemoteVolumeAction
    /** This hold already skipped; swallow the rest of it. */
    data object Swallow : RemoteVolumeAction
}

internal class RemoteVolumeHold(
    private val repeatGapMaxMs: Long,
    private val leadInMaxMs: Long,
    private val isForeground: () -> Boolean,
    private val now: () -> Long,
) {
    /** [currentVolume] is the live STREAM_MUSIC index before this call's step. */
    fun onAdjust(direction: VolumeDirection, currentVolume: Int): RemoteVolumeAction
}
```

State: the last two calls' times and the last call's direction (updated on
every call, whatever the result), whether this press has already skipped, and
the volume *before* each of the last two steps (`beforeLast`, `beforePrev`) —
both **nullable, never defaulted to a number**. A `Skip` with `beforePrev`
unset restores `beforeLast`; there is always a `beforeLast`, because the first
call ever is a fresh key-down by definition. A default of `0` here would be
`setStreamVolume(…, 0, 0)` on the first hold — the volume-hostage failure the
whole plan is built to avoid.

Rules, in order:

| Condition | Result |
| --- | --- |
| direction differs from the last call, or gap since the last call > `repeatGapMaxMs` | a fresh key-down: skipped = false; `beforePrev ← beforeLast`, `beforeLast ← currentVolume` → `Step(direction)` |
| `isForeground()` | `Step(direction)` — never skip in an open activity (decision 2) |
| this press already skipped | `Swallow` |
| otherwise (a key repeat) | mark skipped → `Skip(direction, restoreVolume)`, where `restoreVolume = beforePrev` if the previous call followed *its* predecessor within `leadInMaxMs` (and `beforePrev` is set), else `beforeLast` |

Why `beforePrev`: a hold produces the key-down (call 1, `Step`), the first
repeat ~250 ms later (call 2 — a gap above `repeatGapMaxMs`, so it reads as a
fresh key-down and applies a `Step`), the second repeat ~50 ms after that (call
3 — the first gap under the limit, so `Skip`). At call 3, `beforeLast` is the
volume before call 2 and `beforePrev` the volume before call 1: the hold's own
key-down. That is what gets restored. A tap 251 ms before the hold (measured in
spike 5's log) is call 0; its step has left the two-deep history and stays. The
hold owns exactly its key-down and its first repeat, nothing before.

Why `leadInMaxMs`: `beforePrev` is the right target only when the previous
call really was the hold's lead-in — call 2 following call 1 by the ~250 ms
repeat delay. On a device whose repeat delay is *under* `repeatGapMaxMs`, call 2
is already the skip, and `beforePrev` then points at some unrelated earlier
press — minutes old, or unset. The guard is the gap between the two previous
calls: at most `leadInMaxMs = 500` (Android's default long-press timeout, above
any key-repeat delay a user can set) and the previous call is the lead-in;
above it, the hold owns only the previous call and restores `beforeLast`. The
soft failure left is a tap under 500 ms before a hold on such a device, whose
step the restore also undoes — one lost step, never a stale volume.

`repeatGapMaxMs = 100` sits between the measured 48–50 ms repeat and the 156 ms
fastest tap; `leadInMaxMs = 500` only decides what a skip restores, never
whether it fires. A fast triple-tap (gaps ≥ 156 ms) is three `Step`s and never a
`Skip`. A hold skips once, at ~300 ms, and every later repeat is `Swallow`.
Releasing the key ends the repeats, so the next call — whenever it comes — is
more than 100 ms away and starts a fresh press: the latch needs no timer, and
the press-release-press race of the earlier draft (a release-and-press faster
than the fastest measured tap) cannot occur in practice. It still gets a
boundary test at `repeatGapMaxMs` exactly and one millisecond either side.

`VolumeKeyTrackSwitch` from #806 (recoverable from that commit) is the ancestor
and its decision table is still right, but it **cannot be reused**: it cleared
its latch on a real key-up. There is none here; the gap does that job.

**A skip from this feature must never change the playback state.** Paused stays
paused. Decision 5 means the gesture cannot fire while paused anyway, but the
rule is written down so it survives any later loosening of that gate.

**The constants are device-measured, not universal.** The repeat cadence is
the input dispatcher's key-repeat setting, which Android 15+ lets the user
change under physical-keyboard settings. Both are named constants with the
measurement beside them. If a device repeats slower than ~100 ms the gesture
degrades to "never skips", never to "skips on taps"; if it leads in faster than
100 ms the skip fires one call earlier and `leadInMaxMs` keeps the restore
honest — both the right direction to fail in.

## Task 3 — wire it into `CoreControlledPlayer`

The wrapper already routes transport into the core; the volume overrides belong
in the same place.

- `getDeviceInfo()` → `PLAYBACK_TYPE_REMOTE` **only while playback runs and
  the switch is on** (decisions 5 and 8), `LOCAL` otherwise; `minVolume = 0`,
  `maxVolume = getStreamMaxVolume(STREAM_MUSIC)`. Flipping the switch while
  playing goes through the same `DeviceInfo` change as play/pause does — one
  mechanism, two triggers. **Fallback if the `DeviceInfo` check below fails:**
  the adjust handlers read the switch too, and off means every callback is a
  `Step`. That keeps the switch behaviourally correct even if the session never
  re-reads `DeviceInfo`, at the price of the remote slider staying while
  playing. Implement the read in the handlers regardless — it is one line and
  the belt to the buckle.
- `getAvailableCommands()` → `super` plus the five device-volume commands.
  Without them Media3 silently builds no volume provider — measured, and it
  fails with no error at all.
- **`isCommandAvailable(command)` → `getAvailableCommands().contains(command)`.**
  Not optional and not instrumentation: this is the override whose absence made
  three spikes read as "the platform does not deliver". `ForwardingPlayer`
  answers the singular form from the wrapped player, and Media3's volume
  provider asks only the singular form. Derive it from `getAvailableCommands()`
  rather than listing the five constants by hand, so the two answers cannot
  drift apart — that drift is the failure being fixed.
- `getDeviceVolume()` → the live `STREAM_MUSIC` volume; `isDeviceMuted()` → the
  live mute state.
- `increaseDeviceVolume(flags)` / `decreaseDeviceVolume(flags)` **and their
  no-flag twins** → read the live `STREAM_MUSIC` index, ask `RemoteVolumeHold`,
  then: `Step` → `adjustStreamVolume(STREAM_MUSIC, direction, FLAG_SHOW_UI)`;
  `Skip` → `setStreamVolume(STREAM_MUSIC, restoreVolume, 0)` (no UI flag — the
  panel must not flash the undo), then `commands.next()` /
  `commands.previousInQueueOrder()`, then the tick; `Swallow` → nothing.
- **The tick** goes through the `Commands` interface as `fun hapticTick()`,
  beside `isActivityInForeground()`, so the wrapper never sees an Android type
  and the tests below can count calls on the fake. `ReprisePlaybackService`
  owns the implementation: `Vibrator` via `VibratorManager` (API 31+) or
  `getSystemService(Vibrator)` below; `VibrationEffect.createPredefined(
  EFFECT_TICK)` on API 29+, `createOneShot(20, DEFAULT_AMPLITUDE)` on 26–28.
  `VIBRATE` is already in the manifest; `QueueHaptics.kt` is the UI-side
  precedent and stays untouched — the service has no `View`.
- **The switch:** persist it exactly the way `gaplessEnabled` is persisted
  (`PlaybackSettingsScreen.kt:65,102,128`) — follow that path end to end rather
  than opening a second preference store. The service reads it through the
  existing `Commands` interface, like the foreground flag below. Title "Hold
  volume keys to skip tracks", subtitle "With the screen off or another app
  in front, while playing. Volume up skips forward, volume down back."
- `setDeviceVolume(volume, flags)` → `setStreamVolume`, so the panel's slider
  still works.
- **`flags` cannot separate a key press from a slider drag.** Measured: the
  framework logs `flags=4113` (`FLAG_FROM_KEY | FLAG_VIBRATE | FLAG_SHOW_UI`),
  but what arrives at `decreaseDeviceVolume(flags)` is `flags=1` — `FLAG_SHOW_UI`
  alone, 16 of 16 times. Media3 does not pass them through, so a
  `flags and FLAG_FROM_KEY` test would never fire. A SystemUI slider drag must
  still never skip a track; the distinction left is the **shape** of the call —
  a key adjusts relatively (`increase`/`decreaseDeviceVolume`), an absolute drag
  should land on `setDeviceVolume(volume, flags)`, which never reaches
  `RemoteVolumeHold`. **Unmeasured.** Confirm it with one slider drag before
  relying on it; if a drag turns out to produce relative adjusts too, the
  gesture needs a different guard and this plan needs revisiting.

`CoreControlledPlayer` takes no `Context` today; the service passes itself.
The foreground flag goes through the existing `Commands` interface as
`fun isActivityInForeground(): Boolean`, backed by a `@Volatile` field on
`ReprisePlaybackService` that `MainActivity` sets in `onResume`/`onPause`. That
keeps the wrapper's dependency shape unchanged.

Keep all of this separate from the sleep timer's `player.volume` fade
(`ReprisePlaybackService.kt:199-201`). ExoPlayer's logical gain and the device
stream are different concepts; merging them would make the sleep fade drag the
system slider.

**Verify that the session notices when `DeviceInfo` changes.** A computed
`getDeviceInfo()` does not by itself emit `onDeviceInfoChanged`, and Media3 may
cache the value from session construction. If the session does not re-read it
when playback starts, decision 5 is not implementable as written and the choice
returns to the user. Expect this to fail silently if it fails — check it
explicitly rather than inferring it from the feature working.

**Check, in the same run, whether the remote slider follows the stream.** Media3
mirrors `getDeviceVolume()` into the remote slider on `onDeviceVolumeChanged`;
a computed getter does not emit it. If the slider goes stale after a forwarded
adjust, emit the event after each one. Cosmetic while the screen is off, visible
on the lock screen — decide from what the run shows, do not guess.

## Task 4 — tests

- `RemoteVolumeHold`, plain JUnit, with a fake clock: one call → `Step`; a
  second 250 ms later → `Step`; a third 50 ms later → `Skip` whose
  `restoreVolume` is the volume before the **first** call; every further call
  at 50 ms → `Swallow`; a call > `repeatGapMaxMs` after the last swallow → a
  fresh `Step`, latch cleared; a direction change mid-hold → `Step`;
  `isForeground()` true → `Step` on every repeat, never a skip or a swallow;
  a triple-tap at 156 ms gaps → three `Step`s; a tap 251 ms before a hold →
  the skip restores to the volume *after* the tap, not before it; the very
  first call ever followed by a repeat 50 ms later → `Skip` restores that first
  call's volume, never `0` or a default; a repeat whose two predecessors are
  `leadInMaxMs` + 1 apart → restores `beforeLast`; and the boundary at
  `repeatGapMaxMs` − 1, 0 and + 1 ms.
- `CoreControlledPlayer` under Robolectric, beside `PlaybackServiceLifetimeTest`
  and using its `CorelessPlaybackService` pattern: `getDeviceInfo()` reports
  remote while playing and local otherwise; `getAvailableCommands()` contains
  `COMMAND_ADJUST_DEVICE_VOLUME`; **`isCommandAvailable(COMMAND_ADJUST_DEVICE_VOLUME)`
  is `true` even though the wrapped player says otherwise** — the whole route
  hangs off that one answer, and its absence is silent everywhere else; a
  sequence of `increaseDeviceVolume(0)` calls produces exactly one `next()` on
  the fake commands; and `setDeviceVolume(volume, flags)` never produces one.
  Plus, from the amendment: with the switch off, `getDeviceInfo()` is `LOCAL`
  while playing; a `Skip` calls `setStreamVolume` with the restore value
  *before* `next()` on the fake commands, and fires the tick exactly once;
  a `Step` and a `Swallow` never touch the vibrator.
- **Mutation check.** #806's review found by mutation that its two
  "consume without a side effect" branches had no test and could be broken with
  the suite staying green. `Swallow` is the same shape here. Prove by mutation
  that breaking it turns the suite red; do not infer it from reading. The
  restore is the second such shape: drop the `setStreamVolume` and the suite
  must go red, not stay green.

## Task 5 — gate

`scripts/check-android-suite.sh`. `ANDROID_TEST_FLOOR=334` is a floor and new
tests only raise the count — do not edit the script.

### Implementation outcome, 2026-09-16

Tasks 2–5 are implemented. The Android gate exited 0 with 636 tests, no failures,
no errors and no skips. Both required mutations were observed red before being
reverted: adding a haptic side effect to `Swallow` failed
`aStepAndASwallowNeverTick`, and removing the restore failed
`oneHoldSkipsOnceAndAnAbsoluteSliderSetNeverSkips`.

The device precondition and every manual verification item below remain for the
human verification run. No device, emulator, `adb`, or Android device-build
script was used during implementation.

## Verification

The gate proves the decision object and the wiring. It cannot prove the platform
delivers volume keys to a remote session; that is what task 1 measures and what
Robolectric cannot see.

**Manual, on the device, mandatory before landing** (decision 6), with
`state=PLAYING(3)` confirmed in `dumpsys media_session` before each step:

0. Task 2's precondition, if not already written down: screen off, session
   still `PLAYING(3)`.

1. Screen off, hold `VOLUME_UP` → next track; `VOLUME_DOWN` → previous.
2. Screen off, short press → one volume step, no skip.
3. **App in the foreground, hold → volume ramps as normal, no skip.** This is
   the check that the inversion actually happened.
4. Another app in front, playback running, hold → skip.
5. Paused: both keys behave exactly as stock Android, including hold-to-ramp.
6. The volume panel's slider still moves the volume by drag.
7. After a hold-skip, `cmd audio get-volume`/the panel show the volume from
   before the hold — not one or two steps off.
8. The tick is felt on the skip and on nothing else.
9. Switch off, playing, screen off: hold ramps like stock Android, the panel
   shows the stock slider, nothing skips. Switch on again: back to 1.
10. Last track of the queue, repeat off, screen off, hold volume up: `next()`
    has nothing to do, so the only visible effect is the tick and the volume
    undo. Note whether that reads as broken; if it does, the fix is the
    wrapper asking `commands` whether a next exists before it ticks, not a
    change to the decision object.

## Risks

- **The route may deliver nothing.** Two spikes produced no callback. Task 1 is
  the gate for exactly this and is allowed to end the plan.
- **The app takes the volume hostage.** Every path must forward to
  `STREAM_MUSIC`; one missed path leaves a dead volume key, which is what spike 1
  did to a real phone. Decision 5 shrinks the window; it does not remove it.
- **Decisions 2 and 5 pull against each other.** The grill rejected a
  `DeviceInfo` that follows the foreground as too many moving parts, then
  reintroduced runtime switching on a different trigger. That is defensible —
  playback start/stop is rare and slow, an app switch is frequent and fast — but
  it is the same mechanism, and the task-3 check above is where it is proven or
  found wanting.
- **The repeat cadence is a device setting.** `repeatGapMaxMs` is derived from
  one device's 48–50 ms; a slower repeat setting makes the gesture stop
  skipping, a faster tap than 156 ms is physiologically out of reach. Both
  fail towards "no skip". Named constant, measurement beside it.
- **The restore races the panel.** Two steps are applied and undone within
  ~300 ms; with the screen on and the app in the background the remote slider
  may visibly twitch. Accepted; the undo carries no UI flag.
- **The system panel shows a remote slider** while the session is remote.
  Mirroring `maxVolume` and `getDeviceVolume()` onto `STREAM_MUSIC` keeps it
  meaningful, but it is not the stock panel.
- **ROM-level implementations will be blamed on us.** LineageOS and others
  skip on volume-hold regardless of the player; a report that the gesture
  fires with the switch off is a ROM feature (see the survey).

## Parallelität

**No cut. One strand.**

Task 1 is a gate, not a work package: tasks 2–5 do not exist until it returns,
and two of its three outcomes cancel them outright. Starting anything alongside
it would be building on the premise the gate exists to test.

After the gate, tasks 2–4 could nominally split — 2 is one new file plus its
test, 3 touches `CoreControlledPlayer.kt` and `ReprisePlaybackService.kt` — but
that is a dependency, not parallelism: 3 does not compile without 2's API, and
both constants come from the gate. Two worktrees and a merge seam would cost
more than the wall-clock they could win on a change this size.

Merge order: n/a. Post-merge cross-checks: none — every verification step reads
files this strand owns. The device run belongs to the branch either way and is
listed under Verification.
