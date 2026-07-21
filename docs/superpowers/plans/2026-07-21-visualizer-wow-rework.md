# Visualizer WOW Rework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Now Playing visualizer react hard and legibly to music (log-spaced bands, per-band auto-gain), add 8 selectable visual modes (Grid, Bars, Rings, Flow, Pulse, Particles, Neon, Tunnel) with a dual-accent color system, and rebuild the fullscreen view after the approved Claude-Design mock — with **everything except widgets and the Cairo draw loop living in `reprise-core`** so future frontends (KDE/Qt, Android, …) reuse the whole engine.

**Architecture:** `reprise-core` gains a `visuals` module: a stateful `VisualEngine` consumes `SpectrumFrame`s and 60 Hz ticks and emits a resolution-independent `Scene` (list of `Shape`s with concrete colors — solid or horizontal gradient). All mode math, envelopes, water simulation, particle pools, impact ornaments, and palette extraction are pure core code with pure tests. The GTK frontend shrinks to: a Cairo `Shape` renderer, the widgets (canvas, mode pills, fullscreen chrome), and plumbing. A future frontend implements one `Scene` renderer (~100 lines) and gets every mode for free.

**Tech Stack:** Rust (core: no GUI deps, `f32` geometry), gtk4-rs + Cairo (GNOME renderer), GStreamer `spectrum` element, rusqlite (next-up title lookup).

**Design source:** Claude-Design project `GTK4 Musik-Visualizer Darkmode`, file `Vollbild Visualizer.dc.html` (second revision, etag `1784630467844682`, read 2026-07-21). The mode math below is a faithful port of that mock's canvas code. Deliberately dropped mock-only artifacts: fake BPM beat synthesis, fake "notes"/shimmer spectrum bumps (real audio replaces them), cover drag-drop slots, the `colorSource` toggle, frame-precise timecode (positions arrive in ms → `H:MM:SS`).

