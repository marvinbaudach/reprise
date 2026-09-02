# Volume keys switch tracks (Android)

Date: 2026-09-01
Status: design approved, ready for planning

## Goal

Holding a hardware volume key in the Reprise Android app moves to the next or
previous track. A short press keeps changing the volume exactly as before.


> **Outcome 2026-09-02:** built and shipped as #806, then reverted. The scope
> below — in-app only — is precisely what the owner did not want. The successor
> takes the **Media3 remote volume** route rejected in this document; the
> rejection reasoning stays accurate and is now a cost to design around rather
> than a reason to stop.

## Scope

In-app only: the gesture is active while a Reprise activity is in the
foreground. It deliberately does **not** work with the screen off or from other
apps.

Two routes to a system-wide gesture were considered and rejected:

- **Media3 remote volume** (report `DeviceInfo` as `PLAYBACK_TYPE_REMOTE` and
  map `increaseDeviceVolume()` to next). `VolumeProvider` delivers a bare
  direction delta with no key-up event, so a long press cannot be told from a
  short one. Synthesising it means either a ~450 ms delay on every short press
  or a visible volume jump that gets reverted, and the system panel renders a
  remote slider while active. Rejected.
- **AccessibilityService** with `FLAG_REQUEST_FILTER_KEY_EVENTS` — the only API
  that yields real key-down/key-up globally. Rejected for the scary permission
  prompt and the Play Store policy risk it would carry.

There is no settings toggle. With the short press left intact the gesture is
non-invasive, and the app has no playback settings page to hang a switch on.

## Behaviour

| Event | Condition | Result |
| --- | --- | --- |
| `onKeyDown`, `repeatCount == 0` | a track is loaded | `event.startTracking()`, event consumed |
| `onKeyDown`, `repeatCount > 0` | a track is loaded | consumed, ignored (this is what removes hold-to-ramp) |
| `onKeyLongPress` VOLUME_UP | a track is loaded | `playbackControls.next()` |
| `onKeyLongPress` VOLUME_DOWN | a track is loaded | `playbackControls.previous()` |
| `onKeyUp`, `!event.isCanceled` | a track is loaded | `AudioManager.adjustStreamVolume(STREAM_MUSIC, dir, FLAG_SHOW_UI)` |
| `onKeyUp`, `event.isCanceled` | a track is loaded | ignored — the long press already fired |
| any | no track loaded | not consumed, default system handling |

Android does the long-press timing: `startTracking()` on the down event makes
the framework call `onKeyLongPress` after the `ViewConfiguration` long-press
timeout, and flags the following up event as cancelled.

**Accepted cost:** holding a volume key no longer ramps the volume inside the
app. One step per press. This is the price of consuming the repeat stream and
is not worked around.

**Gate:** interception requires `playbackState.ready && currentTrackId != null`
(`MainActivity.kt:178`, `PlaybackUiState.kt:37`). It is deliberately *not* tied
to `isPlaying` — skipping while paused is useful. With no track loaded there is
nothing to skip to, so the keys keep their normal behaviour including ramping.

## Structure

New file `android/app/src/main/java/de/reprise/spike/playback/VolumeKeyTrackSwitch.kt`
holding an activity-free class. It maps (key code, phase, may-intercept) to a
`VolumeKeyAction`:

- `SkipNext`
- `SkipPrevious`
- `AdjustVolume(direction)`
- `Ignore` — consumed, nothing to do
- `Passthrough` — not consumed, let the system handle it

`MainActivity` (`MainActivity.kt:62`, `ComponentActivity`) gains
`onKeyDown`/`onKeyLongPress`/`onKeyUp` overrides that translate the `KeyEvent`
into a call on that class and execute the returned action against the existing
`playbackControls` field (`ActivityPlaybackControls`, `MainActivity.kt:160`)
and an `AudioManager`. The activity holds no key state of its own.

`de/reprise/spike/playback/` is a new package; the module currently has 75+
files directly in `de/reprise/spike/`.

## Testing

Plain JVM unit tests for `VolumeKeyTrackSwitch`, covering:

- short press up/down → `AdjustVolume` with the right direction
- long press up → `SkipNext`, long press down → `SkipPrevious`
- the up event following a long press → `Ignore`
- no track loaded → `Passthrough` in every phase

One Robolectric test alongside `MainActivityDockTest.kt`
(`createAndroidComposeRule<MainActivity>()`, Robolectric 4.16.1 per
`build.gradle.kts:147`) that dispatches a synthetic long press through
`activity.dispatchKeyEvent(...)` and asserts the track changed.

**Final verification is manual, on a device** (scrcpy / mobile-mcp). The module
has no instrumented tests, and a synthetic key event under Robolectric does not
prove the real framework delivers volume keys to `Activity.onKeyDown` on the
target device.
