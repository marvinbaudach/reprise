# The phone's fog answers a kick, like the desktop bloom

Branch: `feature/android-desktop-visualizer` (this work sits on top of the
visualizer commits, so both merge together).

## Why

The desktop's now-playing bloom and the phone's now-playing fog are the same
idea — an out-of-focus copy of the cover lying behind it, breathing with the
music — but they are driven by different signals, and the phone's is the poorer
one.

`crates/reprise-gnome/src/ui/now_playing/cover_bloom.rs:36-57` reads **two**
values:

```
opacity = 0.06 + 0.15 * pressure + 0.16 * swell     // 0.06 … 0.37
scale   = 1.0  + 0.025 * swell
```

`pressure` is the fast one: the engine's glow layer, thrown to full by a kick
and released per tick (`crates/reprise-core/src/visuals/engine.rs:163-200`).
`swell` is the slow breath over the whole wave. Brightness carries both; the
**size follows the slow term alone**, so a kick lights the bloom without
pumping its geometry.

`android/app/src/main/java/de/reprise/spike/NowPlayingFog.kt:16-52` reads
**one**: `SceneState.fogLevel`, the mean of the *slow* follower bank over all 24
bands (`scene/SceneState.kt:163`). Both alpha and size hang off it. So the
phone's fog breathes, but it never answers a beat — and what size it has swings
on the slow value the desktop deliberately keeps size on.

Two things this plan does **not** change, because measurement already settled
them:

- The size swing stays at `SCALE_SWING = 0.14`, well above the desktop's 0.025.
  Against a dark cover the fog region sits at luma ≈ 8/255, where alpha alone
  moves the picture by a tenth of a stop; size is what the eye picks up. That
  was measured, not guessed.
- The normalisation window `0.05 … 0.70` stays for the slow term. `fogLevel`
  never reaches more than ~0.66 on real music (p5 0.131, median 0.600, p95
  0.644, max 0.659 over 4480 frames), so a formula mapping 0…1 throws away the
  top third.

## What to build

### 1. A fast bass reading in `SceneState`

`scene/SceneState.kt` already keeps a fast follower bank (`motionEnvelopes`,
`motionBands`) next to the slow one. Add a scalar `bassPressure` taken from the
**first seven** of those bands — the same span the shared core calls bass
(`crates/reprise-core/src/visuals/spectrogram_frame.rs:6-7`, ~20–139 Hz).

It must move at display rate, not at 20 Hz, so it is read the way the bands are
read: at the fraction of the way into the current frame, through the existing
`projectInto` path. Add `readBassPressureAt(frameFraction: Float)` next to
`motionBandsWithin`, sharing its projection buffer, and have `SceneDriver.tick`
call it once per tick with the fraction it already computed.

**Do not** let this touch the fog angles. `motionBandsWithin`'s doc comment
spells out why fog was left out of the fractional read: the angles *integrate*
whole frames, and reading between them would lend them a second step. A level
is not integrated, so reading it between frames is safe — the angles stay
exactly as they are.

`resetTo`, `wanderTo` and the paused path must all leave `bassPressure` in a
defined state (a seek snaps it, an unanalysed track leaves it at 0 and lets the
wander drive the fog as it does today).

### 2. The two-term response in `NowPlayingFogSpec`

Replace the single-input `response(fogLevel, floor)` with the desktop's shape,
in the desktop's proportions (0.16 swell : 0.15 pressure → 0.52 : 0.48):

```
drive    = 0.52 * normalised(swell) + 0.48 * normalisedPressure(pressure)
response = floor + (1 - floor) * drive.coerceIn(0, 1)
```

`wideAlpha`/`tightAlpha` keep their floors (0.34 / 0.14) and their opacity
factor. `breathingSize` keeps reading the **slow** term only.

`normalisedPressure` needs its own window, and it must be measured rather than
assumed: the bass bands through a fast follower do not have the same
distribution as the slow mean of all 24. Before fixing the constants, run the
production `SceneState` over the checked-in spectrogram fixture, print p5 /
median / p95 / max of `bassPressure`, and put those numbers in the commit
message next to the window you chose. If they land close to the fog's own
0.05 … 0.70, say so and reuse it.

### 3. Pass it through

`drawPlayedNowPlayingFog` (`NowPlayingScene.kt:286`) already receives the whole
`SceneState`, so nothing new crosses that boundary: read `state.bassPressure`
there and hand it to `drawNowPlayingFog` alongside `fogLevel`.

`NowPlayingSheet.kt` draws the same fog for the sheet — it must get the same
value, not a zero, or the two surfaces disagree about the same moment.

### 4. Tests

`android/app/src/test/java/de/reprise/spike/NowPlayingFogTest.kt` already pins
the response with measured values. Extend it, do not replace it:

- a kick at constant swell brightens both layers, and by more than the
  quantisation floor
- the same kick leaves `breathingSize` **unchanged** (this is the property that
  distinguishes this from the current behaviour)
- pressure and swell at full reach the same ceiling the current formula reaches
  at `fogLevel = 1` (the fog must not get brighter overall, only more articulate)
- out-of-range inputs clamp, never extrapolate

`SceneStateTest` gets the counterpart: `bassPressure` follows the fast bank on
the bass bands, a fractional read lands between two frames, a seek snaps, and a
paused position keeps its value.

## Decision to record, not to silently take

The desktop **pins** its bloom to rest while the Visual tab is open
(`cover_bloom.rs:217-226`): "two systems pulsing in different colours against
each other is the failure case". The phone is not in that position — its
visualizer lives inside the cover square while the fog is the atmosphere of the
whole screen, and both are drawn from the same artwork, so they do not fight
over colour. Keep the fog live while the visualizer runs. If it turns out to be
too busy on screen, damping belongs in a follow-up with a measurement behind it,
not in this branch.

## Verification

- `JAVA_HOME=/usr/lib/jvm/java-21-openjdk ./gradlew testDebugUnitTest` in
  `android/`, and count the XMLs under
  `android/app/build/test-results/testDebugUnitTest` — Gradle reports BUILD
  SUCCESSFUL without running a single test.
- On the emulator (`pixel10xl_api37`, host GPU): a bright cover
  ("Das Album") and a dark one, visualizer off, playing. Grab a burst of
  screenshots and compare the fog region's luma against the same track before
  the change. A dark cover will barely move — that is expected and measured;
  judge on the bright one.
