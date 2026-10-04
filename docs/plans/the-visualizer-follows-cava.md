---
slug: the-visualizer-follows-cava
worktree: /home/marvin/Projects/reprise-the-visualizer-follows-cava
branch: feature/the-visualizer-follows-cava
phase: planned
codex_session:
created: 2026-10-04
---
# The visualizer follows CAVA again

## Goal

The Song Visuals bars feel unnatural. Measured against the original `cavacore.c`
(CAVA, pinned commit `4b12c2b043723f42567ddbfd5a516566bdf52316`, the same commit
`crates/reprise-core/src/playback/cava.rs` names as its source), the Rust port is
faithful — band plan, EQ, FFT, input order, gravity, integral and autosensitivity
all match — **except for two additions cavacore does not have**, both in
`crates/reprise-core/src/playback/cava/smoothing.rs`:

1. **Frame-wide duck on every overshoot** (`output_scale`, around line 125). When
   any band overshoots 1.0, the whole frame is rescaled to a max of
   `INITIAL_SENSITIVITY_HEADROOM` (0.85). Because line ~114 sets
   `sensitivity_settling = true` on *every* overshoot, the following frame is
   rescaled too. cavacore instead clips only the overshooting band to 1.0
   (`cavacore.c:437-443`) and leaves every other band alone. This was meant for the
   cold start only (commit `7b54f9920b`, "prevent cold CAVA saturation").
2. **Noise-floor gate** (`*bar <= self.noise_floor → 0`, around line 79), applied
   after sensitivity and *before* gravity and the integral. The integral
   amplifies about 4.4×, so the 0.04 gate removes roughly the lowest 17 % of the
   display height and breaks the gravity fall of quiet bands.

Both are removed, except that the cold-start protection stays, and only for the
cold start.

## Evidence (2026-10-04)

The harness lived in `~/.cache/reprise-scratch/cava-oracle.9b8Yd8/`. It fed the C
oracle and the port the same PCM in 735-sample hops (desktop cadence, 60 Hz), at
44.1 kHz mono, with 64 bars. Frames 0–299 were skipped.

| Track | Arm | frames ducked while the oracle clips | frames whose total spectrum is < 90 % of the oracle | bar-frames gated to 0 while the oracle shows > 0.05 |
|---|---|---|---|---|
| rock FLAC | dev | 5.4 % | 31.2 % | 1.85 % |
| rock FLAC | no duck | 0 | 25.3 % | 1.85 % |
| rock FLAC | no duck, no floor | 0 | 0 | 0 |
| bass-heavy FLAC | dev | 6.6 % | 71.9 % | 6.1 % |
| bass-heavy FLAC | no duck | 0 | 66.0 % | 6.1 % |
| bass-heavy FLAC | no duck, no floor | 0 | 0 | 0 |

The control arm ("no duck, no floor") matches the oracle to within a mean
absolute difference of 1.7e-5 and a max of 8e-4 over 60 s. The cutoff tables are
identical.

## Tasks

The task file lists are a starting point, not a fence. Stop only if the contract
below is itself wrong.

### Task 1 — Golden test against cavacore (write it first, see it fail)

Add a rule-named test that runs the real `CavaBarProcessor` against values
produced by the original C `cavacore`. Name it
`ac_28_cava_bars_match_the_cavacore_reference_after_calibration`. Put it in
`crates/reprise-core/src/playback/cava_tests.rs`, or in a cohesive sibling test
module if that file would pass 800 lines.

- Processor: `CavaBarProcessor::new(CavaConfig::new(44_100, 64))`, defaults
  unchanged (noise reduction 0.77, autosensitivity 1, 50–10 000 Hz).
- Input: 360 consecutive chunks of 735 samples each (264 600 samples), each chunk
  passed to `process_into` once. Frame `k` is the bar output after chunk `k`,
  0-based. Sample `n` is computed in `f64` and then cast to `f32`:
  ```
  t    = n as f64 / 44_100.0
  kick = (-(t % 0.5) / 0.06).exp()
  x    = 0.45 * kick * (TAU * 55.0 * t).sin()
       + 0.15 * (TAU * 440.0 * t).sin()
       + 0.08 * (0.5 + 0.5 * (TAU * 1.5 * t).sin()) * (TAU * 2500.0 * t).sin()
       + 0.04 * (TAU * 7000.0 * t).sin()
  ```
  The peak is below 0.72, so the port's ±1 input clamp never applies.
- Assertion: for frames 172, 240, 255 and 330, every one of the 64 bars is within
  **2e-3 absolute** of the reference below. Frames 172 and 240 are frames where
  cavacore clips one band (bar 26) to 1.0 and leaves the rest untouched. Most of
  the low values sit under the current noise floor's reach.
- On current dev this test must FAIL. Run it and record the failure in the commit
  body before implementing.

Reference values from cavacore, generated with FFTW. The C driver fed
`(double)(float)x * 65535.0`, which matches `CAVA_FIXED_POINT_SCALE`:

```
// frame 172
[0.315462, 0.538361, 0.432593, 0.227695, 0.133478, 0.099056, 0.077458, 0.061830, 0.051015, 0.027866, 0.023555, 0.020010, 0.017583, 0.015646, 0.013820, 0.011710, 0.010882, 0.009944, 0.008711, 0.007743, 0.006963, 0.006320, 0.005764, 0.005153, 0.004577, 0.022583, 1.000000, 0.004843, 0.003091, 0.002839, 0.002604, 0.002353, 0.002143, 0.001937, 0.001762, 0.001596, 0.001455, 0.001322, 0.001207, 0.001090, 0.000992, 0.000902, 0.000820, 0.000746, 0.000680, 0.000619, 0.000949, 0.450923, 0.000468, 0.000425, 0.000388, 0.000354, 0.000323, 0.000295, 0.000270, 0.000247, 0.000226, 0.000207, 0.000190, 0.209014, 0.000162, 0.000150, 0.000139, 0.000129],
// frame 240
[0.058821, 0.118953, 0.080272, 0.036906, 0.022331, 0.017028, 0.013680, 0.011222, 0.009442, 0.008849, 0.007267, 0.005997, 0.005171, 0.004654, 0.004204, 0.003551, 0.003248, 0.002986, 0.002650, 0.002336, 0.002131, 0.001950, 0.001833, 0.001793, 0.002247, 0.022507, 1.000000, 0.004799, 0.001217, 0.000944, 0.000827, 0.000729, 0.000656, 0.000589, 0.000534, 0.000482, 0.000439, 0.000399, 0.000364, 0.000328, 0.000299, 0.000272, 0.000247, 0.000225, 0.000206, 0.000192, 0.000418, 0.123017, 0.000157, 0.000130, 0.000117, 0.000107, 0.000097, 0.000089, 0.000081, 0.000074, 0.000068, 0.000063, 0.000060, 0.209097, 0.000067, 0.000046, 0.000042, 0.000039],
// frame 255
[0.429656, 0.687207, 0.587391, 0.333759, 0.196359, 0.145710, 0.114006, 0.091001, 0.075086, 0.057741, 0.048798, 0.041457, 0.036423, 0.032416, 0.028631, 0.024266, 0.022533, 0.020599, 0.018045, 0.016041, 0.014429, 0.013106, 0.011896, 0.010570, 0.010168, 0.021761, 0.952236, 0.008296, 0.006561, 0.005914, 0.005384, 0.004882, 0.004438, 0.004014, 0.003651, 0.003307, 0.003013, 0.002738, 0.002500, 0.002259, 0.002056, 0.001869, 0.001700, 0.001546, 0.001408, 0.001284, 0.001266, 0.453540, 0.000972, 0.000881, 0.000804, 0.000733, 0.000670, 0.000611, 0.000559, 0.000511, 0.000468, 0.000429, 0.000394, 0.198836, 0.000337, 0.000310, 0.000287, 0.000268],
// frame 330
[0.056308, 0.113861, 0.076799, 0.035371, 0.021457, 0.016393, 0.013195, 0.010844, 0.009135, 0.009009, 0.007397, 0.006102, 0.005260, 0.004732, 0.004278, 0.003618, 0.003308, 0.003033, 0.002696, 0.002386, 0.002149, 0.001996, 0.001845, 0.001783, 0.002370, 0.022057, 0.983998, 0.004396, 0.001311, 0.000981, 0.000834, 0.000739, 0.000666, 0.000599, 0.000543, 0.000491, 0.000447, 0.000405, 0.000370, 0.000334, 0.000304, 0.000277, 0.000252, 0.000230, 0.000211, 0.000203, 0.000858, 0.408962, 0.000192, 0.000136, 0.000120, 0.000109, 0.000099, 0.000090, 0.000083, 0.000076, 0.000069, 0.000064, 0.000061, 0.205451, 0.000067, 0.000046, 0.000043, 0.000040],
```

Keep the doc comment on the test short: these are cavacore outputs for this
signal, and any deviation means the port drifted from its source.

### Task 2 — The frame-wide duck protects the cold start only

File: `crates/reprise-core/src/playback/cava/smoothing.rs`.

- `sensitivity_settling` becomes `true` only on the overshoot that ends
  `sensitivity_initializing`, which is the first overshoot after a cold
  `Smoother::new`. Later overshoots never set it.
- `output_scale` applies only while `protect_initial_output` holds. Drop the
  `|| overshoot` arm. In steady state an overshoot is handled by the existing
  per-band `.clamp(0.0, 1.0)`, which is exactly cavacore's clip.
- Keep the gain-search arithmetic, `MAX_INTEGRAL_FEEDBACK`, the sensitivity
  clamps, the non-finite guards and `reset()` exactly as they are.
  `reset()` keeps the settled gain; that was a deliberate earlier fix.
