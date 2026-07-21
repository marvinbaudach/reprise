# Audio-Reactive Visualizer — Design

**Date:** 2026-07-21
**Branch:** `feat/improve-visual-effects`
**Status:** Approved (design), pending implementation plan

## Problem

The Now Playing "Visual" tab (modes Rings / Flow / Pulse) reacts too weakly to
the music. Users cannot read *what is happening* in the audio — e.g. a deathcore
breakdown or a techno kick. The data path is already sound (real FFT via the
GStreamer `spectrum` element, no fake animation), so the weakness is in
smoothing, event detection, and visual mapping — not the data source.

### Root causes

1. **Symmetric smoothing kills transients.** `advance_state` eases `current`
   toward `target` by only `delta * 0.18` per frame. Sharp hits (kick,
   breakdown slam) are averaged away. Needs a **fast-attack / slow-release**
   envelope.
2. **No onset/beat detection.** Nothing distinguishes "loud and sustained" from
   "sudden impact" — which is exactly the legibility the user wants.
3. **Coarse resolution / update rate.** 16 bands at 50 ms (20 Hz) is marginal
   for crisp beats.
4. **Timid mapping.** Large fixed base values plus a small energy-scaled term →
   little visible swing at real playback levels.

## Goals

- Visuals react *hard* to the music (attack punch, big dynamic swing).
- Discrete musical events are **legible**: techno kicks, breakdown slams, drops,
  build-ups.
- **Entertaining & playful**: fun to *watch*, not just informative. It should
  reward staring at it — spectacle, surprise, and variety over a whole song.
- Stay recognizably Reprise (accent-color line work, depth) while allowing more
  expressive, playful motion than the current restraint.
- Respect reduced-motion / `animations_enabled()` gating (Motion rulebook O).

## Design direction — playful spectacle

The point is delight, not a diagnostic readout. Guiding principles:

- **Reward the eye.** Beats and drops should feel like little events worth
  watching. Overshoot, bounce, and settle rather than snapping to a value.
- **Springy, organic motion.** Prefer spring/overshoot easing for impacts so
  things feel alive and bouncy, not mechanical.
- **Emit, don't just scale.** On strong beats, spawn transient ornaments —
  radiating sparks / particles / satellite dots that fly out and fade — so the
  canvas feels generative, not a static gauge that grows and shrinks.
- **Playful accent variety.** Allow tasteful lightness/saturation shifts of the
  accent on impact (brighter on a hit) so color itself reacts — without leaving
  the accent identity.
- **Stay fresh over time.** Slow ambient drift (rotation, gentle wander) so a
  long track never freezes into a static picture between beats.
- **Discipline still applies.** Playful ≠ noisy. Motion clarifies the music;
  everything still respects reduced-motion and the photosensitivity-safe caps.

## Decisions (locked)

- **Analysis lives in `reprise-core`** (pure Rust, portable, testable), not in
  the GTK visualizer.
- **Refine the audio pipeline**: more bands + faster interval.
- **Dedicated impact layer** on top of stronger baseline reactivity.
- **Modes open to rework** (keep concepts, amplify, optionally add one mode).
- **Single enriched frame** (approach A): one core analyzer emits one
  `VisualFrame`; the existing `PlayerEvent::Spectrum` carries it. Rejected: a
  second parallel event stream (approach B) and doing the math in the frontend
  (approach C).

## Architecture

```
GStreamer spectrum (32 bands, ~23 ms)
  → spectrum_frame_from_structure  (reprise-platform-linux)
  → SpectrumAnalyzer::ingest(raw_db_frame)   (reprise-core, stateful)
  → VisualFrame { bands, level, bass, beat, dynamics }
  → PlayerEvent::Spectrum(VisualFrame)
  → NowPlayingPanel::set_spectrum
  → SongVisualizer::set_spectrum  (RenderState + impact envelopes)
  → per-mode Scene + shared impact layer  (Cairo)
```

## 1 · Audio analysis (reprise-core)

### Constants

- `SPECTRUM_BAND_COUNT`: `16 → 32`.
- New `SPECTRUM_INTERVAL_MS` (target ~23 ms; owned by core, consumed by the
  platform layer when building the `spectrum` element).

### `SpectrumFrame` → `VisualFrame`

`SpectrumFrame` (16 normalized bands) becomes `VisualFrame`, carrying the
normalized bands plus derived scalars. All scalars are finite and bounded.

| Field      | Type            | Meaning |
|------------|-----------------|---------|
| `bands`    | `[f32; 32]`     | normalized `0..=1`, per-band detail |
| `level`    | `f32` `0..=1`   | overall envelope, fast attack / slow release (~250 ms release) — the "punch" |
| `bass`     | `f32` `0..=1`   | same envelope over the lowest ~4 bands (kick/sub) |
| `beat`     | `Beat { fired: bool, strength: f32 }` | onset event this frame |
| `dynamics` | `f32` `-1..=1`  | short-term loudness vs slow baseline: `+` = drop/slam after a lull, `-` = sudden quiet |

### `SpectrumAnalyzer` (stateful, pure Rust)

Holds cross-frame state (previous bands, envelopes, flux running mean/variance,
short + long loudness baselines, beat refractory timer). `ingest(raw_db_frame)`
returns a `VisualFrame`.

- **level / bass**: envelope follower — `if x > env { env = x }` (instant
  attack) else `env += (x - env) * release_coeff` (slow release derived from the
  frame interval so behavior is rate-independent).
