# Volume keys outside the app: what two spikes measured

Date: 2026-09-02. Device: Pixel 10 Pro XL (arm64), Android with `targetSdk 37`.
App under test: debug builds of `dev` at `0.1.92`–`0.1.94`.

## Why this document exists

The in-app volume-key gesture shipped as **#806** and was reverted the same day
as **#810**, because trying it on a device made the gating question concrete:
the gesture is wanted with the screen off or while another app is in front, and
unwanted while a Reprise activity is on screen — there, tapping *next* beats
holding a key.

The successor was to take the **Media3 remote-volume** route that
`docs/superpowers/specs/2026-09-01-android-volume-keys-track-switch-design.md`
had considered and rejected. Before planning it, one premise needed measuring,
because every variant of that plan rests on it:

> A held volume key must produce a *stream* of `increaseDeviceVolume()` calls.
> `VolumeProvider` delivers a bare direction with no key-up, so "held" can only
> be inferred from several calls arriving in quick succession.

If the route delivers exactly one call per press regardless of hold duration,
the route is not awkward — it is unusable, and no timing scheme can be built.

**The premise could not be confirmed, because no call arrived at all.**

## What the code looks like (established, not guessed)

- `androidx.media3:media3-exoplayer:1.11.0` and `media3-session:1.11.0`
  (`android/app/build.gradle.kts:134-135`).
- A real `ExoPlayer`, wrapped by `CoreControlledPlayer : ForwardingPlayer`
  (`CoreControlledPlayer.kt:7-10`), built into a `MediaSession` inside
  `ReprisePlaybackService : MediaSessionService` (`ReprisePlaybackService.kt:118-121`).
  That wrapper already intercepts `seekToNext()`/`seekToPrevious()` and routes
  them into the Rust core, so it is the natural home for device-volume overrides.
- Before these spikes there was **no** `AudioManager`, `STREAM_MUSIC`,
  `DeviceInfo` or `VolumeProvider` anywhere under `android/`. Volume was
  app-internal only (`player.volume`, used by the sleep-timer fade).

From `javap` on the 1.11.0 artifacts: `MediaSessionLegacyStub` builds a
`VolumeProviderCompat` only when **both** hold — `DeviceInfo.playbackType !=
PLAYBACK_TYPE_LOCAL` **and** `getAvailableCommands()` contains
`COMMAND_SET_DEVICE_VOLUME` or `COMMAND_ADJUST_DEVICE_VOLUME`. Miss the second
and nothing happens, with no error. Both spikes satisfied both conditions.

## Spike 1 — does the takeover happen?

Overrode `getDeviceInfo()` to report `PLAYBACK_TYPE_REMOTE` with `maxVolume=25`,
added all five device-volume commands, logged every adjust call. Deliberately
did **not** forward anything to the music stream.

| Observation | Evidence |
| --- | --- |
| The session really did go remote | `dumpsys media_session`: `volumeType=REMOTE, controlType=ABSOLUTE, max=25` — that 25 exists nowhere but in the spike |
| The keys were captured | the device's volume stopped responding entirely |
| No callback arrived | zero log lines, while 1216 other log lines from the same process flowed |
| `getDeviceVolume()` was not consulted | dump showed `current=0`; the spike would have answered `10` |

**Cost note:** taking the volume away without forwarding it made the phone's
volume keys dead while the spike was installed. That was avoidable and should
not be repeated — a measurement spike must leave the device usable.

## Spike 2 — is our player consulted at all?

Same overrides, plus: every method logs (including the getters), and every
adjustment is forwarded to `STREAM_MUSIC` via
`AudioManager.adjustStreamVolume(..., FLAG_SHOW_UI)` so the device stays usable.

What the log shows at session construction, repeatedly:

```
getAvailableCommands adjust=true adjustFlags=true get=true stream=6
getDeviceInfo stream=6
```

So Media3 **does** consult the wrapper, and it **does** see the adjust commands.
That kills the first hypothesis (that `getAvailableCommands()` on a
`ForwardingPlayer` is not the effective lever — it is).

And yet:

