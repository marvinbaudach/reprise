# Visualizer: Honest-Loudness Rework + Mode Trim

Date: 2026-07-22 · Branch: `fix/visualizer-honest-loudness` (base `dev`)

## Problem

The song visualizer feels out of sync with the music and over-reacts: a tiny,
quiet sound already drives the Grid mode into the red "crest" zone. Two
compounding root causes:

1. **Per-band AGC in the analyzer** (`playback.rs`). Each display band is
   divided by its own slowly-decaying recent maximum
   (`shape = folded[band] / agc[band]`, `AGC_FLOOR = 0.26`, 8 s half-life). This
   deliberately normalizes loudness *away* — a band near its own recent peak
   reads ~1.0 regardless of whether that peak was loud or faint. After a quiet
   stretch the reference decays to the floor and a ~-56 dB tone drives full
   scale. The `audible` knee (-36 dB) meant to counter this saturates for most
   audible content, so the AGC dominates. Net: quiet ≈ loud on screen.
2. **Grid drive is far too hot** (`water.rs` + `grid.rs`). `DRIVE_AMPLITUDE =
   2.6` turns a band of 0.2 into wave height 0.52, and the red crest triggers at
   height `> 0.5`. So a band value of ~0.2 already paints a red crest.

## Decision (user-approved)

- **Honest to loudness.** Remove AGC; band height reflects real loudness. Quiet
  passages read calm, loud drops reach full scale, "hot" color only on genuine
  peaks. (User chose this over a "keep-it-lively" variant.)
- **Reactivity fix applies to every retained mode** (shared analyzer change).
- **Optical polish for Bars, Grid, Flow only.** Pulse keeps its look (reactivity
  fix only).
- **Remove Neon + Particles entirely** (modes, buttons, dead helpers), with a
  safe fallback to Grid for any persisted `neon`/`particles` setting.

Final mode set: **Grid, Bars, Flow, Pulse**.

## 1. Analyzer — proven perceptual pipeline (`playback.rs`)

Replace the AGC/`audible`/gamma block in `SpectrumAnalyzer::ingest` that
produces the display `bands` (leave `level`/`bass`/`beat`/`dynamics` untouched —
they already derive from the honest `folded` values).

Per band, working from the max-pooled `folded[band]` (= `(dB+80)/80`,
GStreamer threshold -80 dB, 0 dB = full scale):

1. Reconstruct dB, apply a **fixed dB window** `DISPLAY_DB_MIN..DISPLAY_DB_MAX`
   (start **-70 .. -12 dB**, a ~58 dB span; cf. audioMotion's -85/-25, W3C
   AnalyserNode default -100/-30). `norm = ((dB - MIN)/(MAX - MIN)).clamp(0,1)`.
2. **Pink-noise tilt** `+3 dB/octave` (per-band, precomputed from band center
   frequency at a nominal 44.1 kHz) so treble bands stay alive.
3. Mild **contrast gamma ≈ 1.3** (down from 2.0 — the dB curve is already the
   perceptual mapping).
4. **Gentle noise gate** just above the floor to kill sub-audible shimmer (no
   -36 dB knee).

Remove: `AGC_HALF_LIFE_MS`, `AGC_FLOOR`, `agc`/`agc_decay` state, `AUDIBLE_FLOOR`,
`AUDIBLE_KNEE`, old `DISPLAY_GAMMA` value. All new constants tunable; final
values set against headless screenshots.

Sources: W3C Web Audio API §1.8.6 (magnitude→dB→byte), MDN AnalyserNode,
audioMotion-analyzer defaults, pink-slope +3 dB/oct convention.

## 2. Reactivity recalibration (all four modes)

- **Grid** (`water.rs`): `DRIVE_AMPLITUDE` 2.6 → ~1.4; crest threshold
  (`grid.rs`) 0.5 → ~0.85; adjust `h` clamp headroom.
- **Bars/Flow/Pulse**: retune each mode's "hot color" threshold and displacement
  gain so nothing hits accent2/full-scale on quiet input.

## 3. Optical polish

- **Bars**: peak-hold caps drawn from `ctx.peaks[]` (classic analyzer look);
  smooth accent→accent2 blend instead of the hard flip at 0.66; subtle base
  reflection for depth.
- **Grid**: crest red fades in by height instead of a binary threshold; slight
  depth darkening toward the horizon. **Big-beat splash**: on strong beats
  (scaled by `beat.strength`), inject a pronounced upward velocity impulse into
  the water mesh so liquid erupts up and falls back under the existing spring
  physics — replaces the current small `level`-scaled `splash()`. Gate on
  strength so only real, big beats erupt.
- **Flow**: filled area under the middle trail (gradient → transparent) for
  depth; glow coupled to `level`.
- **Pulse**: no polish (reactivity fix only).

## 4. Remove Neon + Particles

- `engine.rs`: drop `Neon`/`Particles` from `VisualMode` enum, `ALL`, `id()`,
  and any cycling/`from_id`.
- Delete `modes/neon.rs`, `modes/particles.rs`; update `modes.rs` dispatch +
  `mod` declarations.
- Dead-helper cleanup: remove the dust **particle** field/`DUST_COUNT`/generation
  from the engine + `ctx.dust` (particles-only) **but keep `dust::xorshift`**
  (used by `water`/`impact`). Remove `hue_sweep_fill`/`rgb_hue` if unused after
  Neon is gone.
- Frontend (`reprise-gnome`): remove Neon/Particles buttons + labels in
  `song_visualizer.rs`, `strings_audio_analysis.rs`, and fix
  `song_visualizer_tests.rs`.
- **Migration**: loading a persisted mode id of `neon`/`particles` (or any
  unknown id) falls back to `Grid` — don't break existing users.

## 5. Verification

- TDD. New invariants: quiet field (~-55 dB equiv) produces **no crest / no
  hot-color** in any mode; silence → engine settles still; full-scale input →
  reaches hot-color/full deflection; big beat → water erupts then settles.
- Keep existing mode tests green (adjust expectations changed by removals).
- Headless render (Xvfb screenshot, **no** desktop window) comparing a quiet vs
  loud vs big-beat frame to eyeball sync; tune constants against it.
- `rust-reviewer` pass before finishing.

## Out of scope

Rendering-engine changes (stay within existing Scene primitives), new modes,
changes to beat/level/bass/dynamics derivation.
