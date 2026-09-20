# Device run 2026-09-18/19 — the rock gesture on the volume keys

Plan: `docs/plans/volume-key-taps-skip-the-track.md`, § Verification
(decision 9, binding). Branch `feature/volume-key-taps-skip-the-track`, HEAD
`e31ae18448` (rebased onto dev `7e545989ec` for the run). Build:
`ANDROID_HOME=~/.local/share/android-sdk ANDROID_TARGET=aarch64-linux-android
ANDROID_ABI=arm64-v8a scripts/android-build.sh` + `:app:assembleDebug`
(0.1.146, debug — the `VolumeKeys` line is `BuildConfig.DEBUG` only). Device:
Pixel 10 Pro XL, physical keys pressed by the user; the agent held
`device-lock` (re-acquired at every hand-off), read `dumpsys media_session`
and `cmd audio get-stream-volume 3` before each step, and pulled the
unfiltered logcat (`adb logcat -G 16M`).

Raw evidence, all under `target/` and untracked (`.gitignore` swallows
`*.log`, so these are `.txt`):

| file | window | size |
| --- | --- | --- |
| `volume-keys-device-run-logcat-2026-09-19.txt` | 09-19 09:58 – 13:34 (steps 1–6) | 25 MB |
| `volume-keys-device-run-logcat-2026-09-19b.txt` | 09-19 16:43 – 19:31 (step 9, first step-10 attempts) | 13 MB |
| `volume-keys-device-run-logcat-2026-09-19c.txt` | 09-19 19:33 – 22:35 (step 10) | 29 MB |
| `volume-keys-device-run-logcat-2026-09-20.txt` | 09-20 13:5x – 14:00 (step 12) | 105 MB |

`adb logcat` replays the kernel ring buffer when it starts, so `…-19c.txt`
repeats `…-19b.txt`'s three skips; the counts below are taken over the union,
not the sum. Across the run: **26 `Skip(…)` decisions, 25 of them from a human
press** (the 26th is the synthetic diagnostic below), and 425 `VolumeKeys`
lines in total.

Gap columns: `DOWN→DOWN` is taken from the system's own
`dispatchVolumeKeyEvent … ACTION_DOWN` timestamps (`MediaSessionService`,
`pkg=android` with the screen off); `gapMs` is the player's own number from
the `VolumeKeys` line (gap since the previous callback of any kind) and serves
as the cross-check. Both agree to within ~12 ms throughout.

## Deviations from the protocol, stated up front

- **Start level 3–4 instead of 10.** The run happened at night and had to stay
  inaudible on the speaker; the user set 3 before step 1 and the agent kept
  the level between 3 and 9 (25 and 0 only for the rail step). Nothing in the
  protocol depends on the absolute level — the restore assertions compare
  before/after, and the rails are tested explicitly in 5a.
- **One synthetic pair, as a diagnosis, not as evidence.** After three rounds
  of human presses arrived as single taps seconds apart, the agent injected
  one `adb shell input keyevent KEYCODE_VOLUME_UP KEYCODE_VOLUME_DOWN` pair
  (45 ms) to establish that the route and the decision logic worked before
  re-instructing the gesture: `Skip(direction=UP, restoreVolume=4)`. It is
  excluded from every count below.
- **Steps 9–12 ran on a second install of the same branch build.** While the
  phone was idle, another session installed a dev build (0.1.152) over the
  run's build, which does not contain the gesture at all. The branch was
  rebuilt from the same HEAD and reinstalled with a locally bumped
  `versionCode` (153, `versionName 0.1.146-rock-devicerun`, **uncommitted**
  and reverted afterwards) so the install could go over 152 without losing the
  app's data. A rebase onto the newer dev was deliberately *not* done mid-run:
  it conflicts in four Kotlin files (`MainActivity`, `LibraryScreen`,
  `BrowseScreen`, `ReprisePlaybackService`) and that is landing work, not
  measurement work.