| Observation | Evidence |
| --- | --- |
| Adjust callbacks | **0**, in any screen state |
| `getDeviceVolume()` calls | **0** — so `current=0` in the dump is a default, never a read value |
| Screen on | volume behaved normally — i.e. Android handled the keys locally and the remote session never saw them; the spike's forwarding never ran |
| Screen off | the keys did nothing at all |

## What is still unknown, and the confound that caused it

The screen-off observation is **not usable evidence**: `dumpsys` afterwards
showed `state=PAUSED(2)`. A paused session is not a volume-key target, so
"nothing happened" is the expected result and says nothing about the route. The
run needs repeating with `state=PLAYING(3)` verified *before* the keys are
pressed — that check is cheap and belongs in the protocol, not in the analysis
afterwards.

So the load-bearing question remains open:

> Does a remote Media3 session receive volume keys at all on this device, and if
> so, does holding produce repeated calls?

## The lead worth following first

```
Global priority session is com.android.server.telecom/HeadsetMediaButton/1
Media button session is io.github.marvinbaudach.reprise/androidx.media3.session.id./121
Volume key long-press listener: null
```

For **media buttons** the app is already the system's chosen receiver. For
**volume keys** it is not. The design document treated these as one mechanism;
the platform clearly does not. Any further work should start by establishing
what makes a session the volume-key target — playback state, audio focus,
priority, or whether local audio output disqualifies a remote volume claim at
all — before anyone writes another line about timing constants.

## Do not repeat

- **Do not plan a timing scheme on this route yet.** Two spikes produced no
  callback; a 450 ms constant would be a parameter of an unobserved mechanism.
- **Do not take the volume away without forwarding it.** Spike 1 did, and the
  device's volume keys went dead.
- **Do not test with the session paused.** Verify `state=PLAYING(3)` in
  `dumpsys media_session` immediately before pressing.

## Reusable from the reverted work

`VolumeKeyTrackSwitch` (in #806, now removed from `dev` — recover it from that
commit) was a pure decision object with no Android types, and its decision table
is still the right behaviour. Note one thing it cannot carry over: it cleared
its latch on a real key-up event. This route has no key-up, so any successor
needs a timeout instead — with a named constant and a test, since that timeout
also creates a failure mode #806 never had (press, release, press again quickly
→ the second press read as a continuation of the first).

The review of #806 also found, by mutation, that the two "consume the event
without a side effect" branches had no test and could be broken with the suite
staying green. Whatever the successor looks like, it needs that kind of check.

---

# Spike 3 — 2026-09-02, Pixel 10 Pro XL, physical keys

The first spike to press real buttons. It answers the question the plan's task 1
was written for, and it overturns the conclusion of spikes 1 and 2.

Instrument: `feature/volume-keys-remote-spike`, commit `a57446bcc2` — an
instrumented `CoreControlledPlayer` reporting `PLAYBACK_TYPE_REMOTE`
unconditionally, advertising all five device-volume commands, logging every
device-volume method on tag `VolSpike` with `System.nanoTime()`, and forwarding
every mutation to `STREAM_MUSIC` with `FLAG_SHOW_UI`.

## Verdict on task 1's gate: a fourth outcome

The plan enumerated three. None of them fired cleanly, and that is the finding:

| Plan's stop condition | Measured |
| --- | --- |
| Repeated calls while holding → route carries, continue to task 2 | **True at the session** — ~50 ms cadence, the constants are derivable |
| No call at all → route does not deliver, stop | **True at the player** — zero callbacks on `CoreControlledPlayer` |

Both halves are true at different layers, because the call is dropped between
them. `RemoteVolumeHold` is driven by `increaseDeviceVolume`/`decreaseDeviceVolume`
on `CoreControlledPlayer` (task 3), and those never fired.

The cause was found afterwards and is in our own wrapper, not in the platform or
in Media3 — see "Resolved" below. With that one override added, the gate's first
condition is the one that holds and tasks 2–5 become buildable against the
measured constants. Until a device run confirms the callbacks actually arrive,
treat that as expected rather than established.

## The result: the platform delivers, and holding repeats

`state=PLAYING(3)` verified before the press. One physical hold of the volume
key produced, in `MediaSessionService`:

