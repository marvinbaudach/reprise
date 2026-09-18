# Device run 2026-09-18 — the rock gesture on the volume keys

Plan: `docs/plans/volume-key-taps-skip-the-track.md`, § Verification
(decision 9, binding). Branch `feature/volume-key-taps-skip-the-track`, HEAD
`de2ccc4bba`. Build: `ANDROID_HOME=~/.local/share/android-sdk
ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a
scripts/android-build.sh` + `:app:assembleDebug` (0.1.143, debug — the
`VolumeKeys` line is `BuildConfig.DEBUG` only). Device: Pixel 10 Pro XL,
physical keys pressed by the user; the agent held `device-lock` (re-acquired
at every hand-off), read `dumpsys media_session` and `cmd audio get-volume 3`
before each step, and pulled the unfiltered logcat (`adb logcat -G 16M`).

Gap columns: `DOWN→DOWN` is taken from the system's own
`dispatchVolumeKeyEvent … ACTION_DOWN` timestamps (`MediaSessionService`,
`pkg=android` with the screen off); `gapMs` is the player's own number from
the `VolumeKeys` line (gap since the previous callback of any kind) and serves
as the cross-check.

## Result

_(filled in as the run proceeds)_

## Per-step table

| Step | Result |
| --- | --- |

## Calibration (decision 6)

## Logcat excerpts