- **Three capture gaps.** The `adb logcat` capture died twice (once when the
  session's scratchpad was recycled, once unexplained) and Reprise's media
  session was gone for a while after step 10 ended the queue. Presses made in
  those windows produced no evidence and were repeated; the evidence files are
  `target/volume-keys-device-run-logcat-2026-09-19.txt`, `…-19b.txt` and
  `…-20.txt`.

## Result

**The gesture works on the device, in every state the plan asks about, and
the false-positive arm passes with a wide margin.** All twelve protocol steps
ran. Two findings, neither about the gesture's decision logic:

1. **The haptic tick (decision 8) never fired** — no tick was felt on any of
   the 25 skips, although the code path runs and the phone's
   `haptic_feedback_enabled` is 1. Decision 8 is therefore **unmet**, and
   since decision 9 makes this run the landing gate, the run is not a clean
   pass.
2. **At the end of the queue the skip ends playback** instead of no-opping:
   `next()` on a one-item queue stopped the music, emptied the queue
   (`queueTitle size=0`) and tore the media session down. The plan's step 10
   expected a no-op. The user's verdict, asked for by the plan: **acceptable**
   — so this is recorded, not fixed.

Calibration verdict (decision 6): **`ROCK_MAX_MS = 500` stands.** The
deliberate rock measured 11–588 ms DOWN-to-DOWN (5 of 7 under 320 ms); the
fastest *natural* correction measured **1259 ms**. The criterion — fastest
natural correction exceeds `ROCK_MAX_MS` by at least 150 ms — is met with
759 ms to spare. A 700 ms window would still have 559 ms of margin, so the
constant is not on an edge; it is not changed.

## Per-step table

| Step | Result |
| --- | --- |
| 1, screen off, up→down = next | ✅ 7 deliberate rocks, 5 skips: DOWN→DOWN 221 / 311 / 292 / 246 / 241 ms → `Skip(direction=UP, restoreVolume=4)`, level back to 4 each time, position back to ~0. The two misses were 526 and 582 ms — outside the window, decided as two steps, net level unchanged. |
| 1, screen off, down→up = previous | ✅ 6 rocks, 5 skips: 385 / 292 / 217 / 246 / 235 ms → `Skip(direction=DOWN, restoreVolume=4)`. The miss was 781 ms → two steps. |
| 2, uncoached corrections | ✅ **0 skips in 172 key events.** 10 corrections, up→down gaps 11488 / 1530 / 1522 / 1511 / 3163 / 1584 / 1297 / 1259 / 1489 / 1641 ms. Fastest 1259 ms. |
| 2, control: single taps | ✅ one step per tap, no skip; up, pause 3.1 s, down → two steps, no skip. |
| 3, Reprise in front (screen on) | ✅ `topResumedActivity=…/.MainActivity`; rocks of 57 ms and 40 ms → step + step, **no skip**; holding up ramped 6 → 15 through repeats, no skip (21 events, 0 skips). |
| 4, another app in front (Telegram), rock | ✅ 13 ms → `Skip(direction=UP, restoreVolume=7)`. |
| 4, hold in another app | ✅ ramp 7 → 15, no skip; a later hold ramped 25 → 8 and 0 → 6, no skip. |
| 4, tap up then hold down (decision 5) | ✅ 689 ms attempt → steps only; 489 ms attempt → `Skip(direction=UP, restoreVolume=6)`, then the hold's repeats ramped 6 → 0 in ~500 ms. Reads as "track changes, then the volume falls" — the stock behaviour of a hold, as accepted. |
| 5, paused | ✅ `state=PAUSED(2)`, `volumeType=LOCAL`; four taps after the pause produced **no** `VolumeKeys` line at all — stock path. |
| 5a, rail at 0 | ✅ three rocks (206 / 51 / 54 ms) → `Skip(direction=DOWN, restoreVolume=0)`, `get-stream-volume 3` stays 0. |
| 5a, rail at 25 | ✅ rock 11 ms → `Skip(direction=UP, restoreVolume=25)`, level stays 25. |
| 5b, lock screen with the screen on | ✅ `mDreamingLockscreen=true`, `mWakefulness=Awake`; rock 11 ms → `Skip(direction=UP, restoreVolume=4)` — decision 2's claim holds. |
| 6, slider drag | ✅ the level moved 25 → 9 with **no** volume-key event and no skip (54 `setVolumeTo`/`VolumeProvider` lines in that window); the session's slider is the one SystemUI draws while Reprise plays, as decision 7 accepts. |
| 7, level after every skip | ✅ every skip carried `restoreVolume` equal to the level before the first tap, and `get-stream-volume 3` agreed after each one (4, 0, 25, 6, 7). |
| 8, tick on the skip only | ❌ **no vibration was felt on any skip** (user, unprompted, after step 6). See the finding below. |
| 11, REMOTE across a skip | ◐ `dumpsys media_session` showed `volumeType=REMOTE` on every probe while playing — including immediately after skips — and `LOCAL` only when paused. No `LOCAL` was ever observed while `playWhenReady`, but the run sampled the value rather than watching it continuously, so this is consistent with decision 3, not a proof of it. |
| 9, switch off → stock | ✅ with the setting off and the track still playing, `volumeType=LOCAL` and a 211 ms rock produced **no** `VolumeKeys` line at all — the callbacks are not merely ignored, the route is not taken. |
| 9, switch on again | ✅ `volumeType=REMOTE` again; rock 191 ms → `Skip(direction=UP, restoreVolume=2)`. |
| 10, end of the queue (queue size 1, repeat off) | ◐ two rocks (292 / 216 ms) → `Skip(direction=UP, restoreVolume=1)`, level restored to 1 — but playback **stopped**: queue empty, session gone. Not the expected no-op. Two earlier attempts on what the user took to be the last track of a longer queue each skipped to a real next track (titles read from `dumpsys`: *Downshift — Cogitations* → *All Hail the Fallen King — Chelsea Grin*), so the end-of-queue case is only reachable with a one-item queue. User's verdict on how it reads: **acceptable**. |
| 12, leave the phone with working keys | ✅ playback stopped → no session, `volumeType=LOCAL`; an injected up/down pair moved the system volume and produced no `VolumeKeys` line. Level left at 8. |

## Calibration (decision 6)

| | fastest | slowest | n |
| --- | --- | --- | --- |
| deliberate rock (steps 1, 3–5b, 9, 10) | 11 ms | 781 ms | 31 |
| deliberate rock that skipped | 11 ms | 489 ms | 25 |
| natural correction (step 2, uncoached) | **1259 ms** | 11488 ms | 10 |

The 25 gaps that skipped, sorted: 11, 11, 13, 30, 45, 51, 53, 54, 191, 206,
215, 216, 217, 235, 241, 243, 246, 252, 252, 292, 292, 292, 311, 321, 385,
489 ms (26 values including the synthetic pair at 45 ms). The two populations
do not overlap and are 770 ms apart at their closest (489 ms vs 1259 ms).
`ROCK_MAX_MS = 500` admitted every deliberate rock except four the user
himself timed above the window (522, 588, 689, 781 ms), and admitted no
correction. **Verdict: keep 500.**

## Finding — the haptic tick never fired (decision 8)

Reported by the user after step 6, unprompted: "keine vibration dagewesen" —
no tick on any of the 25 skips, in any state.

What is known:

- The code path runs: `CoreControlledPlayer` calls `commands.hapticTick()` in
  the `is RemoteVolumeAction.Skip` branch (after the restore and the
  `next()`/`previousInQueueOrder()`), and `ReprisePlaybackService.hapticTick()`
  resolves `VibratorManager.defaultVibrator` and plays
  `VibrationEffect.createPredefined(VibrationEffect.EFFECT_TICK)`.
  `android.permission.VIBRATE` is in the manifest.
- The phone allows haptics: `settings get system haptic_feedback_enabled` = 1.
- The unfiltered logcat carries **no** `VibratorManagerService` or
  `VibrationThread` line at all (216 `Vibrator: waitForComplete: Vibrator is
  already off` lines from the HAL, none tied to Reprise's uid 10285), so the
  log neither confirms nor refutes the call reaching the service.

Candidates, none verified: `EFFECT_TICK` is the weakest predefined effect and
may be below the threshold of perception on this device while the phone lies
on a table; a vibration requested by a background service without an
attribution to a foreground use case may be dropped silently on Android 17; or
`hapticTick()` throws and is swallowed. This is the one part of decision 8
that the run could not confirm, and it is the reason the run is **not** a
clean pass. It does not block the gesture itself — the skip, the restore and
the guard all work — but decision 8 asked for the tick.

## Logcat excerpts

Step 1, the five up→down rocks (the lines the table's first row summarises):

```
11:05:32.689 direction=UP   gapMs=3047006 volume=4 action=Step(direction=UP)
11:05:32.905 direction=DOWN gapMs=215     volume=5 action=Skip(direction=UP, restoreVolume=4)
11:24:43.038 direction=UP   gapMs=2368    volume=4 action=Step(direction=UP)
11:24:43.349 direction=DOWN gapMs=311     volume=5 action=Skip(direction=UP, restoreVolume=4)
11:24:46.831 direction=UP   gapMs=1594    volume=4 action=Step(direction=UP)
11:24:47.123 direction=DOWN gapMs=292     volume=5 action=Skip(direction=UP, restoreVolume=4)
11:24:48.371 direction=UP   gapMs=1249    volume=4 action=Step(direction=UP)
11:24:48.614 direction=DOWN gapMs=243     volume=5 action=Skip(direction=UP, restoreVolume=4)
11:24:50.099 direction=UP   gapMs=1485    volume=4 action=Step(direction=UP)
11:24:50.340 direction=DOWN gapMs=241     volume=5 action=Skip(direction=UP, restoreVolume=4)
```

The two near misses in the same series — the window doing its job:

```
11:24:40.150 direction=UP   gapMs=39014 volume=4 action=Step(direction=UP)
11:24:40.671 direction=DOWN gapMs=522   volume=5 action=Step(direction=DOWN)
11:24:44.649 direction=UP   gapMs=1299  volume=4 action=Step(direction=UP)
11:24:45.236 direction=DOWN gapMs=588   volume=5 action=Step(direction=DOWN)
```

Step 1, down→up = previous:

```
11:29:xx DOWN gap=971  vol=4 Step(direction=DOWN)
11:29:xx UP   gap=385  vol=3 Skip(direction=DOWN, restoreVolume=4)
11:29:xx DOWN gap=983  vol=4 Step(direction=DOWN)
11:29:xx UP   gap=292  vol=3 Skip(direction=DOWN, restoreVolume=4)
11:29:xx DOWN gap=1278 vol=4 Step(direction=DOWN)
11:29:xx UP   gap=217  vol=3 Skip(direction=DOWN, restoreVolume=4)
11:29:xx DOWN gap=1187 vol=4 Step(direction=DOWN)
11:29:xx UP   gap=246  vol=3 Skip(direction=DOWN, restoreVolume=4)
11:29:xx DOWN gap=1415 vol=4 Step(direction=DOWN)
11:29:xx UP   gap=235  vol=3 Skip(direction=DOWN, restoreVolume=4)
```

Step 2, the ten uncoached corrections — every up→down transition, all `Step`:

```
11:32:41.784 +11488 ms   11:33:55.234 +3163 ms
11:32:53.549  +1530 ms   11:34:04.549 +1584 ms
11:33:01.780  +1522 ms   11:34:11.070 +1297 ms
11:33:11.866  +1511 ms   11:34:18.659 +1259 ms
                         11:34:27.291 +1489 ms
                         11:34:36.852 +1641 ms
```

Step 3, Reprise in front — the rock is decided as two steps:

```
11:53:36.834 UP   gap=899091 vol=6 Step(direction=UP)
11:53:36.891 DOWN gap=57     vol=7 Step(direction=DOWN)
11:53:38.357 UP   gap=1466   vol=6 Step(direction=UP)
11:53:38.397 DOWN gap=40     vol=7 Step(direction=DOWN)
```

Step 4c, tap up then hold down — skip, then the hold's ramp:

```
11:58:29.215 DOWN gap=489 vol=7 Skip(direction=UP, restoreVolume=6)
11:58:29.462 DOWN gap=248 vol=6 Step(direction=DOWN)
11:58:29.509 DOWN gap=47  vol=5 Step(direction=DOWN)
11:58:29.572 DOWN gap=63  vol=4 Step(direction=DOWN)
11:58:29.612 DOWN gap=40  vol=3 Step(direction=DOWN)
11:58:29.661 DOWN gap=49  vol=2 Step(direction=DOWN)
11:58:29.733 DOWN gap=72  vol=1 Step(direction=DOWN)
```

Step 5a, both rails — the skip happens and the level does not move:

```
12:03:27.151 DOWN gap=266253 vol=0  Step(direction=DOWN)
12:03:27.353 UP   gap=206    vol=0  Skip(direction=DOWN, restoreVolume=0)
12:05:25.725 UP   gap=85049  vol=25 Step(direction=UP)
12:05:25.734 DOWN gap=11     vol=25 Skip(direction=UP, restoreVolume=25)
```

Step 5b, lock screen with the screen on:

```
12:06:03.478 UP   gap=27370 vol=4 Step(direction=UP)
12:06:03.489 DOWN gap=11    vol=5 Skip(direction=UP, restoreVolume=4)
```

Step 9, the setting off — the rock reaches the system and not the player
(`dispatchVolumeKeyEvent` pairs at 18:23:59.471 / 18:23:59.682, 211 ms apart,
with zero `VolumeKeys` lines in the whole capture), then on again:

```
18:29:40.391 direction=UP   gapMs=none volume=2 action=Step(direction=UP)
18:29:40.582 direction=DOWN gapMs=191  volume=3 action=Skip(direction=UP, restoreVolume=2)
```

Step 10, the one-item queue — the skip lands, then playback ends:

```
20:29:20.809 direction=UP   gapMs=none volume=1 action=Step(direction=UP)
20:29:21.101 direction=DOWN gapMs=292  volume=2 action=Skip(direction=UP, restoreVolume=1)
20:31:59.195 direction=UP   gapMs=none volume=1 action=Step(direction=UP)
20:31:59.412 direction=DOWN gapMs=216  volume=2 action=Skip(direction=UP, restoreVolume=1)
```

`dumpsys media_session` before: `PLAYING(3)`, `volumeType=REMOTE`,
`queueTitle=null, size=1`, `description=Burn Me Alive, Gone Cold, A Sign of
Life`. After: `queueTitle=null, size=0` and `Sessions Stack - have 0
sessions`.

## What this run does not prove

- **Decision 3 is sampled, not watched.** `volumeType` was read before and
  after each step and was always `REMOTE` while playing and `LOCAL` while
  paused, but no continuous observation ran across a track change, so a brief
  `LOCAL` window during the skip's BUFFERING would not have been seen. The
  Robolectric test covers the transition; the device covers the endpoints.
- **The tick's cause.** The run establishes that no tick is felt, not why.
- **Nothing about a second device.** Every number here is a Pixel 10 Pro XL.