```
15:44:02.567  dispatchVolumeKeyEvent ... KEYCODE_VOLUME_DOWN, repeatCount=0
15:44:02.567  Adjusting io.github.marvinbaudach.reprise/androidx.media3.session.id./128 by -1. flags=4113
15:44:02.821  ... repeatCount=1        (+254 ms)
15:44:02.870  ... repeatCount=2        (+49 ms)
15:44:02.921  ... repeatCount=3        (+51 ms)
15:44:02.971  ... repeatCount=4        (+50 ms)
15:44:03.021  ... repeatCount=5        (+50 ms)
15:44:03.072  ... repeatCount=6        (+51 ms)
15:44:03.122  ... repeatCount=7        (+50 ms)
15:44:03.172  ... repeatCount=8        (+50 ms)
```

Each one is followed by `vol.VolumeDialogControl: onRemoteVolumeChanged:
showui? true` — SystemUI draws the **remote** slider, not the stock one.

Reproduced on a second hold at 15:44:39.555 with the same shape. 46
`dispatchVolumeKeyEvent` lines across the run.

**Measured cadence: first repeat ~254 ms after key-down, then ~50 ms.**
Those are the numbers `holdThreshold` and `releaseAfterMs` must be derived from.

## Where it dies: between the session and the player

Not one device-volume method on our `ForwardingPlayer` was ever called. Across
the whole run, on tag `VolSpike`:

| Method | Calls |
| --- | --- |
| `getAvailableCommands()` | many — a ~3.0 s poll, plus extra hits at press time |
| `getDeviceInfo()` | at session setup |
| `increaseDeviceVolume` / `decreaseDeviceVolume` (both overloads) | **0** |
| `getDeviceVolume()` | **0** |
| `setDeviceVolume` / `setDeviceMuted` | **0** |

So the chain is:

```
physical key  →  MediaSessionService.dispatchVolumeKeyEvent   ✅ reaches our session
              →  "Adjusting <our session> by -1"              ✅ the adjust is made
              →  SystemUI onRemoteVolumeChanged               ✅ remote slider drawn
              →  Media3 session → ForwardingPlayer            ❌ never arrives
```

`getDeviceVolume()` never being called also explains the session's stale
`current=0` in `dumpsys media_session` while `STREAM_MUSIC` stood at 10.

No Media3 exception in the log; the call is dropped silently.

## Corrections to earlier conclusions

- **Spikes 1 and 2 concluded "no callback arrives".** That was measured at the
  player. At the framework level the callbacks were most likely arriving all
  along — nobody had logged `MediaSessionService`. The route was never the
  problem the design rejected it for.
- **"Missing device-volume commands cause a fixed provider, hence no dispatch"**
  (proposed 2026-09-02 from a Media3 1.11.0 source reading) does not hold. It is
  the hypothesis line 78 of this document already killed: the commands were
  advertised, `adjust=true` was logged, and dispatch happens either way. The
  provider is relative/absolute here and the framework dispatches into it.
- **Synthetic `adb shell input keyevent 24` is not a substitute.** It produced no
  `dispatchVolumeKeyEvent` and no volume change at all, while a physical press
  produced both. Any future measurement of this path must use real buttons.

## Resolved: the gap is in our wrapper, not in Media3

Established by disassembling media3-session 1.11.0. Media3 behaves correctly;
the spike advertised its commands in a way that cannot be seen.

`MediaSessionLegacyStub$4` is the `VolumeProviderCompat`. Its `onAdjustVolume`
posts to the application looper, and the posted lambda opens with a guard:

```
lambda$onAdjustVolume$1(PlayerWrapper, int direction, int flags):
   0: PlayerWrapper.isCommandAvailable(26)   // COMMAND_ADJUST_DEVICE_VOLUME
   6: ifne 19
   9: PlayerWrapper.isCommandAvailable(34)   // …_WITH_FLAGS
  15: ifne 19
  18: return                                  ← both false: silent drop, no log
  19: … → PlayerWrapper.increaseDeviceVolume(flags) / decreaseDeviceVolume(flags)
```