- The existing cold-start tests stay green unchanged:
  `cold_rising_signal_does_not_expose_autosensitivity_clipping` and
  `cold_start_does_not_inflate_a_quiet_signal_to_full_scale`.
- Add a focused `Smoother`-level test. Once calibration has completed (first
  overshoot, then one non-overshooting frame with signal), a frame where one band
  overshoots must clip that band to 1.0 and leave every other band at its exact
  unscaled value. The frame after it must not be rescaled either. Give it a
  rule-named `ac_28_` name.

### Task 3 — Remove the noise-floor gate

Files: `crates/reprise-core/src/playback/cava.rs`, `cava/smoothing.rs`,
`playback/cava_tests.rs`.

- Delete the gate, the `noise_floor` field on `Smoother` and on `CavaConfig`,
  `DEFAULT_NOISE_FLOOR`, and `CavaError::InvalidNoiseFloor`, with its validation.
  Reprise is unreleased, so no compatibility shim (AGENTS.md, "Not released yet").
- The non-finite guard stays: a non-finite bar is still neutralised to 0.
- Delete `noise_floor_cuts_subthreshold_fft_leakage`. Fix the remaining
  `config.noise_floor = 0.0;` in `test_transient_processor`.
- Nothing outside `reprise-core` reads `noise_floor` or `InvalidNoiseFloor`
  (grep-verified). `CavaConfig::new` call sites in `reprise-platform-linux` and
  `reprise-android-ffi` stay untouched.

### Task 4 — UX rule: AC-28 replaces AC-23

File: `docs/ux-rules.md`, Song Visuals section. Follow the document's process
rules; read lines 1–50.

- AC-23's meaning changes, so it becomes the signpost
  `- **AC-23** [replaced by AC-28]`, in the same style as AC-7…AC-22 above it.
- Add **AC-28** `[active] [core] [gtk]`, placed where the existing replacement
  chain places the live rule. Its text is AC-23's full text with one change: the
  sentence "The portable core uses … noise-floor gate, auto-sensitivity, integral,
  and gravity." becomes:
  > The portable core uses CAVA's double FFT resolution below 100 Hz, quantized
  > cutoff frequencies, and a fixed frequency EQ, as well as auto-sensitivity,
  > integral, and gravity, exactly as `cavacore` computes them; there is no
  > noise-floor gate. An auto-sensitivity overshoot clips only the overshooting
  > band to 1.0. Only the cold-start calibration, until the first frame without
  > overshoot after the first overshoot, scales a whole frame down to headroom.

  Every other sentence stays verbatim, including the AC-27 reference.
- Move every test named `ac_23_…` onto `ac_28_…` in the same commit. That is 43
  test functions in:
  - `reprise-core`: `modules.rs`, `visuals.rs`, `visuals/modes/bars.rs`,
    `visuals/engine/{engine_tests,peak_visibility_tests}.rs`,
    `playback/{bass_pressure_tests,song_visual_tests}.rs`
  - `reprise-platform-linux`: `player/tests/cava_tests.rs`
  - `reprise-gnome`: `ui/now_playing/{now_playing_tests,song_visualizer_tests}.rs`,
    `ui/now_playing/song_visualizer/render.rs`,
    `ui/playback/player_event_handling.rs`

  Also update live code comments that cite `AC-23` to `AC-28`, in
  `playback.rs`, `visuals/engine.rs`, `song_visualizer.rs` and anywhere else under
  `crates/`. Historical documents under `docs/plans/` and `docs/research/` keep
  their `AC-23` references.
- `scripts/check-ux-traceability.sh` must pass.

## Verification scope

Run exactly these:

```
cargo fmt --check
cargo clippy -p reprise-core -p reprise-platform-linux -p reprise-gnome -p reprise-android-ffi --all-targets -- -D warnings
cargo test -p reprise-core
cargo test -p reprise-platform-linux cava
scripts/check-ux-traceability.sh
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must print nothing
```

Do NOT run `cargo test --workspace`, `cargo audit`, `gradlew`,
`uniffi-bindgen`, the Android suite, the display tests (`xvfb-run`,
`check-display-tests.sh`), or `check-merge-readiness.sh`. AGENTS.md asks for the
full gate before every commit. That instruction does not apply to this run, and
the exception is deliberate: the full gate runs at landing. A previous Codex run
lost 30 minutes and every commit by running workspace gates it was told to skip.

## Parallelität

This plan cannot be split into strands. Tasks 2 and 3 both edit `smoothing.rs`,
and task 4 renames tests that tasks 1–2 add to the same files. There is no
disjoint file group, so it runs as one strand.

## Manual check after landing (human)

Play the two measured kinds of material (rock, bass-heavy electronic) in
Song Visuals on desktop and Android. The kick should lift its bands without the
rest of the spectrum flinching, and quiet bands should fall smoothly instead of
cutting out. Headless runs cannot judge feel.
