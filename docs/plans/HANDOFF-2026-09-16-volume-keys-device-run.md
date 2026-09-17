# Handoff 2026-09-16 — volume keys: the device run that the landing skipped

Feature landed as **PR #974**, squash `772f8075` on `dev` (desktop 0.1.207,
android 0.1.139). Holding a hardware volume key skips the track while the
screen is off or another app is in front, only while Reprise is playing; a
short press is a volume step; in an open Reprise activity the keys are stock.
Settings › Audio has the switch "Hold volume keys to skip tracks", default on.

Plan (now `phase: shipped`, all decisions and the amended tasks):
`docs/plans/volume-keys-skip-tracks-with-the-screen-off.md`. Measurements from
the six spikes: `docs/plans/media3-remote-volume-findings.md`.

**Decision 6 of the plan — "no `land.sh` before the device run" — was
overridden on purpose** by the user on 2026-09-16, after the review. So the
on-device verification is the open item, and it is not optional: #806 landed
green and was reverted the same day for a behaviour Robolectric cannot see.

## Result of the device run (2026-09-16, 20:40–22:15, Pixel 10 Pro XL, build 0.1.139 = `772f8075`)

Evidence: `docs/plans/volume-keys-device-run-2026-09-16.txt` (excerpts), raw
logcat kept at `target/volume-keys-device-run-logcat-2026-09-16.txt` (untracked,
73 MB). Every press below was a physical key; `dumpsys media_session` and
`dumpsys power` were read before each step.

**Verdict: the feature cannot work for its headline case.** With the screen
off the session receives exactly one `Adjusting … by ±1` per hold, however
long the hold — and a second, independent defect makes the case that does
work (another app in front) skip 2–3 tracks per hold.