The early `return` at offset 18 is the drop. It is reached because:

```
PlayerWrapper.isCommandAvailable(i)
  → ForwardingPlayer.isCommandAvailable(i)
       0: getfield player          ← the WRAPPED player
       5: Player.isCommandAvailable(i)
```

`ForwardingPlayer.isCommandAvailable(int)` **does not consult
`getAvailableCommands()`**. It delegates straight to the wrapped player. So the
call passes through our `CoreControlledPlayer` — itself a `ForwardingPlayer` —
down to the plain ExoPlayer, which plays to local output and therefore reports
`false` for 26 and 34.

**The spike overrode `getAvailableCommands()` but not `isCommandAvailable(int)`.**
Media3 asks the second one and never sees the first. That is why
`getAvailableCommands()` was logged constantly while no adjust ever arrived —
the two are different questions, and only one of them was answered.

This explains all three spikes at once, including the ones that concluded the
platform was at fault.

### The fix

In `CoreControlledPlayer`, override the singular form so it answers from the
same source as the plural one:

```kotlin
override fun isCommandAvailable(command: Int): Boolean =
    getAvailableCommands().contains(command)
```

(The spike declares `override fun getAvailableCommands(): Player.Commands`, so
the call form — not a synthesised `availableCommands` property — is what matches
the file.)

Not a hand-written whitelist of the five constants: deriving it from
`getAvailableCommands()` keeps the two answers from drifting apart, which is the
exact failure being fixed.

**This override is the one part of the spike that must survive teardown.**
Everything above it in that file sits under a "remove this entire device-volume
surface after the Pixel measurement" comment. The override is not an instrument;
it is the fix, and deleting it with the rest would restore the silent drop.

**Still unverified on device.** The override is now in the spike
(`feature/volume-keys-remote-spike`, commit `6a8fdbb0c5`; see the run protocol
below), but nobody has pressed a button with it installed. Task 1 is
not closed until a run produces `increaseDeviceVolume` / `decreaseDeviceVolume`
callbacks *on `CoreControlledPlayer`*. Expect the count to follow
`repeatCount`; do **not** expect the player-side spacing to mirror the
framework's ~50 ms — `MediaSessionLegacyStub$4.onAdjustVolume` posts to the
application looper before calling in, so bunching there is a property of the
route, not a failed override.

Flag decode for that run: the framework hands the adjust over with `flags=4113`
= `FLAG_FROM_KEY (4096) | FLAG_VIBRATE (16) | FLAG_SHOW_UI (1)`. The bytecode
shows these arriving intact at `increaseDeviceVolume(flags)` — what forwarding
them on to `adjustStreamVolume` then does is not measured here.

## Protocol for the next run

Beyond the three "do not repeat" rules above:

- **Capture logcat unfiltered.** `dispatchVolumeKeyEvent` and
  `Adjusting <session> by -1` come from the system media-session service, not
  from our PID. Filtering by app PID or `-s VolSpike` loses exactly the lines
  that show whether the framework→player gap closed. Capture to a file and grep
  for `VolSpike|dispatchVolumeKeyEvent|Adjusting|onRemoteVolumeChanged` after.
- **`adb logcat -G 16M` first.** The spike is chatty and the default ring buffer
  can roll away the eight callback lines the whole run exists to capture.
- **Hold short — ~500 ms.** With the override in place the adjusts now really
  reach `adjustStreamVolume`; at ~50 ms against `maxVolume=25` a one-second hold
  walks the stream from end to end. Six repeats already prove the cadence.
- **The line that shows the fix working** is `isCommandAvailable command=26
  available=true` at press time, immediately followed by an adjust. It is logged
  only for commands 26 and 34, so it does not flood.

## Environment notes for the next run

- **Build for the device explicitly.** A plain `assembleDebug` links only the
  host bindings; the APK then carries no `lib/arm64-v8a/libreprise_android_ffi.so`
  and dies on launch. Use
  `ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a scripts/android-build.sh`.
- `STREAM_MUSIC` was found muted at index 0 on the speaker before the run — a
  leftover from spike 1. Check and restore it (`adb shell cmd audio set-volume 3 10`);
  a muted stream is one more variable in a measurement that has enough.
