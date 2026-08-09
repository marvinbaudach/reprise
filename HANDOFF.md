# Handover — Now Playing scene, Wave D verification and the title scrim

Written 2026-08-09, 21:40. Branch `feature/now-playing-scene`, 17 commits ahead
of `origin/dev`, nothing behind. Worktree `/home/marvin/Projects/reprise-now-playing-scene`.

## What this session did

Ran the outstanding Wave D checks (3, 5, 7, 8) against a real emulator, then
fixed the one that failed.

Two commits are mine:

- `c001514c03` — `docs: record the Now Playing scene emulator verification`
- `5fa26bc73f` — `fix(android): keep the Now Playing title readable on a bright cover`

Everything is written up in `docs/plans/now-playing-scene-verification.md`;
that file is the real record, this is only the pointer.

## Results in one table

| check | state |
| --- | --- |
| 1 pause | closed — automated, and confirmed pixel-identical on the running app |
| 2 replay | closed — automated |
| 3 verse vs breakdown | Visualizer passes clearly (luma 67→108). **Played view does not** — 60.7 vs 61.8, a 2 % change against a 5.3× energy change. Needs an owner decision, see below |
| 4 greyscale cover | closed — automated |
| 5 very bright cover | **fixed** — title 2.32:1 → 9.24:1, artist line 1.28:1 → 3.82:1 |
| 6 no analysis | closed — automated |
| 7 rapid transition | functionally closed (6 round trips, no crash, no ANR, pause still works). Perceived smoothness needs real hardware |
| 8 both states | played view 434 frames/30 s at 150 ms, Visualizer 216 at 250 ms — the right direction, reproduced twice. Battery per hour needs real hardware |

## The one open decision

Check 3 in the **played view**. The cover fog moves with time but does not
answer the music: across a passage whose energy differs by 5.3× the mean luma
changes by 2 %. The brief says "if they look similar the scaling is too flat".
It may also be deliberate, because that view carries a cover, a title and an
artist line that must stay legible. Somebody has to say which. Nothing else in
the record is waiting on a person.

## Two findings worth acting on

**The Android unit suite is falsely red under the machine's default JDK.**
`java -version` is 26.0.2; Robolectric 4.16.1 cannot instrument class files of
major version 70 and throws `IllegalArgumentException: Unsupported class file
major version 70` out of `Shadows.reset` — after the assertions have already
passed. Verified on an unmodified checkout, so it is the toolchain. Always run:

```bash
JAVA_HOME=/usr/lib/jvm/java-21-openjdk ANDROID_HOME=/home/marvin/.local/share/android-sdk ./gradlew testDebugUnitTest
```

**The cover fog shows the straight edges of its own texture.** `CoverFogBitmap`
box-blurs the artwork into a fixed 256 px square, so the square's border
survives the blur; scaled up and rotated it reads as long straight seams
sweeping across the played view. Pure CPU arithmetic, identical on any device.
A transparent margin around the source square, or a radial alpha falloff before
the blur, would remove it. Not fixed, not filed.

## How to reproduce the emulator run

The AVD is `pixel10xl_api37`. Everything below is what it took; none of it is
in a script yet.

```bash
export ANDROID_HOME=/home/marvin/.local/share/android-sdk
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
emulator -avd pixel10xl_api37 -no-window -no-boot-anim -no-snapshot-save \
         -gpu swiftshader_indirect -accel auto
scripts/android-build.sh && (cd android && ./gradlew assembleDebug)
adb install -r -g android/app/build/outputs/apk/debug/app-debug.apk
```

The three fixture tracks are already on the AVD under `/sdcard/Music/Reprise`,
and the app already holds the SAF grant for that folder. Each track sits beside
a `.reprise-analysis` sidecar encoded from the desktop database — the format is
`RPA-SIDE`, a `u16` version, the source fingerprint, two `u32` lengths, then the
spectrogram cells and the waveform peaks. The phone re-keys the data onto its
own fingerprint on import, so the sidecar's own mtime and size do not have to
match the pushed file.

Screenshots, gfxinfo dumps and the before/after crops are in
`~/Downloads/reprise/now-playing-emulator-2026-08-09/`.

## State of the tree

`android/app/build.gradle.kts` is modified and uncommitted — not mine; another
session was working in this worktree at 21:30 and its work has since been
merged. My scrim survived that merge, refactored into a cached
`titleScrimBrush` in `NowPlayingFog.kt`; the constants are unchanged.

A full `testDebugUnitTest` under JDK 21 ran at 21:41: **49 suites, 224 tests,
3 failures**, all three in `PlaybackServiceLifetimeTest` and all three the same
error — `NoClassDefFoundError: Could not initialize class
uniffi.reprise_android_ffi.UniffiLib`. That is the JVM-host UniFFI library
failing to load in a unit test, not a scene failure; nothing in the scrim
commit touches that class or the FFI. It is very probably the host-target `.so`
missing, since `scripts/android-build.sh` builds the Android ABI and not the
host one. **Not proven pre-existing in this session** — confirm it on plain
`origin/dev` before treating it as unrelated, and build the host library if it
is not.
