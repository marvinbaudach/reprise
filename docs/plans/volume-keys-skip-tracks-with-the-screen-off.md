---
slug: volume-keys-skip-tracks-with-the-screen-off
worktree:
branch:
phase: planned
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
3. **Apply the first step at once and keep it.** On the hold, the track skips
   and the one already-applied step stays. No delay on short presses, no visible
   undo. The price is one stray step (1/25) per skip.
4. **Exactly one skip per hold.** Further callbacks in the same hold are
   swallowed until a gap longer than `releaseAfterMs` ends it. A second skip
   would be the accidental eight-track skip #806 designed against.
5. **`REMOTE` only while playback is actually running**, `LOCAL` otherwise. This
   keeps the window in which a bug can kill the volume keys as small as
   possible — spike 1 left the phone with no volume at all, and that window is
   the whole risk.
6. **No `land.sh` before the device run is done.** #806 landed on a green gate
   alone and was reverted the same day.
7. **One strand.**

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

`holdThreshold` is a call count, not a duration; the ~234 ms is the gap a
`releaseAfterMs` would have to sit under, and the ~50 ms is what makes `Swallow`
load-bearing — a one-second hold is 16 calls, 15 of them swallowed.

**And that gap is where decision 3 breaks.** A second run measured repeated taps at 156–227 ms
apart against a hold's first repeat at 250 ms — a 23 ms window, on one hand, one
session. `releaseAfterMs` cannot separate a fast triple-tap from a hold, so
`holdThreshold = 2` would skip on tapping. The robust signal is the *second* gap
(48–50 ms, an order of magnitude below any tap), i.e. the **third** call ~300 ms
after key-down, at the price of two stray volume steps instead of one. Task 2
cannot start until that trade is decided. Screen-off and the short press are
both measured and both fine; see "Spike 5" in the findings.

**Still unmeasured: the SystemUI slider drag** — the basis for "a drag must
never skip". No shell path reaches it; it needs a finger on the real slider.

Details, including three corrections these runs force:
`docs/plans/media3-remote-volume-findings.md`, "Spike 4" and "Spike 5".

**Stop conditions.**

- **Repeated calls at the player while holding** → the route carries. Continue
  to task 2, and derive `holdThreshold` and `releaseAfterMs` from the measured
  **player-side** interval, not from the framework's ~50 ms.
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

Only if task 1 says the route carries.

New file beside `CoreControlledPlayer.kt`. Plain Kotlin, no Android types, so
its tests need no Robolectric.

```kotlin
internal enum class VolumeDirection { UP, DOWN }

internal sealed interface RemoteVolumeAction {
    /** Apply one step to STREAM_MUSIC. */
    data class Step(val direction: VolumeDirection) : RemoteVolumeAction
    data object SkipNext : RemoteVolumeAction
    data object SkipPrevious : RemoteVolumeAction
    /** This hold already skipped; swallow the rest of it. */
    data object Swallow : RemoteVolumeAction
}

internal class RemoteVolumeHold(
    private val holdThreshold: Int,
    private val releaseAfterMs: Long,
    private val isForeground: () -> Boolean,
    private val now: () -> Long,
)  {
    fun onAdjust(direction: VolumeDirection): RemoteVolumeAction
}
```

Rules, in order:

| Condition | Result |
| --- | --- |
| gap since the last call > `releaseAfterMs` | new press: count = 1, not yet skipped → `Step(direction)` |
| direction differs from the running hold | treated as a new press, same as above |
| `isForeground()` | `Step(direction)` — never skip in an open activity (decision 2) |
| this hold already skipped | `Swallow` |
| count reaches `holdThreshold` | mark skipped → `SkipNext` (UP) / `SkipPrevious` (DOWN) |
| otherwise | `Step(direction)` |

`holdThreshold = 2` is the value decision 3 implies: the first call applies a
step, the second skips, so exactly one stray step per skip. Both constants come
from task 1's measured interval, not from a guess.

`VolumeKeyTrackSwitch` from #806 (recoverable from that commit) is the ancestor
and its decision table is still right, but it **cannot be reused**: it cleared
its latch on a real key-up. There is none here, so the latch is released by a
timeout — which creates a failure mode #806 never had: press, release, press
again inside `releaseAfterMs` reads as one hold. That case gets its own test.

**A skip from this feature must never change the playback state.** Paused stays
paused. Decision 5 means the gesture cannot fire while paused anyway, but the
rule is written down so it survives any later loosening of that gate.

## Task 3 — wire it into `CoreControlledPlayer`

The wrapper already routes transport into the core; the volume overrides belong
in the same place.

- `getDeviceInfo()` → `PLAYBACK_TYPE_REMOTE` **only while playback runs**,
  `LOCAL` otherwise (decision 5); `minVolume = 0`,
  `maxVolume = getStreamMaxVolume(STREAM_MUSIC)`.
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
  no-flag twins** → ask `RemoteVolumeHold`, then either `adjustStreamVolume` or
  `commands.next()` / `commands.previousInQueueOrder()`.
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

## Task 4 — tests

- `RemoteVolumeHold`, plain JUnit: one call → `Step`; the `holdThreshold`-th
  call inside the window → `SkipNext`/`SkipPrevious`; further calls → `Swallow`;
  a call after `releaseAfterMs` → a fresh `Step`, not a continuation; a
  direction change mid-hold → a new press; `isForeground()` true → `Step` even
  at and beyond the threshold, never a skip; and the press-release-press race.
- `CoreControlledPlayer` under Robolectric, beside `PlaybackServiceLifetimeTest`
  and using its `CorelessPlaybackService` pattern: `getDeviceInfo()` reports
  remote while playing and local otherwise; `getAvailableCommands()` contains
  `COMMAND_ADJUST_DEVICE_VOLUME`; **`isCommandAvailable(COMMAND_ADJUST_DEVICE_VOLUME)`
  is `true` even though the wrapped player says otherwise** — the whole route
  hangs off that one answer, and its absence is silent everywhere else; a
  sequence of `increaseDeviceVolume(0)` calls produces exactly one `next()` on
  the fake commands; and `setDeviceVolume(volume, flags)` never produces one.
- **Mutation check.** #806's review found by mutation that its two
  "consume without a side effect" branches had no test and could be broken with
  the suite staying green. `Swallow` is the same shape here. Prove by mutation
  that breaking it turns the suite red; do not infer it from reading.

## Task 5 — gate

`scripts/check-android-suite.sh`. `ANDROID_TEST_FLOOR=334` is a floor and new
tests only raise the count — do not edit the script.

## Verification

The gate proves the decision object and the wiring. It cannot prove the platform
delivers volume keys to a remote session; that is what task 1 measures and what
Robolectric cannot see.

**Manual, on the device, mandatory before landing** (decision 6), with
`state=PLAYING(3)` confirmed in `dumpsys media_session` before each step:

1. Screen off, hold `VOLUME_UP` → next track; `VOLUME_DOWN` → previous.
2. Screen off, short press → one volume step, no skip.
3. **App in the foreground, hold → volume ramps as normal, no skip.** This is
   the check that the inversion actually happened.
4. Another app in front, playback running, hold → skip.
5. Paused: both keys behave exactly as stock Android, including hold-to-ramp.
6. The volume panel's slider still moves the volume by drag.

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
- **The timeout has no key-up to correct it.** Press-release-press faster than
  `releaseAfterMs` reads as one hold. Named constant, own test.
- **The system panel shows a remote slider** while the session is remote.
  Mirroring `maxVolume` and `getDeviceVolume()` onto `STREAM_MUSIC` keeps it
  meaningful, but it is not the stock panel.

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