- The phone repeatedly enumerated and disconnected within ~2 s through the
  USB-C hub before finally holding. Plug into the machine directly.
- **The spike build takes the volume keys hostage while it plays, and that
  survives unplugging.** With the instrumented build installed, every volume key
  press during playback is routed to the remote session and dropped — the keys
  do nothing. This is the plan's headline risk, realised. Force-stop the app or
  reinstall the normal build before leaving the device:
  `adb shell am force-stop io.github.marvinbaudach.reprise`.

---

# Spike 4 — 2026-09-03, Pixel 10 Pro XL, physical keys, override installed

The run spike 3 was missing. Same instrument plus the `isCommandAvailable`
override (`feature/volume-keys-remote-spike`, `6a8fdbb0c5`), APK built for
`arm64-v8a`, `STREAM_MUSIC` unmuted and set to 10, `state=PLAYING(3)` verified,
logcat captured unfiltered at 16M.

## The route carries. Task 1's first stop condition is the one that fired.

One physical hold of `VOLUME_DOWN`, app in the foreground, screen on:

| | Framework | `CoreControlledPlayer` |
| --- | --- | --- |
| `dispatchVolumeKeyEvent` | 17 (`repeatCount=0…15`, then `ACTION_UP`) | — |
| Adjust callbacks | 16 × `Adjusting … by -1` | **16 × `decreaseDeviceVolume`** |

Player-side timing, first call to last: `+234 ms`, then 50, 52, 55, 49, 48, 52,
50, 51, 49, 53, 49, 53, 51, 52.

The same numbers spike 3 measured at the framework, so the looper hop costs
nothing measurable and **there is no bunching to design around**.

**Which number feeds which constant** — they are not interchangeable:

- `holdThreshold` is a *call count* and was already fixed at 2 by the plan's
  decision 3. Nothing here measures it; the ~234 ms only shows the second call
  arrives soon enough that the skip feels immediate.
- `releaseAfterMs` is a *gap* and comes from the **234**, not the 50: it must
  sit above the largest gap inside a hold (234 ms) and below the shortest human
  press-release-press. ~250–300 ms is what this data supports — **and spike 5
  shows that window does not exist**: repeated taps arrive 156–227 ms apart. Read
  that section before using this number.
- The ~50 ms is what makes `Swallow` load-bearing rather than decorative — a
  one-second hold is 16 calls, 15 of them swallowed.

The line that shows the fix taking effect, before every single adjust:

```
isCommandAvailable command=26 available=true
isCommandAvailable command=34 available=true
decreaseDeviceVolume flags=1
```

`getDeviceVolume` was called 50 times in the run. In spikes 1–3 it was called
**zero** times. The wrapper is now genuinely the session's volume source.

## Three corrections this run forces

- **`FLAG_FROM_KEY` does not reach the player.** The framework logs
  `flags=4113` on every adjust, but what arrives at `decreaseDeviceVolume(flags)`
  is `flags=1` — `FLAG_SHOW_UI` alone, 16 times out of 16. Media3 does not pass
  the framework's flags through. Any design that planned to tell a key press
  from a SystemUI slider drag by `flags and FLAG_FROM_KEY` is dead on arrival.
  The distinction that remains available: a key produces
  `increaseDeviceVolume`/`decreaseDeviceVolume` (relative), while an absolute
  slider drag should produce `setDeviceVolume(volume, flags)`. **Not measured
  here** — nobody dragged the slider — so treat it as the next thing to check,
  not as established.
- **There is a key-up at the framework, and it does not reach the player.**
  `ACTION_UP` produces `Adjusting … by 0. flags=4116`, and the player is asked
  `isCommandAvailable command=26` — but no adjust callback follows, because
  direction 0 is neither increase nor decrease. So the plan's premise holds: the
  hold must be released by a **timeout**, not by an event. This is also the
  partial evidence on the missing short press below: since key-up produces no
  callback, a tap should produce exactly one — which is what `holdThreshold = 2`
  rests on.