- **beat**: **spectral flux** = sum of positive per-band differences vs the
  previous frame, low-frequency-weighted. Fires when flux exceeds an *adaptive*
  threshold (running mean + k·std over a short window) and the refractory timer
  has elapsed; `strength` = normalized overshoot above threshold.
- **dynamics**: short-term loudness EMA minus long-term baseline EMA, squashed to
  `-1..=1`. Detects drops/slams (positive) and sudden lulls (negative).

Rate-independence: all time constants are derived from `SPECTRUM_INTERVAL_MS` so
a future interval change does not re-tune the feel.

### Pipeline (`reprise-platform-linux`)

- `player_effects.rs`: `spectrum` element → 32 bands, ~23 ms interval (pull
  values from the core constants).
- `player.rs`: `spectrum_frame_from_structure` reads 32 magnitudes; the bus
  watch feeds them through the shared `SpectrumAnalyzer` before emitting
  `PlayerEvent::Spectrum(VisualFrame)`.

## 2 · Impact layer (rendering, `song_visualizer.rs`)

`RenderState` gains the scalar fields plus short-lived **impact envelopes** that
the tick loop decays independently of the band values:

- **Beat shockwave**: on `beat.fired`, spawn an expanding ring (radius grows,
  alpha fades over ~350 ms); size/width scale with `strength`. Small ring buffer
  so overlapping beats stack. Drawn mode-agnostically over the scene.
- **Spark burst (playful)**: strong beats also emit a handful of short-lived
  particles/sparks that radiate outward and fade — count/velocity scale with
  `strength`. A lightweight fixed-capacity particle pool (no per-frame
  allocation); positions integrate in the tick loop. This is what makes a kick
  feel *fun*, not just measured.
- **Bass breathing**: `bass` envelope scales each mode's base size with a slight
  **spring overshoot** — the scene punches past and settles, so kicks bounce.
- **Breakdown / drop flash**: when `dynamics` crosses a threshold, a brief soft
  full-canvas glow (radial accent gradient, ≤ ~15 % opacity, soft fade) *plus* a
  bigger one-shot shockwave — the drop should feel like an event. **No** hard
  white strobe — photosensitivity-safe by construction.
- **Level glow + accent lift**: `level` lifts global alpha / line width, and
  beats briefly brighten/saturate the accent (bounded, identity preserved) so
  color itself reacts.
- **Ambient drift**: a slow continuous rotation / wander of the composition so
  the canvas never freezes into a static picture between beats.

**Springy attack/release replaces the 0.18 easing**: `advance_state` uses
asymmetric smoothing (fast up, slow down) for bands and scalars, and impact
envelopes use spring/overshoot decay for bounce. This is the core fix for the
"timid" feel and the source of the playful bounce.

## 3 · Modes

- **Rings → kick-forward**: concentric rings pulse on the beat (shockwave
  integrated); bands become radial spokes rather than static bars.
- **Pulse → bass-driven**: central core punches hard on each kick; spokes shoot
  outward on onsets.
- **Flow → level-coupled**: wave amplitude tracks `level`; onsets produce visible
  spikes/"tears" in the trails; calm when the music is calm.
- **New "Bars" mode (optional, if the rework affords it)**: classic 32-band bars
  with peak-hold — the most direct read of the frequency picture. May be cut if
  it inflates scope; cutting it does not affect the other goals.

## 4 · Accessibility & gating

- Everything stays behind `motion::animations_enabled()`.
- Reduced motion: static profile, **no** flashes/shockwaves; at most gentle
  `level` mapping.
- The dynamics glow is deliberately soft and accent-tinted (no full-luminance
  strobe) even when motion is enabled.

## 5 · Testing

- **reprise-core** (`SpectrumAnalyzer`), deterministic + headless:
  - constant input → no beat, stable envelopes.
  - single impulse → one beat + attack spike, then release decay.
  - silence-then-slam → positive `dynamics` spike + beat.
  - linear ramp / build-up → no false-positive beat storm.
- **reprise-gnome**: extend existing scene tests — scenes stay finite & bounded
  including active impact envelopes and live particles; impact envelopes and the
  particle pool decay to rest; the particle pool never exceeds its fixed capacity
  (no unbounded growth / allocation under a beat storm).
- **Verification headless** (Xvfb screenshot) — never open a desktop window.

## Build order

1. **Core foundation** — constants (32 bands, interval), `SpectrumAnalyzer` +
   `VisualFrame` + tests; migrate `player_effects.rs` and
   `spectrum_frame_from_structure` to 32 bands.
2. **Envelope rework** — asymmetric attack/release in the visualizer; wire
   scalars into `RenderState`. Already dramatically more reactive here.
3. **Impact layer** — shockwave, bass breathing, dynamics glow, level glow;
   mode-agnostic.
4. **Mode rework** + optional Bars mode.
5. **Tests + headless verification**, then commit.

Steps 1 and 2 can be prepared in parallel (core vs. visualizer envelope touch
separate files).

## Out of scope

- Beat-grid / BPM display, tempo sync, or persisted per-track analysis.
- Any change to the audio *character* analysis (the static, stopped-state
  profile) beyond consuming its existing 4 dimensions as today.
- Reworking the fullscreen window chrome or preset-control layout beyond adding
  one toggle if the Bars mode ships.