| Step | Result |
| --- | --- |
| Precondition, item 1: screen off keeps `PLAYING(3)` | ✅ measured; the spike-5 `PAUSED` was the synthetic power key |
| Precondition, item 2: `volumeType=REMOTE` while playing, `LOCAL` when paused | ✅ both halves — `onDeviceInfoChanged` does reach the session |
| Item 3: re-entrant `publishDeviceVolume` | no Media3 warning, no exception in the whole log |
| 6.3 Reprise in front, hold → ramp, no skip | ✅ (3×; lead-in 240–258 ms, repeats 48–52 ms — the plan's constants match this device) |
| 6.2 screen off, taps → one step each, no skip | ✅ (11 taps, 120–170 ms down) |
| **6.1 screen off, hold → skip** | ❌ **never skips.** Holds of 1.8 s, 2.1 s, 2.2 s and 8.5 s each produced `ACTION_DOWN repeatCount=0` → `Adjusting by 1`, then `ACTION_UP` → `Adjusting by 0` → dropped in `VolumeProviderCompat` ("Ignoring unknown direction: 0"). **No key repeats at all.** Sender is `pkg=android, uid=1000, asSystem=false` instead of the foreground window's `pkg=<app>, asSystem=true`. Audible effect: one step louder per hold. |
| 6.4 other app in front (Brave), hold → skip | ⚠️ skips, but **3 skips in a 1.1 s hold, 2 in a 0.9 s hold** (BUFFERING→PLAYING→BUFFERING cycles at 41.179 / 41.679 / 41.939). The volume panel shows and the level moves. |
| 6.5 paused → keys stock, `LOCAL` | ✅ |
| 6.6–6.10 | not run — pointless after 6.1 |
| Control: AOD off (`doze_always_on=0` was already the device setting), screen off, 8.5 s hold | same as 6.1: DOWN and UP only |

### Why 6.1 fails (hypothesis, consistent with every line in the log)

Key repeats are synthesised when a key is delivered to a focused window. With
the device non-interactive no window receives it; the policy hands the raw
hardware DOWN and UP straight to the media-session service, and the UP is
`direction=0`, which the compat volume provider discards before Media3 sees
it. The app therefore gets one callback per hold and no signal when the hold
ends — **no constant can be tuned to recover this**; there is nothing to
count. This contradicts `media3-remote-volume-findings.md` § "Screen off
behaves exactly like screen on" (10 callbacks on a 600 ms hold). Today's run
could not reproduce that on the same device in any screen-off configuration;
treat the spike's screen-off row as an artefact until someone shows the
logcat that produced it.

### Why 6.4 over-skips

`RemoteVolumeHold.onAdjust` measures the gap between *player-side* callbacks
against `repeatGapMaxMs = 100`. The skip itself (`restore volume → next() →
tick`) runs on the same thread and takes ~200 ms, so the next repeat arrives
>100 ms after the last one, counts as `freshPress`, resets `skipped`, and the
repeat after that skips again. The findings doc warned about exactly this
("do not expect the player-side spacing to mirror the `repeatCount`"). Also
note: the framework lead-in (~250 ms) already exceeds 100 ms, so the second
event of every hold is a `freshPress` too — two `Step`s land before the
`Skip`, not one.

### Options, in the handoff's own ladder

1. **Revert #974** (recommended). The gesture's purpose was the pocket / screen
   off; that path is dead by construction, and the surviving path (another app
   in front) is both niche and currently broken. A switch that defaults to off
   for a gesture that cannot do what its label says is worse than no switch.
2. Flip the default to off and keep the code for the other-app case — only
   worth it if someone wants that case *and* fixes the over-skip (measure
   gaps against the event's own timestamps, or latch `skipped` until a gap
   well above the repeat rate, e.g. 400 ms).
3. Do nothing: users with the switch on get "hold = one step louder" with the
   screen off and multi-skips in other apps. Not acceptable.

The device was left with playback paused (`volumeType=LOCAL`), music volume
8/25, the switch still on, build 0.1.139 installed.

Side request from the user, still open: the swipe transitions between songs
in the Android player screen "look rough" — needs a screen recording, separate
task.

## What was verified, and what was not

Verified (Codex report, exit codes stated, not re-measured by hand):
`scripts/check-android-suite.sh` exit 0 — 106 suites, 636 tests; mutation
checks red-then-green for `Swallow`, the restore, and the DOWN transport call;
`cargo test --workspace`, clippy `-D warnings`, fmt, audit clean. Two Sonnet
reviews (Kotlin generic, `rust-reviewer`): nothing above Medium, both Medium/Low
test-sharpness findings applied in `87e5329d`.

**Not verified — anything that needs the platform:**

1. Task 2's precondition: with the screen off, the session still reports
   `state=PLAYING(3)` in `dumpsys media_session`. Spike 5 once saw `PAUSED(2)`
   after `input keyevent 26` and left it unexplained; nothing in the code
   pauses on screen-off, so the expectation is that the synthetic power key
   caused it. If playback really stops on screen-off, the feature has no
   window and the switch should default off until that is understood.
2. Whether Media3's volume provider registers its listener through
   `CoreControlledPlayer.addListener` — otherwise `onDeviceInfoChanged`
   (emitted by `refreshDeviceInfo()`) never reaches the session, the session
   keeps the `DeviceInfo` it read at construction, and the remote takeover
   either never happens or never stops. Symptom to look for: the SystemUI
   slider is the *remote* one only while playing with the switch on, stock
   otherwise. The plan expected this to fail silently if it fails.
3. `publishDeviceVolume()` (`CoreControlledPlayer.kt`, ~line 177) calls
   listeners synchronously inside `increaseDeviceVolume`/`decreaseDeviceVolume`,
   i.e. re-entrantly during Media3's command dispatch. Watch logcat for Media3
   warnings or an exception on the first hold. Mitigation if it bites: post the
   emission to the application handler.

## The device protocol (from the plan, task 1 and Verification)

Every precondition verified, never assumed. Pixel 10 Pro XL, **physical keys**
— `adb shell input keyevent 24/25` produces no `dispatchVolumeKeyEvent` and is
not a substitute.

1. `device-lock acquire --wait 300 volume-keys "device run PR #974"` and hold it
   across build, install, presses and log pull. `scripts/android-build.sh`
   calls `adb` without taking it.
2. Build for the device explicitly: `ANDROID_TARGET=aarch64-linux-android
   ANDROID_ABI=arm64-v8a scripts/android-build.sh`, then `:app:assembleDebug`
   — a plain `assembleDebug` links only host bindings and the APK dies on
   launch. Build from a checkout of `dev` at or after `772f8075`.
3. `adb logcat -G 16M`; capture **unfiltered** to a file, grep afterwards.
   `dispatchVolumeKeyEvent` and `Adjusting …` come from the system's
   media-session service, not our PID.
4. Known starting volume: `adb shell cmd audio set-volume 3 10`.
5. Start playback. **Confirm `state=PLAYING(3)` in `dumpsys media_session`**
   and that the session shows `volumeType=REMOTE` — that is item 2 above
   passing the first half.
6. Then, each with `PLAYING(3)` confirmed first:
   1. Screen off, hold VOLUME_UP → next track; VOLUME_DOWN → previous.
   2. Screen off, short press → one volume step, no skip.
   3. App in the foreground, hold → volume ramps, no skip.
   4. Another app in front, playing, hold → skip.
   5. Paused: both keys stock, including hold-to-ramp; `dumpsys` shows
      `volumeType=LOCAL` (item 2, second half).
   6. Volume panel slider drag → volume moves, no skip.
   7. After a hold-skip, `adb shell cmd audio get-volume 3` shows the volume
      from before the hold — not one or two steps off.
   8. The tick is felt on the skip and on nothing else.
   9. Switch off, playing, screen off: hold ramps, stock slider, no skip.
      Switch on again: back to 6.1.
   10. Last track, repeat off, screen off, hold up: `next()` no-ops; note
       whether tick + volume undo reads as broken.
7. Before releasing the lock: leave the phone with working volume keys
   (switch on, playback stopped → `LOCAL`), or reinstall the previous build.

Record per press the `increaseDeviceVolume`/`decreaseDeviceVolume` count at
the player and the intervals; the plan's constants are `repeatGapMaxMs = 100`
(measured repeat 48–50 ms) and `leadInMaxMs = 500` (measured lead-in
234–254 ms). If this device or its key-repeat settings differ, the failure
direction is "never skips", not "skips on taps" — still write it down.

## If the run fails

- Route dead (no callbacks at the player, or `DeviceInfo` never switches):
  the switch turns the session back to `LOCAL`; a one-line PR flipping the
  default to off is the safe holding position, the revert of #974 the full one.
- Volume keys dead while playing: that is a missed forwarding path in
  `CoreControlledPlayer.adjustDeviceVolume` — every branch must call
  `adjustStreamVolume` or `setStreamVolume` except `Swallow`. Force-stop the
  app to get the keys back, then fix forward.
- Skips on taps: raise nothing by guesswork; measure the tap gaps and the
  repeat gap first, then argue the constants from the numbers.

## Also open, not blocking

- The dev CI run behind the merge:
  https://github.com/marvinbaudach/reprise/actions/runs/35071734365 — probably
  cancelled by the next merge; the evidence is the next completed dev run
  containing `772f8075`. Red dev gates are often not this change; check the
  control arm.
- Survey result worth remembering when a user reports "it skips even with the
  switch off": LineageOS and other ROMs implement the same gesture at OS level
  and users blame the player (Auxio #1064). Details in the plan's survey
  section.
- Review nits left deliberately: `unwrap_or(true)` in
  `settings.rs` swallows DB errors exactly like its gapless twin; the switch
  has no GNOME counterpart (gapless has one).