- **A foreground Reprise activity does not keep the keys local.** `MainActivity`
  had focus and the screen was on, and the keys still went to the remote session
  — the volume did not move by itself; every step came through our player. Spike
  2's "screen on: volume behaved normally" was an artefact of the callbacks
  never arriving. The foreground exemption (decision 2) therefore has to be
  implemented in code; the platform will not provide it.

## Still open

- **The short press was not captured, and it is the gap that can still kill
  decision 3.** Only one key-down sequence reached the log; a separate short
  press produced no `dispatchVolumeKeyEvent` at all. If a tap turns out to
  produce *two* quick calls, `holdThreshold = 2` skips a track on every tap —
  exactly what #806 was reverted for. The key-up finding above argues it will
  produce one, but that is inference, not measurement. Required, not optional:
  one deliberate short tap, nothing else, and count.
- **Screen-off was not run.**
- **The slider drag was not run** (see the `FLAG_FROM_KEY` correction).

## How the run ended

Interrupted, not finished: `device-lock` passed to another session (`viz-gain`,
"Aufnahme Visualizer-Uebergang") at 19:45:55 host time while the spike build was
the installed build.

**Whether that session recorded against a *playing* spike is not established.**
The phone's log clock and the host clock were ~5 h 48 min apart during the run
(the device clock jumped afterwards), so the two timelines cannot be lined up
with confidence. What the log does show: our session's last activity is at
13:51:00 device-log time, and a `scrcpy` virtual display was destroyed two
seconds later. Certain: the spike build was installed when the lock changed
hands. Not certain: that it was audible during their take.

The device was left restored — the app force-stopped, `STREAM_MUSIC` back at 10,
and the backed-up **0.1.96** reinstalled over the 0.1.94 spike build.

---

# Spike 5 — 2026-09-03, same instrument, the three missing measurements

Run 2 with the same build. Log: `scratchpad/volspike-run2.log`.

## Screen off behaves exactly like screen on

`state=PLAYING(3)` verified with the screen already off, then physical keys:

| Gesture | Player callbacks |
| --- | --- |
| one short tap | **1** |
| one ~600 ms hold | **10** — first repeat **+250 ms**, then 48–50 ms |

Same shape as screen-on, and the `ACTION_UP` again produces no callback. The
feature's core premise — the keys reach a remote session with the screen off —
**holds**.

## The short press: one call, twice measured

Screen on, a deliberate tap: `ACTION_DOWN repeatCount=0` → exactly one
`decreaseDeviceVolume`, `ACTION_UP` 1.09 s later → nothing. Screen off, six taps
in a row: six calls, one each. So `holdThreshold = 2` does not skip on a single
tap. That question is closed.

## But repeated taps and a hold are barely separable by gap

The six screen-off taps, measured callback to callback:

```
200 ms, 156 ms, 203 ms, 212 ms, 227 ms
```

The hold's first repeat: **250 ms**. And a tap immediately followed by a hold
put a real press-release-press gap of **251 ms** in the same log.

**So the window between "still tapping" and "now holding" is 227 → 250 ms — 23
ms wide, on one device, one hand, one session.** A `releaseAfterMs` of 250–300 ms
as derived in spike 4 would read a fast triple-tap as a hold and skip a track:
exactly the accidental skip #806 was reverted for, arriving through a different
door.

The separation that *is* robust is the one after the first repeat: **48–50 ms**,
an order of magnitude below the fastest human tap here (156 ms) and below the
physiological floor. A hold is unambiguous at its **third** call, not its second.

This is a design constraint for task 2, not a fact task 2 can absorb unchanged:

- Keying the skip to the second call (decision 3, `holdThreshold = 2`) rests on
  the 250 ms gap and is therefore **not safe as measured**.
- Keying it to the first gap **≤ ~100 ms** — the third call, ~300 ms after
  key-down — is unambiguous, at the price of two stray volume steps instead of
  one and ~50 ms more latency before the skip.

Decision 3 was argued when the second call looked like a clean signal. It is the
user's call whether the extra step is acceptable; the measurement only says the
cheap version cannot be made reliable by choosing a better constant.

## Not measured: the SystemUI slider drag