**Design-revision deltas (folded in):**
- **Dual accent.** Secondary accent extracted from the cover (12 × 30° hue buckets, saturation/value-weighted; ≥ 2 buckets ≈ 60° from the primary hue; ≥ 16 % of the top bucket's weight; brightness-normalized). Fallback: `hue_shift(primary, +42°)`. The **primary stays the app-wide cover accent** — the frontend passes it in; only the secondary is engine-extracted. Users: Grid crests, loud Bars (> 0.66), shockwaves (Rings/Pulse), middle Flow trail, Pulse orbit dots, Particle chain tips.
- **Grid = water surface with beat splashes** (no scrolling history): 26×44 height+velocity field, far row driven by the mirrored spectrum, each beat throws 2–3 random Gaussian splashes (power scaled by level), spring/neighbor coupling + damping spread waves, cells above `0.5` glow in the secondary accent.
- **Frequency-dependent release** (highs fall faster) + faster attack.
- Bars: 64 columns (1:1 with display bands). Particles: 6 px raster. Pulse orbit sampling `0.075 + i·0.057`.

## Global Constraints

- **Portability rule:** nothing in `reprise-core::visuals` may depend on gtk4/cairo/glib. Geometry `f32`. No wall-clock reads — the frontend drives `tick()` at ~60 Hz.
- All visuals stay behind the frontend's reduced-motion gate (Motion rulebook O): reduced motion → engine is fed the static profile and not ticked; no flashes.
- User-visible strings: English source via `N_!` in `crates/reprise-gnome/src/ui/strings_audio_analysis.rs` (mode labels map from `VisualMode::id()` frontend-side).
- No fake audio data: every animated value derives from real `SpectrumFrame` signals; only ambient drift (`clock`) is time-based.
- Commit style `<type>: <description>`; `cargo fmt` + `clippy` clean; tests headless.
- Never open a desktop window for verification (PPM gallery / Xvfb only).
- Files focused; per-mode files well under 300 lines.

## File Map

| File | Responsibility | Task |
|---|---|---|
| `crates/reprise-core/src/playback.rs` | constants, log fold, AGC analyzer | 1–2 |
| `crates/reprise-platform-linux/src/{player_effects,player}.rs` (+tests) | 256-band element, raw parse | 3 |
| `crates/reprise-core/src/lib.rs` | `pub mod visuals;` | 4 |
| `crates/reprise-core/src/visuals.rs` | module root: re-exports | 4 |
| `crates/reprise-core/src/visuals/scene.rs` | `Rgba`, `Fill`, `Geom`, `Shape`, `Scene`, sanity check | 4 |
| `crates/reprise-core/src/visuals/color.rs` | HSL conversions, `hue_shift`, `secondary_accent` palette extraction | 4 |
| `crates/reprise-core/src/visuals/water.rs` | Grid water simulation | 5 |
| `crates/reprise-core/src/visuals/impact.rs` | shockwave/spark pools, flash, kick (moved from gnome) | 5 |
| `crates/reprise-core/src/visuals/dust.rs` | Particle-mode dust field + xorshift rng | 5 |
| `crates/reprise-core/src/visuals/engine.rs` | `VisualEngine`: state, envelopes, tick, ingest, `ModeCtx` | 6 |
| `crates/reprise-core/src/visuals/modes/{grid,bars,rings,flow,pulse,particles,neon,tunnel}.rs` | one mode each, tests in-file | 6 (bars), 11–17 |
| `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs` | widget shell: engine host, tick loop, picker | 7 |
| `crates/reprise-gnome/src/ui/now_playing/song_visualizer/render.rs` (new) | Cairo `Scene` renderer (Solid/HGradient/glow/dash) | 7 |
| `crates/reprise-gnome/src/ui/now_playing/song_visualizer/fullscreen.rs` (new) | fullscreen chrome per design | 9, 10 |
| `crates/reprise-gnome/src/ui/now_playing/song_visualizer_tests.rs` | widget-level tests + PPM gallery | 7, 18 |
| `crates/reprise-gnome/src/ui/strings_audio_analysis.rs` | labels + chrome strings | 7, 9 |
| `crates/reprise-gnome/src/ui/now_playing/now_playing.rs` + playback/window wiring files | position/cover/hooks plumbing | 8 |

Deleted along the way: `crates/reprise-gnome/src/ui/now_playing/song_visualizer/impact.rs` (moves to core, Task 5).

## Parallel Lanes

- Tasks 1–7 sequential (each builds on the previous).
- **Lane FS:** Tasks 8 → 9 → 10 (owns all gnome files).
- **Lanes M1–M3:** Tasks 11–17 (own exactly their core mode file; tests in-file). M1: 11 (grid, includes water tuning). M2: 12, 13, 14 (rings, flow, pulse). M3: 15, 16, 17 (particles, neon, tunnel).
- Task 18 last, single lane.

---

### Task 1: Core — log-spaced display bands

**Files:** Modify `crates/reprise-core/src/playback.rs` (constants ~line 76; add `log_band_edges` near `ema_coeff`).

**Interfaces:** Produces `pub const SPECTRUM_ANALYSIS_BAND_COUNT: usize = 256` (raw FFT bins), `pub const SPECTRUM_BAND_COUNT: usize = 64` (display bands), `pub const SPECTRUM_INTERVAL_MS: u64 = 16`, private `fn log_band_edges() -> [usize; SPECTRUM_BAND_COUNT + 1]`.

- [ ] **Step 1: Failing tests** (append in `mod spectrum_analyzer_tests`)

```rust
#[test]
fn log_band_edges_cover_every_raw_bin_exactly_once() {
    let edges = log_band_edges();
    assert_eq!(edges[0], 0);
    assert_eq!(edges[SPECTRUM_BAND_COUNT], SPECTRUM_ANALYSIS_BAND_COUNT);
    for band in 0..SPECTRUM_BAND_COUNT {
        assert!(edges[band] < edges[band + 1], "band {band} empty or non-monotonic");
    }
}

#[test]
fn log_band_edges_keep_bass_resolution_and_widen_highs() {
    let edges = log_band_edges();
    assert_eq!(edges[1] - edges[0], 1);
    assert_eq!(edges[2] - edges[1], 1);
    assert!(edges[SPECTRUM_BAND_COUNT] - edges[SPECTRUM_BAND_COUNT - 1] >= 8);
}
```

- [ ] **Step 2: RED** — `cargo test -p reprise-core --lib log_band_edges` → `cannot find function`.
- [ ] **Step 3: Implement**

```rust
/// Raw FFT band count requested from the platform analyzer. Linear in
/// frequency (an FFT property) — [`SpectrumAnalyzer`] folds these into
/// [`SPECTRUM_BAND_COUNT`] log-spaced display bands before anything reaches a
/// frontend.
pub const SPECTRUM_ANALYSIS_BAND_COUNT: usize = 256;
/// Log-spaced display bands carried by [`SpectrumFrame`].
pub const SPECTRUM_BAND_COUNT: usize = 64;
/// Target interval between spectrum messages: 16 ms (~60 Hz) matches the
/// display refresh. Envelope time constants derive from it.
pub const SPECTRUM_INTERVAL_MS: u64 = 16;
const SPECTRUM_FLOOR_DB: f32 = -80.0;

/// Raw-bin edges of the log-spaced display bands: band `d` folds raw bins
/// `edges[d]..edges[d+1]`. Strictly increasing, complete. Low bands map 1:1
/// (kick sub alone in band 0); high bands widen geometrically.
fn log_band_edges() -> [usize; SPECTRUM_BAND_COUNT + 1] {
    let mut edges = [0usize; SPECTRUM_BAND_COUNT + 1];
    let ratio = (SPECTRUM_ANALYSIS_BAND_COUNT as f32).powf(1.0 / SPECTRUM_BAND_COUNT as f32);
    let mut geometric = 1.0_f32;
    for band in 1..SPECTRUM_BAND_COUNT {
        geometric *= ratio;
        edges[band] = (geometric.round() as usize)
            .max(edges[band - 1] + 1)
            .min(SPECTRUM_ANALYSIS_BAND_COUNT - (SPECTRUM_BAND_COUNT - band));
    }
    edges[SPECTRUM_BAND_COUNT] = SPECTRUM_ANALYSIS_BAND_COUNT;
    edges
}
```

The tree is only green again after Task 2 (analyzer signature) — do not push between Tasks 1 and 2; run only the two new tests here.

- [ ] **Step 4:** `cargo test -p reprise-core --lib log_band_edges 2>&1 | tail -3` → both PASS.

---

### Task 2: Core — AGC + gamma, `ingest` takes raw bins

**Files:** Modify `crates/reprise-core/src/playback.rs` (analyzer + tests).

**Interfaces:** Produces `pub fn SpectrumAnalyzer::ingest(&mut self, decibels: [f32; SPECTRUM_ANALYSIS_BAND_COUNT]) -> SpectrumFrame`. `bands()` = post-AGC display values; `level`/`bass`/`beat`/`dynamics` computed pre-AGC. `SpectrumFrame::from_decibels` keeps its display-sized neutral-constructor signature.

- [ ] **Step 1: Rewrite `mod spectrum_analyzer_tests`** (keep Task-1 tests):

```rust
const SILENCE: [f32; SPECTRUM_ANALYSIS_BAND_COUNT] = [SPECTRUM_FLOOR_DB; SPECTRUM_ANALYSIS_BAND_COUNT];
const FULL: [f32; SPECTRUM_ANALYSIS_BAND_COUNT] = [0.0; SPECTRUM_ANALYSIS_BAND_COUNT];

fn ingest_n(
    analyzer: &mut SpectrumAnalyzer,
    db: [f32; SPECTRUM_ANALYSIS_BAND_COUNT],
    n: usize,
) -> SpectrumFrame {
    let mut frame = analyzer.ingest(db);
    for _ in 1..n {
        frame = analyzer.ingest(db);
    }
    frame
}

#[test]
fn constant_input_settles_without_beats_and_tracks_level() {
    let mut analyzer = SpectrumAnalyzer::new();
    let moderate = [-20.0_f32; SPECTRUM_ANALYSIS_BAND_COUNT]; // pre-AGC 0.75
    let frame = ingest_n(&mut analyzer, moderate, 60);
    assert!(!frame.beat().fired);
    assert!((frame.level() - 0.75).abs() < 0.05, "level pre-AGC, got {}", frame.level());
    assert!(frame.bands().iter().all(|&band| band > 0.95), "AGC → full range");
}

#[test]
fn agc_preserves_contrast_when_the_music_gets_quieter() {
    let mut analyzer = SpectrumAnalyzer::new();
    ingest_n(&mut analyzer, [-20.0; SPECTRUM_ANALYSIS_BAND_COUNT], 60);
    let quiet = ingest_n(&mut analyzer, [-40.0; SPECTRUM_ANALYSIS_BAND_COUNT], 3);
    assert!(
        quiet.bands().iter().all(|&band| (0.30..=0.75).contains(&band)),
        "expected visible contrast, got {:?}",
        &quiet.bands()[..4]
    );
}

#[test]
fn silence_stays_at_rest() {
    let mut analyzer = SpectrumAnalyzer::new();
    let frame = ingest_n(&mut analyzer, SILENCE, 40);
    assert!(frame.bands().iter().all(|&band| band == 0.0));
    assert_eq!(frame.level(), 0.0);
}

#[test]
fn impulse_after_silence_fires_beat_with_instant_attack() {
    let mut analyzer = SpectrumAnalyzer::new();
    ingest_n(&mut analyzer, SILENCE, 20);
    let hit = analyzer.ingest(FULL);
    assert!(hit.beat().fired);
    assert!(hit.beat().strength > 0.0);
    assert!(hit.level() > 0.9);
}

#[test]
fn level_releases_gradually_after_impulse() {
    let mut analyzer = SpectrumAnalyzer::new();
    ingest_n(&mut analyzer, SILENCE, 20);
    let hit = analyzer.ingest(FULL);
    let after = analyzer.ingest(SILENCE);
    assert!(after.level() < hit.level());
    assert!(after.level() > 0.1);
}

#[test]
fn silence_then_sustained_loud_spikes_dynamics() {
    let mut analyzer = SpectrumAnalyzer::new();
    ingest_n(&mut analyzer, SILENCE, 40);
    let frame = ingest_n(&mut analyzer, FULL, 4);
    assert!(frame.dynamics() > 0.3, "got {}", frame.dynamics());
}

#[test]
fn slow_ramp_does_not_produce_a_beat_storm() {
    let mut analyzer = SpectrumAnalyzer::new();
    let mut beats = 0;
    for step in 0..64 {
        let db = [-80.0_f32 + step as f32; SPECTRUM_ANALYSIS_BAND_COUNT];
        if analyzer.ingest(db).beat().fired {
            beats += 1;
        }
    }
    assert!(beats <= 3, "fired {beats}");
}

#[test]
fn all_outputs_stay_finite_and_bounded() {
    let mut analyzer = SpectrumAnalyzer::new();
    for step in 0..200 {
        let db = [-80.0_f32 + (step % 80) as f32; SPECTRUM_ANALYSIS_BAND_COUNT];
        let frame = analyzer.ingest(db);
        assert!(frame.bands().iter().all(|b| b.is_finite() && (0.0..=1.0).contains(b)));
        assert!((0.0..=1.0).contains(&frame.level()));
        assert!((0.0..=1.0).contains(&frame.bass()));
        assert!((0.0..=1.0).contains(&frame.beat().strength));
        assert!((-1.0..=1.0).contains(&frame.dynamics()));
    }
}
```

- [ ] **Step 2: RED** — signature/array-size compile errors.
- [ ] **Step 3: Implement**

(a) `normalize_bands` → generic:

```rust
fn normalize_db<const N: usize>(decibels: [f32; N]) -> [f32; N] {
    decibels.map(|value| {
        if !value.is_finite() {
            return 0.0;
        }
        ((value - SPECTRUM_FLOOR_DB) / -SPECTRUM_FLOOR_DB).clamp(0.0, 1.0)
    })
}
```

`from_decibels` calls `normalize_db`; doc: "display-resolution decibels, no log fold, no auto-gain".

(b) Constants after `BEAT_STRENGTH_OVERSHOOT`:

```rust
/// Per-band auto-gain: each display band slowly tracks its own recent maximum
/// and is normalized against it, so every band uses the full visual range.
const AGC_HALF_LIFE_MS: f32 = 8000.0;
/// Auto-gain never amplifies below this reference (silence stays at rest).
const AGC_FLOOR: f32 = 0.10;
/// Contrast curve applied to the auto-gained display value.
const DISPLAY_GAMMA: f32 = 1.4;
```

(c) Struct gains `edges: [usize; SPECTRUM_BAND_COUNT + 1]`, `agc: [f32; SPECTRUM_BAND_COUNT]`, `agc_decay: f32`; `new()` adds:

```rust
edges: log_band_edges(),
agc: [AGC_FLOOR; SPECTRUM_BAND_COUNT],
agc_decay: 0.5_f32.powf(SPECTRUM_INTERVAL_MS as f32 / AGC_HALF_LIFE_MS),
```

(d) `ingest`:

```rust
pub fn ingest(&mut self, decibels: [f32; SPECTRUM_ANALYSIS_BAND_COUNT]) -> SpectrumFrame {
    let raw = normalize_db(decibels);
    let mut folded = [0.0_f32; SPECTRUM_BAND_COUNT];
    for band in 0..SPECTRUM_BAND_COUNT {
        folded[band] = (self.edges[band]..self.edges[band + 1])
            .map(|bin| raw[bin])
            .fold(0.0_f32, f32::max);
    }
    let overall = mean(&folded, 0..SPECTRUM_BAND_COUNT);
    let bass_input = mean(&folded, 0..BASS_BAND_COUNT);
    let level = envelope(self.level_env, overall, self.level_release);
    self.level_env = level;
    let bass = envelope(self.bass_env, bass_input, self.bass_release);
    self.bass_env = bass;
    let beat = self.detect_beat(&folded);
    self.prev_bands = folded;
    let dynamics = self.detect_dynamics(overall);

    let mut bands = [0.0_f32; SPECTRUM_BAND_COUNT];
    for band in 0..SPECTRUM_BAND_COUNT {
        self.agc[band] = (self.agc[band] * self.agc_decay).max(folded[band]).max(AGC_FLOOR);
        bands[band] = (folded[band] / self.agc[band]).clamp(0.0, 1.0).powf(DISPLAY_GAMMA);
    }
    SpectrumFrame { bands, level, bass, beat, dynamics }
}
```

- [ ] **Step 4:** `cargo test -p reprise-core --lib "playback::" 2>&1 | tail -3` → all PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(core): log-spaced display bands with per-band auto-gain"`

---

### Task 3: Platform — 256-band element + parse

**Files:** `crates/reprise-platform-linux/src/player_effects.rs:33-44`, `crates/reprise-platform-linux/src/player.rs:20-40`, `crates/reprise-platform-linux/src/player/tests.rs`.

**Interfaces:** `spectrum_decibels_from_structure` returns `Option<[f32; SPECTRUM_ANALYSIS_BAND_COUNT]>`; element `bands` property = `SPECTRUM_ANALYSIS_BAND_COUNT`.

- [ ] **Step 1: Update tests** — in `ac_10_audio_filter_contains_a_disabled_bounded_spectrum_analyzer`, assert `bands == SPECTRUM_ANALYSIS_BAND_COUNT as u32`. Replace `ac_10_spectrum_messages_project_exactly_one_bounded_frame`:

```rust
#[test]
fn ac_10_spectrum_messages_project_exactly_one_bounded_frame() {
    gst::init().unwrap();
    let magnitudes = gst::List::new(
        (0..reprise_core::playback::SPECTRUM_ANALYSIS_BAND_COUNT)
            .map(|index| -80.0_f32 + (index % 80) as f32),
    );
    let structure = gst::Structure::builder("spectrum")
        .field("magnitude", magnitudes)
        .build();
    let decibels = spectrum_decibels_from_structure(&structure).expect("valid spectrum frame");
    let frame = reprise_core::playback::SpectrumAnalyzer::new().ingest(decibels);
    assert_eq!(frame.bands().len(), reprise_core::playback::SPECTRUM_BAND_COUNT);
    assert!(frame.bands().iter().all(|b| b.is_finite() && (0.0..=1.0).contains(b)));
    assert!(spectrum_decibels_from_structure(&gst::Structure::new_empty("other")).is_none());
}
```

- [ ] **Step 2: RED** — array-size compile error.
- [ ] **Step 3: Implement** — `player.rs` imports `SPECTRUM_ANALYSIS_BAND_COUNT` (drop the display-count import) and:

```rust
pub(super) fn spectrum_decibels_from_structure(
    structure: &gst::StructureRef,
) -> Option<[f32; SPECTRUM_ANALYSIS_BAND_COUNT]> {
    if structure.name() != "spectrum" {
        return None;
    }
    let magnitudes = structure.get::<gst::List>("magnitude").ok()?;
    if magnitudes.len() != SPECTRUM_ANALYSIS_BAND_COUNT {
        return None;
    }
    let mut decibels = [0.0_f32; SPECTRUM_ANALYSIS_BAND_COUNT];
    for (slot, magnitude) in decibels.iter_mut().zip(magnitudes.iter()) {
        *slot = magnitude.get::<f32>().ok()?;
    }
    Some(decibels)
}
```

`player_effects.rs` bands property → `SPECTRUM_ANALYSIS_BAND_COUNT`.

- [ ] **Step 4:** `cargo test -p reprise-platform-linux 2>&1 | tail -3` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(platform): request 256 spectrum bands and feed raw bins to the analyzer"`

---

### Task 4: Core visuals — scene model + color

**Files:**
- Create: `crates/reprise-core/src/visuals.rs` (`pub mod scene; pub mod color; …` + re-exports)
- Create: `crates/reprise-core/src/visuals/scene.rs`, `crates/reprise-core/src/visuals/color.rs`
- Modify: `crates/reprise-core/src/lib.rs` (add `pub mod visuals;` in the alphabetical list, after `view_source`)

**Interfaces (all `pub`):**

```rust
// scene.rs
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba { pub r: f32, pub g: f32, pub b: f32, pub a: f32 }
#[derive(Clone, Debug)]
pub enum Fill {
    Solid(Rgba),
    /// Horizontal gradient across x0..x1 with explicit stops (offset 0..1).
    HGradient { x0: f32, x1: f32, stops: Vec<(f32, Rgba)> },
}
#[derive(Clone, Debug)]
pub enum Geom {
    Polyline { points: Vec<(f32, f32)>, closed: bool },
    Arc { cx: f32, cy: f32, r: f32, a0: f32, a1: f32 },
    Disc { cx: f32, cy: f32, r: f32 },
    Rect { x: f32, y: f32, w: f32, h: f32 },
    /// Filled radial gradient: fill color at center → transparent at r.
    RadialGlow { cx: f32, cy: f32, r: f32 },
}
#[derive(Clone, Debug)]
pub struct Shape {
    pub geom: Geom,
    pub fill: Fill,
    /// Stroke width; 0.0 = filled (Disc/Rect/RadialGlow always filled).
    pub width: f32,
    /// 0..=1: renderers fake bloom (wide translucent under-stroke ×3 width).
    pub glow: f32,
    pub dash: Option<(f32, f32)>,
}
#[derive(Clone, Debug, Default)]
pub struct Scene { pub shapes: Vec<Shape> }
impl Scene {
    /// All coordinates finite and within ±4×max(w,h); widths ≥ 0; glow 0..=1.
    pub fn is_finite_and_sane(&self, width: f32, height: f32) -> bool;
}

// color.rs
pub fn hsla_to_rgb(hue: f32, sat: f32, light: f32) -> (f32, f32, f32);
pub fn rgb_hue(rgb: (f32, f32, f32)) -> f32;          // 0..360; 250.0 for gray
pub fn hue_shift(rgb: (f32, f32, f32), degrees: f32) -> (f32, f32, f32);
/// Second-most-dominant hue from RGBA pixels (12×30° buckets, weight
/// sat^1.6·v², ≥2 buckets from `primary`'s hue, ≥16 % of the top bucket,
/// brightness normalized to 208/255). None when the cover has no distinct
/// second color.
pub fn secondary_accent(
    rgba: &[u8],
    pixel_count: usize,
    primary: (f32, f32, f32),
) -> Option<(f32, f32, f32)>;
```

- [ ] **Step 1: Failing tests** (in each file's `#[cfg(test)] mod tests`)

```rust
// color.rs tests
#[test]
fn hsla_roundtrip_and_hue_shift() {
    for hue in [0.0_f32, 60.0, 120.0, 200.0, 300.0] {
        let rgb = hsla_to_rgb(hue, 0.85, 0.6);
        let back = rgb_hue(rgb);
        let delta = (back - hue).abs().min(360.0 - (back - hue).abs());
        assert!(delta < 2.0, "hue {hue} → {back}");
        let shifted = rgb_hue(hue_shift(rgb, 42.0));
        let want = (hue + 42.0) % 360.0;
        let delta = (shifted - want).abs().min(360.0 - (shifted - want).abs());
        assert!(delta < 3.0, "shift {hue} → {shifted}, want {want}");
    }
}

#[test]
fn secondary_accent_finds_a_distinct_second_hue() {
    // Half saturated red pixels, half saturated cyan-blue: primary red → secondary ≈ blue.
    let mut rgba = Vec::new();
    for _ in 0..64 { rgba.extend_from_slice(&[220, 30, 30, 255]); }
    for _ in 0..40 { rgba.extend_from_slice(&[30, 120, 220, 255]); }
    let secondary = secondary_accent(&rgba, 104, (0.86, 0.12, 0.12)).expect("distinct hue");
    let hue = rgb_hue(secondary);
    assert!((170.0..=250.0).contains(&hue), "got {hue}");
}

#[test]
fn secondary_accent_none_for_monochrome_covers() {
    let mut rgba = Vec::new();
    for _ in 0..100 { rgba.extend_from_slice(&[200, 40, 40, 255]); }
    assert!(secondary_accent(&rgba, 100, (0.78, 0.16, 0.16)).is_none());
}

// scene.rs tests
#[test]
fn sanity_accepts_bounded_and_rejects_nan() {
    let ok = Shape {
        geom: Geom::Disc { cx: 10.0, cy: 10.0, r: 3.0 },
        fill: Fill::Solid(Rgba { r: 1.0, g: 1.0, b: 1.0, a: 0.5 }),
        width: 0.0, glow: 0.0, dash: None,
    };
    assert!(Scene { shapes: vec![ok.clone()] }.is_finite_and_sane(100.0, 100.0));
    let mut bad = ok;
    bad.geom = Geom::Disc { cx: f32::NAN, cy: 10.0, r: 3.0 };
    assert!(!Scene { shapes: vec![bad] }.is_finite_and_sane(100.0, 100.0));
}
```

- [ ] **Step 2: RED** — `cargo test -p reprise-core --lib visuals 2>&1 | rg error | head -3`.
- [ ] **Step 3: Implement.** `hsla_to_rgb`/`rgb_hue` standard formulas (port from the plan header of paint.rs in git history is NOT available — write fresh):

```rust
pub fn hsla_to_rgb(hue: f32, sat: f32, light: f32) -> (f32, f32, f32) {
    let hue = hue.rem_euclid(360.0);
    let c = (1.0 - (2.0 * light - 1.0).abs()) * sat;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = light - c / 2.0;
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

pub fn rgb_hue(rgb: (f32, f32, f32)) -> f32 {
    let (r, g, b) = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta < 0.001 {
        return 250.0;
    }
    let hue = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } * 60.0;
    hue.rem_euclid(360.0)
}

pub fn hue_shift(rgb: (f32, f32, f32), degrees: f32) -> (f32, f32, f32) {
    let (r, g, b) = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let light = (max + min) / 2.0;
    let sat = if max - min < 0.001 {
        0.0
    } else {
        (max - min) / (1.0 - (2.0 * light - 1.0).abs())
    };
    hsla_to_rgb(rgb_hue(rgb) + degrees, sat, light)
}

pub fn secondary_accent(
    rgba: &[u8],
    pixel_count: usize,
    primary: (f32, f32, f32),
) -> Option<(f32, f32, f32)> {
    #[derive(Default, Clone, Copy)]
    struct Bucket { w: f32, r: f32, g: f32, b: f32 }
    let mut buckets = [Bucket::default(); 12];
    for pixel in 0..pixel_count.min(rgba.len() / 4) {
        let o = pixel * 4;
        let (r, g, b) = (f32::from(rgba[o]), f32::from(rgba[o + 1]), f32::from(rgba[o + 2]));
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
        let value = max / 255.0;
        let weight = sat.powf(1.6) * value * value;
        if weight < 0.01 {
            continue;
        }
        let hue = rgb_hue((r / 255.0, g / 255.0, b / 255.0));
        let bucket = &mut buckets[((hue / 30.0) as usize).min(11)];
        bucket.w += weight;
        bucket.r += r * weight;
        bucket.g += g * weight;
        bucket.b += b * weight;
    }
    let primary_bucket = ((rgb_hue(primary) / 30.0) as usize).min(11);
    let top_weight = buckets.iter().map(|b| b.w).fold(0.0_f32, f32::max);
    if top_weight < 0.5 {
        return None;
    }
    let mut order: Vec<usize> = (0..12).filter(|&i| buckets[i].w > 0.0).collect();
    order.sort_by(|&a, &b| buckets[b].w.total_cmp(&buckets[a].w));
    order
        .into_iter()
        .find(|&i| {
            let distance = (i as i32 - primary_bucket as i32).unsigned_abs() as usize;
            distance.min(12 - distance) >= 2 && buckets[i].w >= top_weight * 0.16
        })
        .map(|i| {
            let bucket = buckets[i];
            let (r, g, b) = (bucket.r / bucket.w, bucket.g / bucket.w, bucket.b / bucket.w);
            let max = r.max(g).max(b);
            let k = if max > 0.0 { 208.0 / max } else { 1.0 };
            ((r * k).min(255.0) / 255.0, (g * k).min(255.0) / 255.0, (b * k).min(255.0) / 255.0)
        })
}
```

`Scene::is_finite_and_sane` mirrors the test contract (bound = `4.0 * width.max(height)`), checking every geometry variant plus `width >= 0`, `(0.0..=1.0).contains(&glow)`, and every `Fill` color component finite.

- [ ] **Step 4:** `cargo test -p reprise-core --lib visuals 2>&1 | tail -3` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(core): portable visual scene model with dual-accent color tools"`

---

### Task 5: Core visuals — water, impact, dust

**Files:**
- Create: `crates/reprise-core/src/visuals/water.rs`, `crates/reprise-core/src/visuals/impact.rs`, `crates/reprise-core/src/visuals/dust.rs` (+ `pub mod` lines in `visuals.rs`)
- Delete (moved): `crates/reprise-gnome/src/ui/now_playing/song_visualizer/impact.rs` — the gnome build keeps compiling because Task 7 rewires it; until then leave the gnome copy untouched and only ADD the core files (the delete happens in Task 7).

**Interfaces:**

```rust
// water.rs — the Grid mode's surface (design revision: "Wasserfläche + Trampolin")
pub const WATER_ROWS: usize = 26;
pub const WATER_COLS: usize = 44;
pub struct WaterGrid { /* h, v: [f32; ROWS*COLS], rng: u32 */ }
impl WaterGrid {
    pub fn new() -> Self;
    /// One 60 Hz step: far row driven by the mirrored display bands, spring
    /// coupling (30), restoring force (4.5), damping exp(-dt·1.7), h clamped
    /// to -0.9..=2.2. dt fixed at 1/60.
    pub fn advance(&mut self, bands: &[f32; SPECTRUM_BAND_COUNT]);
    /// Beat: 2–3 random Gaussian splashes, power (3.5..6.5)·(0.45+level).
    pub fn splash(&mut self, level: f32);
    pub fn height(&self, row: usize, col: usize) -> f32;
    pub fn reset(&mut self);
    pub fn is_still(&self) -> bool;    // all |h|,|v| < 0.01
}

// impact.rs — ported 1:1 from the gnome module (same constants/behavior),
// plus the kick envelope moves here:
pub struct ImpactState { /* shockwaves, sparks, flash, accent_boost, kick */ }
impl ImpactState {
    pub fn new() -> Self;
    pub fn spawn_beat(&mut self, strength: f32);   // shockwave + sparks + kick=max(kick,0.6+0.4·strength)
    pub fn spawn_drop(&mut self, dynamics: f32);   // threshold 0.35 → flash + big shockwave
    pub fn advance(&mut self);                     // + kick *= 0.90
    pub fn is_idle(&self) -> bool;
    pub fn kick(&self) -> f32;
    pub fn flash(&self) -> f32;
    pub fn shockwaves(&self) -> impl Iterator<Item = ShockwaveDraw> + '_;
    pub fn particles(&self) -> impl Iterator<Item = ParticleDraw> + '_;
}
pub struct ShockwaveDraw { pub progress: f32, pub strength: f32 }
pub struct ParticleDraw { pub angle: f32, pub dist: f32, pub life_frac: f32 }

// dust.rs
pub const DUST_COUNT: usize = 120;
#[derive(Clone, Copy)]
pub struct Dust { pub nx: f32, pub ny: f32, pub r: f32, pub a: f32, pub tw: f32, pub ph: f32, /* dx, dy private */ }
pub fn make_dust() -> [Dust; DUST_COUNT];          // deterministic xorshift seed
pub fn advance_dust(dust: &mut [Dust; DUST_COUNT], level: f32);   // dt 1/60, wraps at edges
pub(crate) fn xorshift(state: &mut u32) -> f32;    // shared rng helper (0..1)
```

- [ ] **Step 1: Failing tests**

```rust
// water.rs
#[test]
fn water_settles_flat_without_input() {
    let mut water = WaterGrid::new();
    water.splash(1.0);
    let silent = [0.0_f32; SPECTRUM_BAND_COUNT];
    for _ in 0..2000 { water.advance(&silent); }
    assert!(water.is_still(), "waves must damp out");
}

#[test]
fn splash_raises_the_surface_and_stays_bounded() {
    let mut water = WaterGrid::new();
    for _ in 0..8 { water.splash(1.0); }
    let bands = [1.0_f32; SPECTRUM_BAND_COUNT];
    let mut peak = 0.0_f32;
    for _ in 0..600 {
        water.advance(&bands);
        for row in 0..WATER_ROWS {
            for col in 0..WATER_COLS {
                let height = water.height(row, col);
                assert!(height.is_finite() && (-0.9..=2.2).contains(&height));
                peak = peak.max(height);
            }
        }
    }
    assert!(peak > 0.5, "driven surface must actually move, peaked at {peak}");
}

// impact.rs — port the three existing gnome tests verbatim (beat storm capacity,
// decay to rest, drop threshold noop) plus:
#[test]
fn kick_envelope_rises_on_beat_and_decays() {
    let mut impact = ImpactState::new();
    impact.spawn_beat(1.0);
    let peak = impact.kick();
    assert!(peak >= 0.9);
    for _ in 0..60 { impact.advance(); }
    assert!(impact.kick() < 0.05);
}

// dust.rs
#[test]
fn dust_stays_in_unit_box_forever() {
    let mut dust = make_dust();
    for _ in 0..5000 { advance_dust(&mut dust, 1.0); }
    assert!(dust.iter().all(|p| (-0.05..=1.05).contains(&p.nx) && (-0.05..=1.05).contains(&p.ny)));
}
```

- [ ] **Step 2: RED**, **Step 3: Implement.** Water core loop (port of the mock's physics, fixed `DT = 1.0/60.0`):

```rust
pub fn advance(&mut self, bands: &[f32; SPECTRUM_BAND_COUNT]) {
    const DT: f32 = 1.0 / 60.0;
    let half = (WATER_COLS - 1) as f32 / 2.0;
    for col in 0..WATER_COLS {
        let f = (col as f32 - half).abs() / half;
        let drive = bands[((f * 0.775 * (SPECTRUM_BAND_COUNT - 1) as f32) as usize)
            .min(SPECTRUM_BAND_COUNT - 1)];
        self.v[col] += (drive * 1.5 - self.h[col]) * DT * 26.0;
    }
    let damp = (-DT * 1.7).exp();
    for row in 0..WATER_ROWS {
        for col in 0..WATER_COLS {
            let i = row * WATER_COLS + col;
            let up = if row > 0 { self.h[i - WATER_COLS] } else { self.h[i] };
            let down = if row < WATER_ROWS - 1 { self.h[i + WATER_COLS] } else { self.h[i] };
            let left = if col > 0 { self.h[i - 1] } else { self.h[i] };
            let right = if col < WATER_COLS - 1 { self.h[i + 1] } else { self.h[i] };
            self.v[i] += ((up + down + left + right - 4.0 * self.h[i]) * 30.0 - self.h[i] * 4.5) * DT;
            self.v[i] *= damp;
        }
    }
    for i in 0..self.h.len() {
        self.h[i] = (self.h[i] + self.v[i] * DT).clamp(-0.9, 2.2);
    }
}

pub fn splash(&mut self, level: f32) {
    let count = 2 + (xorshift(&mut self.rng) * 2.0) as usize;
    for _ in 0..count {
        let col = (WATER_COLS as f32 * (0.2 + xorshift(&mut self.rng) * 0.6)) as usize;
        let row = (WATER_ROWS as f32 * (0.25 + xorshift(&mut self.rng) * 0.55)) as usize;
        let power = (3.5 + xorshift(&mut self.rng) * 3.0) * (0.45 + level);
        for r in row.saturating_sub(3)..(row + 4).min(WATER_ROWS) {
            for c in col.saturating_sub(3)..(col + 4).min(WATER_COLS) {
                let dr = r as f32 - row as f32;
                let dc = c as f32 - col as f32;
                self.v[r * WATER_COLS + c] += (-(dr * dr + dc * dc) / 4.0).exp() * power;
            }
        }
    }
}
```

Impact: copy the gnome `impact.rs` body, adjust to `f32`, add the `kick` field (`spawn_beat` sets `self.kick = self.kick.max(0.6 + 0.4 * strength)`, `advance` multiplies by 0.90, floor to 0 under 0.01, `is_idle` includes `kick == 0.0`). Dust: port from the plan's earlier gnome version (identical math, `f32`).

- [ ] **Step 4:** `cargo test -p reprise-core --lib visuals 2>&1 | tail -3` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(core): water surface, impact pools and dust field for visuals"`

---

### Task 6: Core visuals — `VisualEngine` + Bars mode

**Files:**
- Create: `crates/reprise-core/src/visuals/engine.rs`, `crates/reprise-core/src/visuals/modes.rs`, `crates/reprise-core/src/visuals/modes/bars.rs` + 7 stub mode files (each `pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> { super::bars::scene(ctx) }`)

**Interfaces:**

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VisualMode { Grid, #[default] Bars, Rings, Flow, Pulse, Particles, Neon, Tunnel }
impl VisualMode {
    pub const ALL: [Self; 8];
    pub fn id(self) -> &'static str;   // "grid","bars","rings","flow","pulse","particles","neon","tunnel"
}

pub struct ModeCtx<'a> {
    pub bands: &'a [f32; SPECTRUM_BAND_COUNT],   // UI-smoothed, post-AGC
    pub peaks: &'a [f32; SPECTRUM_BAND_COUNT],
    pub level: f32, pub bass: f32, pub mid: f32, pub high: f32,
    pub kick: f32, pub clock: f32,
    pub accent: (f32, f32, f32), pub accent2: (f32, f32, f32),
    pub water: &'a WaterGrid,
    pub dust: &'a [Dust; DUST_COUNT],
    pub impact: &'a ImpactState,
    pub width: f32, pub height: f32,
}
impl ModeCtx<'_> {
    pub fn band(&self, f: f32) -> f32;                        // fraction sample
    pub fn accent_fill(&self, alpha: f32) -> Fill;            // Solid
    pub fn accent2_fill(&self, alpha: f32) -> Fill;
    pub fn hsla_fill(&self, hue: f32, sat: f32, light: f32, alpha: f32) -> Fill;
    /// Design's neon gradient: hue−70 → hue+70 across x0..x1, ends fade to 0.
    pub fn hue_sweep_fill(&self, hue: f32, alpha: f32, x0: f32, x1: f32) -> Fill;
}

pub struct VisualEngine { /* mode, current/target/peaks bands, static profile,
                             level/mid/high envelopes + targets, playing,
                             clock, water, dust, impact, accent, cover_accent2 */ }
impl VisualEngine {
    pub fn new() -> Self;
    pub fn set_mode(&mut self, mode: VisualMode);
    pub fn mode(&self) -> VisualMode;
    pub fn set_playing(&mut self, playing: bool);             // pauses drift, retargets static profile
    pub fn set_accent(&mut self, rgb: (f32, f32, f32));       // app cover accent (primary)
    pub fn set_cover_pixels(&mut self, rgba: &[u8], pixel_count: usize);  // → secondary_accent
    pub fn clear_cover(&mut self);
    pub fn set_static_profile(&mut self, dimensions: &[u8; 4]);
    pub fn clear_static_profile(&mut self);
    pub fn note_track_changed(&mut self);                     // resets clock/water/impact
    pub fn ingest(&mut self, frame: &SpectrumFrame);          // targets + beat/drop side effects
    /// One 60 Hz step; true = fully settled (frontend may stop ticking).
    pub fn tick(&mut self) -> bool;
    /// Snap to rest instantly (reduced-motion frontends).
    pub fn snap_to_static(&mut self);
    pub fn scene(&self, width: f32, height: f32) -> Scene;    // wash + mode shapes
}
```

**Behavior (exact):**
- Easing: attack `0.9` for bands and scalars; release per band index `0.07 + 0.08 * (index / 63)` (bass lingers, highs sparkle — design revision); scalar release `0.16`. Peaks: instant rise, `-0.018`/tick fall.
- `ingest`: `target = *frame.bands()`, `level_target = frame.level()`; on `beat.fired`: `impact.spawn_beat(strength)` + `water.splash(level)`; always `impact.spawn_drop(frame.dynamics())`.
- `tick`: ease bands/level; derive `mid`/`high` (instant-up envelope over `mean(bands[20..44])` / `mean(bands[44..64])`, release 0.16); `impact.advance()`; `water.advance(&current)`; `advance_dust(level)`; `clock += 1/60` while playing; settled = eased-in targets AND `impact.is_idle()` AND `water.is_still()` AND NOT playing.
- `scene`: first shape = accent wash `RadialGlow { cx: w/2, cy: h*0.44, r: max(w,h)*0.6 }` alpha `0.05 + 0.11*level`; then the mode's shapes; last (if `flash > 0`) a soft accent `RadialGlow` full-canvas, alpha capped `0.15`.
- `accent2()` = `cover_accent2.unwrap_or_else(|| hue_shift(accent, 42.0))`.

**Bars mode (`modes/bars.rs`) — final port (design revision: 64 columns, dual accent):**

```rust
pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let n = SPECTRUM_BAND_COUNT; // 64 columns, 1:1 with display bands
    let mut shapes: Vec<Shape> = (0..n)
        .map(|i| {
            let v = ctx.bands[i];
            let px = (i as f32 + 0.5) * w / n as f32;
            let len = (v * h * 0.8).max(4.0);
            let fill = if v > 0.66 {
                ctx.accent2_fill(0.3 + 0.6 * v)
            } else {
                ctx.accent_fill(0.28 + 0.62 * v)
            };
            Shape {
                geom: Geom::Polyline { points: vec![(px, h - 2.0), (px, h - len)], closed: false },
                fill,
                width: (m * 0.006).max(4.0),
                glow: v,
                dash: None,
            }
        })
        .collect();
    for spark in ctx.impact.particles() {
        shapes.push(Shape {
            geom: Geom::Disc {
                cx: w / 2.0 + spark.angle.cos() * spark.dist,
                cy: h / 2.0 + spark.angle.sin() * spark.dist,
                r: 1.4 + spark.life_frac * 2.6,
            },
            fill: ctx.accent_fill(spark.life_frac),
            width: 0.0, glow: 0.0, dash: None,
        });
    }
    shapes
}
```

- [ ] **Step 1: Failing tests** (`engine.rs` tests)

```rust
fn lively_engine() -> VisualEngine {
    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    engine.set_accent((0.2, 0.7, 0.7));
    let mut analyzer = crate::playback::SpectrumAnalyzer::new();
    // Silence then a slam: produces a beat, a kick, and full bands.
    for _ in 0..20 { engine.ingest(&analyzer.ingest([-80.0; SPECTRUM_ANALYSIS_BAND_COUNT])); engine.tick(); }
    for _ in 0..10 { engine.ingest(&analyzer.ingest([0.0; SPECTRUM_ANALYSIS_BAND_COUNT])); engine.tick(); }
    engine
}

#[test]
fn every_mode_builds_a_finite_sane_nonempty_scene() {
    let mut engine = lively_engine();
    for mode in VisualMode::ALL {
        engine.set_mode(mode);
        let scene = engine.scene(548.0, 300.0);
        assert!(scene.shapes.len() > 1, "{mode:?} must draw beyond the wash");
        assert!(scene.is_finite_and_sane(548.0, 300.0), "{mode:?}");
    }
}

#[test]
fn engine_reacts_to_a_slam_with_full_bars_and_kick() {
    let engine = lively_engine();
    let scene = engine.scene(548.0, 300.0);
    // Bars mode: with AGC + snap attack, a slam reaches large bar lengths.
    let max_len = scene.shapes.iter().filter_map(|s| match &s.geom {
        Geom::Polyline { points, .. } if points.len() == 2 => Some((points[0].1 - points[1].1).abs()),
        _ => None,
    }).fold(0.0_f32, f32::max);
    assert!(max_len > 150.0, "slam should nearly fill the canvas, got {max_len}");
}

#[test]
fn stopped_engine_settles() {
    let mut engine = lively_engine();
    engine.set_playing(false);
    engine.clear_static_profile();
    let mut settled = false;
    for _ in 0..5000 {
        if engine.tick() { settled = true; break; }
    }
    assert!(settled, "engine must come to rest after stop");
}

#[test]
fn secondary_accent_falls_back_to_hue_shift() {
    let mut engine = VisualEngine::new();
    engine.set_accent((0.8, 0.2, 0.2));
    let ctx_hue = color::rgb_hue(engine.accent2());
    let want = (color::rgb_hue((0.8, 0.2, 0.2)) + 42.0) % 360.0;
    let delta = (ctx_hue - want).abs().min(360.0 - (ctx_hue - want).abs());
    assert!(delta < 3.0);
}
```

(Expose `pub fn accent2(&self) -> (f32, f32, f32)` for the last test and the frontend.)

- [ ] **Step 2: RED**, **Step 3: Implement** engine + bars + stubs per the interface block.
- [ ] **Step 4:** `cargo test -p reprise-core --lib visuals 2>&1 | tail -3` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(core): VisualEngine with 8-mode scaffold and Bars port"`

---

### Task 7: GNOME — engine adapter, Cairo renderer, inline picker

**Files:**
- Create: `crates/reprise-gnome/src/ui/now_playing/song_visualizer/render.rs`
- Rewrite: `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs`
- Delete: `crates/reprise-gnome/src/ui/now_playing/song_visualizer/impact.rs`
- Modify: `crates/reprise-gnome/src/ui/strings_audio_analysis.rs` (8 labels), `song_visualizer_tests.rs`

**Interfaces:**
- `render.rs`: `pub(super) fn draw_scene(cr: &gtk4::cairo::Context, scene: &reprise_core::visuals::Scene)` — maps `Fill::Solid` → `set_source_rgba`, `Fill::HGradient` → `cairo::LinearGradient` with the given stops, `RadialGlow` → `cairo::RadialGradient` fill, glow → under-stroke pass at `width*3`, `alpha*glow*0.35` (approximate: re-set source with scaled alpha for Solid; for HGradient scale each stop's alpha), dash → `set_dash`.
- `song_visualizer.rs`: `SongVisualizer` now owns `engine: Rc<RefCell<VisualEngine>>`. Public panel-facing API unchanged in name: `set_profile`, `clear_profile`, `set_spectrum(frame)` (→ `engine.ingest`), `set_playback_state` (→ `set_playing` + tick management), `set_active`, `toggle_fullscreen`, `close_fullscreen`, `widget()`, plus (kept from the current tree) `set_track_meta`, transport-related methods reworked in Task 8. Tick callback: `engine.tick()` + `queue_draw`; draw func: `engine.set_accent(area color)` then `render::draw_scene(cr, &engine.scene(w, h))`. Reduced motion: on `animations_enabled() == false`, call `engine.snap_to_static()` instead of ticking (existing gating call sites stay).
- Mode picker: `gtk4::FlowBox` (max 4 per line, 2 rows) of grouped `ToggleButton`s labeled via `strings::SONG_VISUALS_MODE_*`, `set_widget_name(mode.id())`; toggling → `engine.set_mode` + queue draws. Appended under the inline canvas AND reused by the fullscreen (Task 9).
- Strings: `SONG_VISUALS_MODE_GRID = N_!("Grid")`, `_BARS = N_!("Bars")`, `_RINGS = N_!("Rings")`, `_FLOW = N_!("Flow")`, `_PULSE = N_!("Pulse")`, `_PARTICLES = N_!("Particles")`, `_NEON = N_!("Neon")`, `_TUNNEL = N_!("Tunnel")`.
- Gallery test (`#[ignore]`): loops `VisualMode::ALL`, builds a lively engine (as in core tests), renders each scene via `render::draw_scene` onto an `ImageSurface`, writes dep-free PPMs to `REPRISE_VIS_OUT` (reuse the existing PPM writer).

- [ ] **Step 1:** Rewrite tests: keep widget/a11y ignored test; replace scene/impact tests (now in core) with: css test (accent classes; updated selectors), a `mode_labels` test asserting the 8 labels, and the gallery.
- [ ] **Step 2: RED → implement → GREEN** (`cargo test -p reprise-gnome song_visualizer`).
- [ ] **Step 3:** Render gallery, eyeball Bars (only real mode yet).
- [ ] **Step 4: Commit** — `git commit -am "refactor(gnome): visualizer renders core VisualEngine scenes"`

---

### Task 8: GNOME — fullscreen plumbing (hooks, position, cover, next-up)

Identical content to the previous plan revision, with two changes:

1. `set_cover` now ALSO feeds the engine: after downscaling the cover texture to a 32×32 RGBA pixbuf (for the palette) call `engine.set_cover_pixels(pixbuf.pixels(), 1024)`; `None` → `engine.clear_cover()`. The 24 px backdrop downscale for the fake blur is produced from the same texture download.
2. `NowPlayingPanel::set_loaded_track`: when the track id changes, call `visualizer.note_track_changed()` (engine resets clock/water/impact — the design clears the water on track switch).

Everything else verbatim from the previous revision: `PlayerHooks { previous, play_pause, stop, next, seek_to_ms, set_volume, initial_volume }`; controller position callback (`player_controller.rs:302` pattern, `now_playing_wiring.rs:192` setter, `player_event_handling.rs:33` invoke site); wiring block in `window_now_playing_wiring.rs`; queue position `TRACK i / n` from `QueueViewModel.ids` + next-up title via `SELECT title FROM tracks WHERE id = ?1` on the panel's `conn` (None on any error); accepted v1 limitation: volume slider does not follow external changes.

- [ ] Implement, `cargo build -p reprise-gnome` clean, tests pass, commit `feat(visualizer): player hooks, position, cover and queue plumbing for fullscreen`.

---

### Task 9: GNOME — fullscreen chrome per design

Identical to the previous plan revision (layout tree, CSS classes, strings), unchanged except:
- The canvas draw already comes from the engine; the fullscreen canvas draw func additionally paints the dark vignette (two Cairo radial gradients: `rgba(15,16,28)` 0.45 center → 0.87 edge) BEFORE `draw_scene`.
- The mode pill row reuses Task 7's FlowBox builder.

Layout summary (unchanged): backdrop Picture (24 px fake blur, opacity 0.45) → canvas → header (timecode left; centered state label / 36 px title / artist·album) → bottom (84 px cover thumb + `TRACK i / n` + next-up; mute + 110 px volume scale right; timeCur · seek scale · timeTotal; transport prev/play/stop/next 46-60-46-46 circular; mode pills; hint). Strings and CSS blocks exactly as in the previous revision (`SONG_VISUALS_STATE_*`, `SONG_VISUALS_TRACK_POS`, `SONG_VISUALS_NEXT_UP`, `SONG_VISUALS_MUTE/VOLUME/SEEK`, `.reprise-fs-*` classes).

- [ ] Implement in `fullscreen.rs`; build clean; css test extended; commit `feat(visualizer): fullscreen chrome after the design mock`.

---

### Task 10: GNOME — keyboard + auto-hide v2

Identical to the previous plan revision: Space/←→ (±5 s)/↑↓ (volume ±0.05)/F/F11/Escape/N/P/digits 1–8 (mode select), every key wakes chrome; auto-hide 3 s only while Playing (Paused/Stopped: chrome stays, timer disarmed); pointer-leave hides immediately while playing; cursor hidden with chrome.

- [ ] Implement; `xvfb-run -a cargo test -p reprise-gnome -- --ignored ac_10_visual_widget` PASS; commit `feat(visualizer): fullscreen keyboard control and playing-only auto-hide`.

---

### Tasks 11–17: The remaining seven modes (core)

Common: replace the stub in `crates/reprise-core/src/visuals/modes/<id>.rs`; tests in-file build a `ModeCtx` via the shared `lively_engine()` helper (export `#[cfg(test)] pub fn test_ctx(...)` from `engine.rs` in Task 6); assert non-empty + `Scene::is_finite_and_sane` + the mode-specific invariants; then implement; `cargo test -p reprise-core --lib modes::<id>`; render via the gnome gallery; commit `feat(core): <id> visual mode`.

### Task 11: Grid (water surface, dual accent — design revision)

Invariants: `WATER_ROWS` gray polylines + verticals every 4 columns; accent2 crest segments appear when the water holds cells > 0.5; all y within canvas.

```rust
pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let horizon = h * 0.30;
    let near_y = h * 0.94;
    let amp = h * 0.26;
    // Project every water cell once.
    let mut rows: Vec<(Vec<(f32, f32)>, f32)> = Vec::with_capacity(WATER_ROWS); // (points, near)
    for row in 0..WATER_ROWS {
        let near = (row as f32 / (WATER_ROWS - 1) as f32).powf(1.6);
        let y0 = horizon + near * (near_y - horizon);
        let half = w * 0.30 + near * w * 0.68;
        let row_amp = amp * (0.35 + 0.65 * near);
        let points = (0..WATER_COLS)
            .map(|col| {
                let px = w / 2.0 + ((col as f32 / (WATER_COLS - 1) as f32) - 0.5) * 2.0 * half;
                (px, (y0 - ctx.water.height(row, col) * row_amp).clamp(0.0, h))
            })
            .collect();
        rows.push((points, near));
    }
    let gray = |alpha: f32| Fill::Solid(Rgba { r: 0.815, g: 0.831, b: 0.894, a: alpha });
    let mut shapes = Vec::new();
    for (points, near) in &rows {
        shapes.push(Shape {
            geom: Geom::Polyline { points: points.clone(), closed: false },
            fill: gray(0.10 + 0.40 * near),
            width: 1.0 + near * 0.8, glow: 0.0, dash: None,
        });
    }
    for col in (0..WATER_COLS).step_by(4) {
        shapes.push(Shape {
            geom: Geom::Polyline { points: rows.iter().map(|(p, _)| p[col]).collect(), closed: false },
            fill: gray(0.14),
            width: 1.0, glow: 0.0, dash: None,
        });
    }
    // Accent2 on crests thrown above 0.5 — open a segment per contiguous run.
    for (row, (points, near)) in rows.iter().enumerate() {
        let mut run: Vec<(f32, f32)> = Vec::new();
        for col in 0..WATER_COLS {
            if ctx.water.height(row, col) > 0.5 {
                run.push(points[col]);
            } else if run.len() >= 2 {
                shapes.push(crest(std::mem::take(&mut run), *near, ctx));
            } else {
                run.clear();
            }
        }
        if run.len() >= 2 {
            shapes.push(crest(run, *near, ctx));
        }
    }
    shapes
}

fn crest(points: Vec<(f32, f32)>, near: f32, ctx: &ModeCtx) -> Shape {
    Shape {
        geom: Geom::Polyline { points, closed: false },
        fill: ctx.accent2_fill(0.28 + 0.5 * near),
        width: 1.7, glow: 0.8, dash: None,
    }
}
```

### Task 12: Rings

Invariants: 7 band rings + shockwaves (accent2) + core RadialGlow.

```rust
pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let (cx, cy) = (w / 2.0, h * 0.46);
    let mut shapes = Vec::new();
    for wave in ctx.impact.shockwaves() {
        shapes.push(Shape {
            geom: Geom::Arc { cx, cy, r: m * 0.12 + wave.progress * m * 0.55, a0: 0.0, a1: TAU },
            fill: ctx.accent2_fill((1.0 - wave.progress) * wave.strength * 0.5),
            width: 1.5, glow: 0.0, dash: None,
        });
    }
    for i in 0..7 {
        let v = ctx.band(i as f32 / 7.0 * 0.625 + 0.04);
        shapes.push(Shape {
            geom: Geom::Arc { cx, cy, r: m * (0.07 + i as f32 * 0.052) + v * m * 0.075, a0: 0.0, a1: TAU },
            fill: ctx.accent_fill(0.14 + 0.6 * v),
            width: 2.0 + v * 2.5, glow: v * 0.6, dash: None,
        });
    }
    shapes.push(Shape {
        geom: Geom::RadialGlow { cx, cy, r: m * 0.07 + ctx.level * m * 0.05 },
        fill: ctx.accent_fill(0.5 + 0.4 * ctx.kick),
        width: 0.0, glow: 0.0, dash: None,
    });
    shapes
}
```

### Task 13: Flow

Invariants: 3 trails spanning ≥ 90 % width; middle trail (layer 1) uses accent2; y clamped.

```rust
pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let cy = h * 0.52;
    (0..3)
        .map(|layer| {
            let l = layer as f32;
            let points = (0..=(w as usize / 6))
                .map(|step| {
                    let px = step as f32 * 6.0;
                    let f = px / w;
                    let v = ctx.band(f * 0.84);
                    let amp = 6.0 + v * h * 0.24 * (1.0 - l * 0.22);
                    let y = cy
                        + (px * 0.006 * (1.0 + l * 0.35) + ctx.clock * (1.3 + l * 0.6) + l * 2.1).sin() * amp
                        + (px * 0.017 - ctx.clock * 2.4 + l).sin() * amp * 0.4;
                    (px, y.clamp(0.0, h))
                })
                .collect();
            let fill = if layer == 1 {
                ctx.accent2_fill(0.42)
            } else {
                ctx.accent_fill(0.55 - l * 0.16)
            };
            Shape {
                geom: Geom::Polyline { points, closed: false },
                fill,
                width: 2.2 - l * 0.5,
                glow: 0.5 - l * 0.15,
                dash: None,
            }
        })
        .collect()
}
```

### Task 14: Pulse

Invariants: shockwaves (accent2) + RadialGlow + core arc + 16 orbit discs (accent2).

```rust
pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let (cx, cy) = (w / 2.0, h * 0.47);
    let mut shapes = Vec::new();
    for wave in ctx.impact.shockwaves() {
        shapes.push(Shape {
            geom: Geom::Arc { cx, cy, r: m * 0.15 + wave.progress * m * 0.55, a0: 0.0, a1: TAU },
            fill: ctx.accent2_fill((1.0 - wave.progress) * wave.strength * 0.4),
            width: 2.0, glow: 0.0, dash: None,
        });
    }
    let r = m * 0.13 + ctx.level * m * 0.10 + ctx.kick * m * 0.04;
    shapes.push(Shape {
        geom: Geom::RadialGlow { cx, cy, r: r * 1.9 },
        fill: ctx.accent_fill(0.4),
        width: 0.0, glow: 0.0, dash: None,
    });
    shapes.push(Shape {
        geom: Geom::Arc { cx, cy, r, a0: 0.0, a1: TAU },
        fill: ctx.accent_fill(0.85),
        width: 2.5, glow: 0.7, dash: None,
    });
    for i in 0..16 {
        let angle = i as f32 / 16.0 * TAU + ctx.clock * 0.55;
        let v = ctx.band(0.075 + i as f32 * 0.057);
        let orbit = r + m * 0.05 + v * m * 0.13;
        shapes.push(Shape {
            geom: Geom::Disc { cx: cx + angle.cos() * orbit, cy: cy + angle.sin() * orbit, r: 2.2 + v * 4.5 },
            fill: ctx.accent2_fill(0.35 + 0.6 * v),
            width: 0.0, glow: 0.0, dash: None,
        });
    }
    shapes
}
```

### Task 15: Particles

Invariants: ≥ `DUST_COUNT` discs; 6 px column raster; chain tips (v > 0.62, fr > 0.45) use accent2; edge fade.

```rust
pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let cy = h * 0.52;
    let t = ctx.clock;
    let mut shapes: Vec<Shape> = ctx.dust.iter().map(|p| {
        let tw = 0.4 + 0.6 * (0.5 + 0.5 * (t * p.tw + p.ph).sin());
        Shape {
            geom: Geom::Disc { cx: p.nx * w, cy: p.ny * h, r: p.r },
            fill: ctx.accent_fill(p.a * tw),
            width: 0.0, glow: 0.0, dash: None,
        }
    }).collect();
    let edge = w * 0.05;
    let span = w - 2.0 * edge;
    let mut px = edge;
    while px <= w - edge {
        let f = (px - edge) / span;
        let ef = (f.min(1.0 - f) * 10.0).min(1.0);
        let v = ctx.band(f * 0.94);
        let sgn = (px * 0.016 + t * 2.1).sin() + 0.55 * (px * 0.037 - t * 3.3).sin();
        let len = v.powf(1.35) * h * 0.34 * sgn * ef;
        let dots = ((len.abs() / 6.0) as usize).clamp(1, 22);
        for d in 0..=dots {
            let fr = d as f32 / dots as f32;
            let alpha = (0.15 + 0.75 * v) * (1.0 - fr * 0.65) * ef;
            let fill = if v > 0.62 && fr > 0.45 {
                ctx.accent2_fill(alpha)
            } else {
                ctx.accent_fill(alpha)
            };
            shapes.push(Shape {
                geom: Geom::Disc { cx: px, cy: cy + len * fr, r: 1.1 + (1.0 - fr) * 1.3 + v * 0.8 },
                fill, width: 0.0, glow: 0.0, dash: None,
            });
        }
        shapes.push(Shape {
            geom: Geom::Disc { cx: px, cy, r: 1.4 + v * 1.6 },
            fill: Fill::Solid(Rgba { r: 0.91, g: 0.925, b: 0.965, a: (0.25 + 0.55 * v) * ef }),
            width: 0.0, glow: 0.0, dash: None,
        });
        px += 6.0;
    }
    shapes
}
```

### Task 16: Neon

Invariants: hue-sweep segment `Rect`s; 4 envelope polylines, outer two dashed `(2.0, 6.0)`.

```rust
pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let cy = h * 0.5;
    let hue = color::rgb_hue(ctx.accent);
    let mut shapes = Vec::new();
    let seg = (w * 0.008).max(9.0);
    let gap = seg * 0.8;
    let n = ((w * 0.8) / (seg + gap)) as usize;
    for i in 0..n {
        let f = i as f32 / (n - 1).max(1) as f32;
        let v = ctx.band(f * 0.92);
        let bh = 3.0 + v * h * 0.065;
        shapes.push(Shape {
            geom: Geom::Rect { x: w * 0.1 + i as f32 * (seg + gap), y: cy - bh / 2.0, w: seg * 0.55, h: bh },
            fill: ctx.hue_sweep_fill(hue, 0.9, w * 0.08, w * 0.92),
            width: 0.0, glow: 0.4, dash: None,
        });
    }
    let lines = [(-h * 0.075, 0.8_f32, None, 1.0_f32), (h * 0.075, 0.8, None, 1.0),
                 (-h * 0.135, 0.45, Some((2.0, 6.0)), 0.7), (h * 0.135, 0.45, Some((2.0, 6.0)), 0.7)];
    for (li, (off, alpha, dash, amp)) in lines.into_iter().enumerate() {
        let sign = if off < 0.0 { -1.0 } else { 1.0 };
        let points = (0..=((w * 0.84) as usize / 5))
            .map(|step| {
                let px = w * 0.08 + step as f32 * 5.0;
                let f = (px - w * 0.08) / (w * 0.84);
                let v = ctx.band(f * 0.92);
                let y = cy + off - sign * v.powf(1.6) * h * 0.05 * amp
                    - sign * (px * 0.05 + ctx.clock * 4.0 + li as f32).sin() * v * h * 0.012;
                (px, y)
            })
            .collect();
        shapes.push(Shape {
            geom: Geom::Polyline { points, closed: false },
            fill: ctx.hue_sweep_fill(hue, alpha, w * 0.08, w * 0.92),
            width: if dash.is_some() { 1.2 } else { 1.8 },
            glow: if dash.is_some() { 0.3 } else { 0.6 },
            dash,
        });
    }
    shapes
}
```

### Task 17: Tunnel

Invariants: 8 depth rings alternating closed wavy polylines / ≤ 44 tick arcs; radii strictly increase with depth; center mini-bars.

```rust
pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let min_r = m * 0.09;
    let max_r = (w * w + h * h).sqrt() * 0.62;
    let hue = color::rgb_hue(ctx.accent);
    let rings = 8;
    let band_at = |a: f32| ctx.band(a.min(TAU - a) / std::f32::consts::PI * 0.86);
    let mut shapes = Vec::new();
    for k in (0..rings).rev() {
        let prog = k as f32 / rings as f32;
        let r0 = min_r * (max_r / min_r).powf(prog) * (1.0 + ctx.kick * 0.025);
        let fade = (prog * 5.0).min(1.0) * ((1.0 - prog) * 2.5 + 0.25).min(1.0);
        if fade <= 0.02 { continue; }
        let ring_hue = hue + ((((k * 47) % 140) + 140) % 140) as f32 - 70.0;
        if k % 2 == 0 {
            let points = (0..=76)
                .map(|s| {
                    let a = s as f32 / 76.0 * TAU;
                    let rr = r0 * (1.0 + band_at(a) * 0.13);
                    (cx + a.cos() * rr, cy + a.sin() * rr)
                })
                .collect();
            shapes.push(Shape {
                geom: Geom::Polyline { points, closed: true },
                fill: ctx.hsla_fill(ring_hue, 0.85, 0.62, 0.85 * fade),
                width: 2.0 + prog * 3.5, glow: 0.7, dash: None,
            });
        } else {
            for s in 0..44 {
                let a = (s as f32 / 44.0 * TAU + k as f32 * 0.3 + ctx.clock * 0.1).rem_euclid(TAU);
                let v = band_at(a);
                let dash_len = (0.25 + v * 0.85) * (TAU / 44.0) * 0.42;
                shapes.push(Shape {
                    geom: Geom::Arc { cx, cy, r: r0, a0: a, a1: a + dash_len },
                    fill: ctx.hsla_fill(ring_hue, 0.85, 0.52 + v * 0.16, (0.35 + 0.6 * v) * fade),
                    width: 3.0 + prog * 4.0 + v * 2.0, glow: 0.5, dash: None,
                });
            }
        }
    }
    let bars = 38;
    let span = w * 0.075;
    for i in 0..bars {
        let f = i as f32 / (bars - 1) as f32;
        let v = ctx.band(f * 0.92);
        let bh = 1.5 + v * m * 0.035;
        shapes.push(Shape {
            geom: Geom::Rect { x: cx - span + f * span * 2.0, y: cy - bh / 2.0, w: 1.6, h: bh },
            fill: ctx.hsla_fill(hue - 50.0 + f * 100.0, 0.85, 0.64, 0.4 + 0.6 * v),
            width: 0.0, glow: 0.0, dash: None,
        });
    }
    shapes
}
```

---

### Task 18: Final pass

- [ ] **Step 1:** `REPRISE_VIS_OUT=<scratchpad> cargo test -p reprise-gnome render_gallery -- --ignored --nocapture` → 8 PPMs → PNG → eyeball against the mock and reference images; fix obvious visual bugs inline.
- [ ] **Step 2:** `cargo fmt && cargo clippy --workspace --all-targets` clean.
- [ ] **Step 3:** `cargo test --workspace 2>&1 | tail -5` green (known flaky-suite caveats: re-run single tests on suspicion).
- [ ] **Step 4:** Xvfb live smoke (never desktop): `REPRISE_AUDIO_SINK=fakesink`, play fixture, Now Playing → Visual → F11, screenshot, verify chrome + one animating mode + mode switching via digits.
- [ ] **Step 5:** Update spec deviations (8 modes from the design mock supersede Bars-only; engine moved to core for KDE/Android frontends; dual accent; water grid; dropped mock artifacts; volume-slider one-way sync).
- [ ] **Step 6:** `git commit -am "docs(visualizer): record design-mock implementation deviations"`.

---

## Self-Review Notes

- Portability: every mode, envelope, pool, palette-extraction and the engine live in `reprise-core` with zero GUI deps; the GTK side is one Cairo mapper + widgets. A KDE/Android frontend implements `draw_scene` + feeds `ingest`/`tick`.
- Spec coverage: sluggishness → Tasks 1–3 + engine easing (Task 6); resolution → 256→64 log bands; dual accent → Tasks 4, 6, modes 11–15; water-grid revision → Tasks 5, 11; chrome → 8–10; mini-view mode picker → Task 7; WOW/glow → scene glow flag + renderer bloom.
- Type consistency: `Rgba`/`Fill`/`Geom`/`Shape`/`Scene` (Task 4) used verbatim in 5–17; `ModeCtx` fixed in Task 6; `PlayerHooks` fixed in Task 8.
- Deferred: mode persistence across sessions; fullscreen volume slider following external changes; real palette-based primary (primary remains the app accent by design).
