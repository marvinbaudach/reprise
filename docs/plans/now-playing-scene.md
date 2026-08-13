---
slug: now-playing-scene
worktree: ~/Projects/reprise-now-playing-scene
branch: feature/now-playing-scene
phase: planned
codex_session:
created: 2026-08-09
---
# Now Playing: the played view and the fullscreen visualizer

Status: **planned — not started**
Design: Claude Design project `Reprise Mobile.dc.html`, frames **27b** (played
view), **28a** (fullscreen burst), **23b** (header row with the toggle).
Base: `origin/dev` at `7214a29de1` (#379), rebased 2026-08-09 after the pending
Android work landed. Every file reference below was re-checked against that
commit and holds.
Target branch: `feature/now-playing-scene`
As of: 2026-08-09

## Product goal

The Now Playing screen has exactly two states and one icon between them.

- **Played view** (default) — sharp cover, fog behind it made from the cover
  itself, title, artist, progress, transport. This is what a tapped track
  opens.
- **Fullscreen visualizer** — wedges, corona, core, hot ray, bloom. Reached
  from the fullscreen icon in the header, left again from the bottom-right
  icon.

Both states draw into **one** Compose `Canvas`, and both are driven by the same
data: the 24-band spectrogram the desktop computed and sync brought across.

## The one rule

**Every motion needs a cause in the signal.**

The implementation consequence, and the thing every reviewer should check
first: the scene advances by **spectrogram frames consumed**, never by wall
clock. There is no `animation-duration`, no `infiniteRepeatable`, no
`sin(systemTime)` anywhere in the scene. Rotation is not forbidden — *clock-driven*
rotation is. Everywhere an angle is needed:

```
angle += energy * factor      // once per spectrogram frame
```

so the angle is the integral of the energy. Loud passages turn faster, quiet
ones barely, pause stands still.

The invariant that makes this testable and makes replays identical:

> Every spectrogram frame between the last processed index and the current one
> is stepped **exactly once, in order** — regardless of how often the UI
> actually redraws.

The wall clock may be used to *estimate the current audio position* between
playback ticks. It may never be used to decide how far the scene moved.

## Decisions taken with the user (2026-08-09)

| # | Decision |
|---|----------|
| D1 | The header keeps queue, sleep timer and heart; the fullscreen icon joins them on the right. Frame 27b shows only two icons; we do not drop three working functions for it. |
| D2 | The visualizer *modes* (Cover / Ambient), the mode bar and the long-press menu **retire**. The played view is cover + fog, the fullscreen state is the burst. One toggle, two states. |
| D3 | Dock mode keeps its palette-derived ambient fields unchanged. Palette extraction therefore stays in the tree; it only leaves Now Playing. |

## Data path

The data already exists end to end; only the last hop is missing.

- `reprise-core/src/spectrogram.rs` — `SPECTROGRAM_BAND_COUNT = 24`,
  `SPECTROGRAM_FRAME_RATE_HZ = 20`, `SPECTROGRAM_LOW_HZ = 20`,
  `SPECTROGRAM_HIGH_HZ = 16_000`, one `u8` per cell, frame-major.
- `reprise-android-ffi/src/mobile_sync.rs` — sync writes the spectrogram into
  the phone database (`import_track_analysis`).
- `reprise-android-ffi/src/track_analysis.rs` — today only
  `track_render_bars()` (the seek bar). **The raw frames are not reachable from
  Kotlin.** That is package P1.

**No analysis on the phone.** No `Visualizer` API, no audio tap, no microphone.
The screen draws numbers that are already there.

**Missing data is not an error.** A track without analysis shows the resting
state and starts playing immediately. Nothing ever waits for data.

---

# Packages

Waves are parallel inside, sequential between. File ownership is exclusive —
no two packages in the same wave touch the same file. Ownership belongs in
`AGENTS.md` on the branch as well as here.

## Wave A — foundations (P1, P2, P7 in parallel)

### P1 — raw spectrogram frames reach Kotlin

Owns: `crates/reprise-android-ffi/src/track_analysis.rs`

Add beside `AndroidTrackRenderBar`:

```rust
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AndroidTrackSpectrogram {
    pub band_count: u32,
    pub frame_rate_hz: u32,
    /// Frame-major cells, `band_count` bytes per frame.
    pub cells: Vec<u8>,
}
```

and on `MusicLibrary`:

```rust
pub fn track_spectrogram(&self, track_id: i64)
    -> Result<Option<AndroidTrackSpectrogram>, LibraryError>
```

- `Ok(None)` is the ordinary no-analysis answer, same as `track_render_bars`.
- The whole track is returned in one call. A four-minute track is
  240 s × 20 fps × 24 B ≈ **115 KB** — cheaper than one artwork decode, and it
  removes any per-frame FFI traffic from the render loop. Do not paginate.
- `band_count` and `frame_rate_hz` are carried in the record rather than
  hard-coded in Kotlin: the format version belongs to Rust.

Tests (Rust, in the existing `mod tests` of that file):
1. A track with a stored spectrogram returns the exact stored cells, and
   `cells.len() % band_count == 0`.
2. A track without analysis returns `Ok(None)`.
3. An unknown id returns `LibraryError::TrackNotFound`.

### P2 — the scene mathematics (pure JVM, no Android imports)

Owns: `android/app/src/main/java/de/reprise/spike/scene/` (new package)
and the matching tests under `android/app/src/test/java/de/reprise/spike/scene/`.

**This package must not import anything from `android.*` or
`androidx.compose.*`.** It is plain Kotlin so the whole rule set is testable on
the JVM without Robolectric, and so the same maths can move to Rust later if
another frontend needs it.

Files:

- `SpectrogramFrames.kt` — `bandCount`, `frameRateHz`, `frameCount`,
  `frameIndexFor(positionMs)`, `band(frameIndex, band): Int`. Out-of-range
  reads clamp; an empty spectrogram reports `frameCount == 0` and yields zeros.
- `BandEnvelopes.kt` — one one-pole follower per band.

  ```
  attackCoef = 1 - exp(-frameMs / attackMs)
  decayCoef  = 1 - exp(-frameMs / decayMs)
  value += (target - value) * (if (target > value) attackCoef else decayCoef)
  ```

  Two configurations: **fog** attack 200 ms / decay 1200 ms, **burst**
  attack 40 ms / decay 220 ms. At 20 fps (`frameMs = 50`) that is
  0.221/0.041 and 0.713/0.204.
- `Lookahead.kt` — the target fed to the envelope is
  `max(cell[i], cell[i + LOOKAHEAD_FRAMES])` with `LOOKAHEAD_FRAMES = 8`
  (400 ms). Rises only; a fall never reaches back.
- `EnergyIntegrator.kt` — `angle = wrap360(angle + energy * factor)`, stepped
  once per spectrogram frame. Used for both fog layers and anywhere else an
  angle is wanted.
- `CoreShape.kt` — three superimposed sine components over the angle, derived
  **once per track** from a stable hash (FNV-1a over `title + "\0" + artist`):
  three integer harmonics in 3..9, three phases, amplitudes 0.06 / 0.04 / 0.03.
  `radiusAt(theta, baseRadius, bass)`. Same track ⇒ same shape, always.
- `SceneColour.kt` — `hue = (250 + angleDegClockwiseFromTop) mod 360`,
  saturation 0.95, lightness `0.30 + energy * 0.26`. **The hue never reacts to
  the music.** Angle → colour is fixed for the whole song.
- `SceneState.kt` — the whole per-frame state and the only entry point:

  ```kotlin
  class SceneState(frames: SpectrogramFrames, shape: CoreShape) {
      fun advanceTo(frameIndex: Int)   // steps every intervening frame, in order
      fun resetTo(frameIndex: Int)     // seek: adopt raw values, no stepping
      val fogBands: FloatArray         // 24, 0..1
      val burstBands: FloatArray       // 24, 0..1
      val level: Float                 // mean of burst bands
      val bass: Float                  // mean of bands 0..3
      val transient: Transient?        // band index + excess, above threshold only
      val fogAngleA: Float             // += level * 0.9 per frame
      val fogAngleB: Float             // -= level * 0.6 per frame
      val revision: Int                // bumped on any change; the draw gate
  }
  ```

  - `advanceTo` with an index **equal to** the last one is a no-op and does not
    bump `revision`. This is what makes pause stand still.
  - `advanceTo` with a backwards or far-forward index (> 1 s, i.e. 20 frames)
    delegates to `resetTo` — a seek is not twenty frames of music.
  - **Transient**: for each band, `raw[i] - fogBands[i]`; the largest positive
    excess above `TRANSIENT_THRESHOLD = 0.18` wins. Below the threshold there
    is no hot ray at all. Exactly one, never several.

Tests (`SceneStateTest`, `BandEnvelopeTest`, `CoreShapeTest`, `SceneColourTest`):

1. **Replay determinism** — two fresh `SceneState`s stepped over the same frame
   sequence, one frame at a time versus in irregular jumps of 1–4 frames, end
   in bit-identical `fogAngleA/B` and band arrays.
2. **Pause stands still** — `advanceTo(n)` repeated ten times leaves every
   value and `revision` unchanged. Not slower — unchanged.
3. **Attack/decay constants** — a step from 0 to 1 reaches 0.63 after
   `attackMs`, and a step from 1 to 0 reaches 0.37 after `decayMs`, ±2 %.
4. **Lookahead lifts only** — a rise 400 ms ahead is already visible now; a
   fall 400 ms ahead is not.
5. **Verse versus breakdown** — two frame windows of clearly different energy
   produce `level` values that differ by at least 2×. This is the numeric
   version of "the two screenshots must differ".
6. **Core shape is per track** — same title/artist ⇒ identical coefficients;
   a different title ⇒ different; and the shape never changes across
   `advanceTo` calls.
7. **Hue is fixed** — `hue(0°) == 250`, `hue(90°) == 340`, `hue(180°) == 70`,
   `hue(270°) == 160`; and hue is independent of every band value.
8. **No analysis** — an empty `SpectrogramFrames` yields the resting state,
   `level == 0`, no transient, and `advanceTo` never throws.

### P7 — retire the visualizer modes (D2)

Owns: `VisualizerSelection.kt`, `NowPlayingVisualizer.kt`,
`MainActivityVisualizerTest.kt`, `VisualizerSelectionTest.kt`

- Delete `MobileVisualizer`, `VisualizerController`, `VisualizerControl`,
  `LocalVisualizerControl`, the mode bar and the long-press menu, and
  `NowPlayingVisualizer` itself.
- **Do not touch the stored value.** `visualizer_setting()` stays in
  `appearance.rs` and keeps whatever the desktop wrote. A retired surface does
  not get to overwrite a shared setting on its way out — the same reasoning the
  current fallback comment gives.
- `DockMode.kt` keeps calling `AmbientFields(visual?.ambientColors)` (D3);
  `AmbientArtwork.kt`, `AmbientSurface.kt`, `ambientFieldColors` and
  `extractAmbientArtworkColors` all stay for it.
- Tests that asserted mode switching are deleted with the feature, not
  disabled. `MainActivityVisualizerTest` is rewritten in P5 against the new
  toggle.

## Wave B — renderers (P3, P4, P6 in parallel; all need P2)

### P3 — the cover fog

Owns: `NowPlayingFog.kt`, `CoverFogBitmap.kt`

The fog **is the cover**. No palette extraction, no accent tinting, no special
cases. A pale cover gives a pale fog and that is correct; a greyscale cover
gives a grey fog and that is correct too.

Two copies of the same cover behind the sharp one:

| Layer | Size | Blur | Opacity | Blend | Rotation |
|-------|------|------|---------|-------|----------|
| 1 | 620 dp | 46 dp | 0.92 | normal | `+= level * 0.9` per frame |
| 2 | 470 dp | 26 dp | 0.55 | Screen | `-= level * 0.6` per frame |

Counter-rotating on purpose: same direction reads as a camera shake, opposite
directions read as depth.

Over them: a radial gradient fading outward to black, plus a soft scrim from
the top and from the bottom so the header and the controls stay legible.

**Performance — this is the part that decides the battery number.** The blur is
**not** a per-frame filter. `Modifier.blur` needs API 31 and this app ships
`minSdk = 26`, so it is not even available on the whole fleet.

- `CoverFogBitmap` renders the artwork **once per track** into a small
  pre-blurred `ImageBitmap` (256 px, two box-blur passes, on the analysis
  worker or another background thread), one per layer if the two radii need to
  differ visibly at that scale.
- The draw then only scales and rotates that bitmap. Nothing per frame is
  filtered.
- No cover at all: the same mechanics fed from the app accent.

**The cover itself does not move.** No pulsing, no scaling. It is the anchor;
only the fog behind it reacts. A pulsing cover is cheap and destroys the calm
this view is selling.

### P4 — the burst

Owns: `NowPlayingBurst.kt`

Centre at **47 %** of the height.

- **Wedges — they are the background, not an accent.** 112 wedges over the full
  circle, from the centre out past the screen edge. Opacity and lightness per
  wedge from the band value at that angle. There is no black area left: the
  whole screen is lit and the corners carry colour. Over them a radial gradient
  darkening outward, otherwise the edges go flat.
- **Corona.** 168 fine strokes on a ring at radius `86 dp + bass * 26 dp`.
  Length `16 dp + band * 62 dp * level`. Stroke width 2.1 dp, round caps.
- **Core.** Dark, **irregular** shape around radius 78 dp with a slightly wavy
  edge from `CoreShape` — an exact circle looks like a diagram. The shape is
  fixed per track; re-rolling it per frame makes the edge flicker. The radius
  breathes with the bass.
- **Hot ray.** One single, distinctly brighter wedge: white at the core,
  running out through a skin tone into orange, plus a thin white line from the
  centre. Only above the transient threshold, opacity from the excess. It is
  the only element that breaks the symmetry — that is why the picture reads as
  alive rather than decorative. **Exactly one, never several.**
- **Bloom.** A second, unsharp copy over the top in Screen blend.
  Blur `6 dp + level * 16 dp`, opacity `level²` — squared, because linear stays
  milky. Implementation: draw the scene into an offscreen `ImageBitmap` at
  **quarter resolution**, then draw it back scaled up and additive. Quarter
  resolution means a quarter of the pixels — each edge halved, so `BLOOM_SCALE`
  is 2. The first implementation halved each edge twice and rendered a
  sixteenth of the area, which is cheaper but visibly coarser once the bloom is
  strong; decided on 2026-08-09 in favour of the finer buffer.
- Colour from `SceneColour` (P2): top blue, right magenta, bottom orange, left
  green, fixed for the whole song.

### P6 — the frame driver and the power gates

Owns: `SceneDriver.kt`, `AmbientRuntime.kt` (extension only)

- A `withFrameNanos` loop that computes the current audio position, converts it
  to a frame index and calls `SceneState.advanceTo`. It **only** invalidates
  the canvas when `revision` changed or a transition is running. On pause: one
  frame, then nothing. In quiet passages the frame rate drops by itself —
  correct.
- The audio position between playback ticks is interpolated from the wall
  clock **for the position estimate only**; the scene still steps whole frames,
  in order, so the estimate cannot change the outcome. First task of this
  package: measure the cadence of `PlaybackUiState.positionMs` and size the
  interpolation to it.
- Reuse the existing `AmbientMotionController`: it already carries `resumed`,
  `screenInteractive` and the `ANIMATOR_DURATION_SCALE` truth, and
  `BindAmbientRuntime` already wires lifecycle, screen-off broadcast and the
  settings observer. In the background and with the screen off the loop stops
  **completely**. A fullscreen visualizer still running in a trouser pocket is
  the fastest route to bad reviews.
- System "animations off": no bloom, no hot ray, no fog rotation; corona static
  at its current value.

Tests: driver stepping against a fake clock and a fake position source —
pause produces no further `revision` bumps; screen-off stops the loop;
animations-off suppresses exactly the three elements named above.

## Wave C — P5, the screen (needs A and B)

Owns: `NowPlayingScene.kt`, `NowPlayingSheet.kt`,
`crates/reprise-android-ffi/src/appearance.rs`, `MainActivity.kt`,
`TrackAnalysisLoader.kt`, and the rewritten `MainActivityVisualizerTest.kt`.

**Layout, played view** (frame 27b), top to bottom:

- status bar, then a narrow header row: chevron-down on the left (closes the
  sheet); on the right queue, sleep timer, heart and the **fullscreen** icon
  (D1).
- cover 272 dp, radius 18 dp, sharp, centred, its centre at about **34 %** of
  the height, with a strong drop shadow downwards.
- title 24 sp, artist 13 sp at 62 % white, both centred.
- progress, times and the transport row as they are today
  (`SpectralSeekSlider`, `PlaybackActions`).

**Controls, fullscreen** (frame 28a): title 33 sp light, artist 15 sp light,
centred at the top. Progress a 3 dp line with a 12 dp knob, times 13 sp. Along
the bottom five flat icons in one row: queue · shuffle · pause · repeat · back
to the played view. Pause is a 62 dp circle, **outline only** (1.6 dp white),
not filled. No surfaces, no cards, nothing filled — in front of this picture
any filled area is a foreign body.

After **4 s** without a touch the controls fade out over 300 ms; a tap brings
them back. Title and progress line stay — they are what you need in passing.

**Persistence.** A new setting in `appearance.rs`, following the existing
`visualizer_setting` pattern exactly:
`AndroidStoredNowPlayingView { Player, Visualizer, Unset, Unsupported }` plus
`AndroidNowPlayingViewChoice` and `set_now_playing_view()`. The state survives
restarts and holds across tracks: whoever left the visualizer open gets it back
on the next track.

**The transition** — 320 ms, `FastOutSlowInEasing`, both directions:

- the cover scales and fades out while the core fades in,
- the fog keeps running and passes over into the wedges; **do not cut** — both
  scenes draw simultaneously for the duration,
- the controls stay where they are and only change size, so the pause button
  stays operable throughout the transition.

Both states draw into one `Canvas`; the transition is a progress value passed
to both renderers, not two composables swapped by `AnimatedContent`.

**Background is true black** (`#000000`). On OLED every area outside the
graphic then costs nothing.

Tests: toggle switches the state and writes the setting; a restart with a
stored `Visualizer` opens fullscreen; the controls carry the faded tag after
4 s of virtual time and lose it on tap; the pause button is hit-testable at
transition progress 0, 0.5 and 1.

## Wave D — P8, verification

Owns: `docs/plans/now-playing-scene-verification.md` and the harness scripts.

From the brief, in order of importance:

1. **Pause.** Nothing may move any more — not the fog, not the corona. Not
   fading out, not slowing down: standing. Covered numerically by P2 test 2 and
   visually by two screenshots 3 s apart while paused, which must be
   pixel-identical.
2. **Play the same song twice from the start** — the fog must run identically.
   If it does not, something still hangs on the system clock. P2 test 1 plus a
   recorded angle trace over the first 30 s, compared between two runs.
3. **Two screenshots, one in the verse, one in the breakdown** — they must
   differ clearly. If they look similar the scaling is too flat.
4. **A greyscale cover** — the fog may be grey. That is right, not broken.
5. **A very bright cover** — title and times must stay legible. If not,
   strengthen the top and bottom scrims, not the type.
6. **A track without analysis** — resting state, no crash, immediate playback
   start.
7. **The transition back and forth quickly, several times** — no stutter, no
   double draw, and the pause button operable throughout.
8. **Battery per hour in both states.** The played view must be clearly
   thriftier than the visualizer; if it is not, the fog is drawing too often.

Honest note on 8: battery per hour is a **device** measurement. On the
`pixel10xl_api37` emulator it is meaningless. The emulator run measures frame
callbacks per second and `dumpsys gfxinfo` instead, and the real battery figure
stays a task for a physical device. The plan does not claim the emulator
number as the answer.

---

## Out of scope

- Dock mode keeps its palette fog (D3). If it should adopt the cover fog later,
  that is its own change with its own visual review.
- The desktop visualizer is untouched.
- No new analysis anywhere. If a track has no spectrogram, the answer is the
  resting state — not a computation on the phone.

## Risks

| Risk | Handling |
|------|----------|
| `dev` moves `NowPlayingSheet.kt` or `MainActivity.kt` under us mid-run | `dev` merges here run hourly. Rebase before starting and again before review, and compile after every rebase — a rebase can break without conflicting. |
| `positionMs` ticks too coarsely for a smooth frame index | Measured in P6 as its first task; the interpolation is sized to the measurement, and the whole-frame stepping keeps the result deterministic either way. |
| Bloom at quarter resolution looks banded on large screens | Visual check in P8; the fallback is an eighth-resolution blur pass over the same offscreen, not a full-resolution filter. |
| `minSdk = 26` has no `RenderEffect` | Already the reason the fog is a pre-blurred bitmap; no code path may depend on API 31 blur. |