Still open, and still the basis for "a drag must not skip". No shell path
exercises it: `cmd media_session volume --set/--adj` goes to `AudioService` and
never reaches the session — zero player callbacks. The remote slider appears
only on a physical key press, and a synthetic `input keyevent` produces no
dispatch at all on this route. It needs a finger.

The one attempt got as far as opening the dialog and no further: a physical hold
at 20:24:37 produced the adjusts *and* `vol.VolumeDialogControl:
onRemoteVolumeChanged … showui? true` with the `VolumeDialog` window being
created — so the remote slider was on screen. No `setDeviceVolume` followed, and
`setDeviceVolume`/`setDeviceMuted` were called **zero** times in the whole run.
That is the absence of a drag, not evidence about one.

**The spike build is no longer on the device.** At 20:33 another session
installed its own build — also `versionName=0.1.94`, so the version string does
not distinguish them. Verified afterwards: `volumeType=LOCAL` and not one
`VolSpike` line. Any further measurement has to reinstall
`feature/volume-keys-remote-spike` first and re-verify `volumeType=REMOTE`
before pressing anything.

## The device lock expired mid-measurement, twice

Both interruptions have the same cause: `DEVICE_LOCK_TTL` is 1800 s, and a
measurement that waits for a human to press a button idles far longer than that
between `adb` calls. The lease expired silently while the run was still live,
and another session legitimately took a free lock. Re-acquire (or extend) the
lease at every hand-off point, and treat "no `adb` call for 30 minutes" as the
moment the device stops being yours.

## One observation, unexplained

Turning the screen off (via `input keyevent 26`) left the session at
`PAUSED(2)`; playback had to be restarted with a media key. Whether the app
pauses on screen-off by itself or the power keyevent caused it is **not
established** — but if playback really stops when the screen goes off, the
feature's whole scenario needs re-examining. Check before task 2.

---

# Spike 6 — 2026-09-03, third run: the slider drag stayed out of reach

Spike reinstalled and re-verified (`volumeType=REMOTE, max=25, current=10`, and
`current` now tracking the real stream instead of the stale 0 of earlier runs).

**`setDeviceVolume` and `setDeviceMuted`: zero calls, in every run so far.** Not
because a drag was dropped — because no drag ever reached the log. Three attempts
produced three key *holds* instead (18, 13 and 12 `decreaseDeviceVolume` calls,
each with a matching `dispatchVolumeKeyEvent`). The distinction the plan needs —
relative adjust from a key vs. absolute set from a drag — is therefore **still
unmeasured**, and every design that leans on `setDeviceVolume` never firing for
a key press rests on inference.

No synthetic substitute exists. `cmd media_session volume --adj lower --show`
opens no `VolumeDialog` window at all (checked in `dumpsys window`), and it
reaches `AudioService` rather than the session, so it produces neither a dialog
to swipe on nor a player callback.

## Two things worth keeping from this run

- **The remote slider shows a stale value unless the session is told.** With an
  older session still cached, SystemUI drew `onRemoteUpdate: Reprise: 10 of 25`
  while `STREAM_MUSIC` stood at 0. After a fresh start it tracked correctly,
  because `getDeviceVolume()` was read again. Shipped code must emit
  `onDeviceVolumeChanged` when it moves the stream, or the system slider drifts
  from the actual volume.
- **The app ANR'd once, ~3 s after a cold start** (`Input dispatching timed out`,
  `.MainActivity`), before any volume key was touched, and the dialog sat until
  it was dismissed 6 minutes later, killing the app. **Not attributed to the
  spike** — nothing in the trace points at the volume overrides, and no key had
  been pressed. Worth watching, because the overrides do synchronous
  `AudioManager` binder calls and Media3 asks for them at key-repeat rate.

## Where this leaves the route

Everything the feature needs from the platform is measured and holds: the keys
reach a remote session with the screen on and off, a tap is one call, a hold
repeats at ~50 ms after a ~250 ms lead-in, and no key-up reaches the player. The
two open items are both about *not* firing: the slider drag, and whether the
250 ms window can separate a fast tap sequence from a hold (spike 5 says it
cannot).
