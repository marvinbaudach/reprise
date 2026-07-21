# Visualizer WOW Rework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Now Playing visualizer react hard and legibly to music (log-spaced bands, per-band auto-gain), add 8 selectable visual modes (Grid, Bars, Rings, Flow, Pulse, Particles, Neon, Tunnel), and rebuild the fullscreen view after the approved Claude-Design mock (blurred-cover backdrop, header, seek bar, volume, transport, pill mode picker, keyboard shortcuts, refined auto-hide).

**Architecture:** Audio analysis stays in `reprise-core` (`SpectrumAnalyzer` folds 256 raw FFT bins into 64 log-spaced display bands with per-band AGC + gamma; beats/level/bass computed pre-AGC). The GTK visualizer becomes an orchestrator (`song_visualizer.rs`) over small per-mode modules that each build a `Vec<Shape>` from a shared `ModeCtx`; one Cairo renderer (`paint.rs`) draws shapes with accent/HSL/gradient paints and fake-bloom glow. The fullscreen chrome moves to its own module (`fullscreen.rs`).

**Tech Stack:** Rust, gtk4-rs + Cairo, GStreamer `spectrum` element, rusqlite (next-up title lookup).

**Design source:** Claude-Design project `GTK4 Musik-Visualizer Darkmode`, file `Vollbild Visualizer.dc.html` (read 2026-07-21). The mode draw math below is a faithful Rust port of that mock's canvas code. Mock-only demo artifacts are deliberately dropped: fake BPM beat synthesis (we have real `beat` events), cover drag-drop slots (real covers via `CoverLoader`), the `colorSource` toggle (accent always follows the cover, as the app already does), frame-precise timecode (positions arrive in ms at 500 ms ticks → `H:MM:SS`).

## Global Constraints

- All visuals stay behind `motion::animations_enabled()` (Motion rulebook O): reduced motion → static frame, no flashes, no particle motion.
- User-visible strings: English source text via `N_!` in `crates/reprise-gnome/src/ui/strings_audio_analysis.rs`.
- No fake audio data: every animated value derives from real `SpectrumFrame` signals; only ambient drift (`clock`) may be time-based.
- Commit messages: `<type>: <description>` (feat/fix/refactor/docs/test/chore), English, no attribution footer.
- `cargo fmt` and `cargo clippy` clean; tests must run headless (no display) except `#[ignore]`d ones.
- Never open an app window on the desktop for verification; use the PPM gallery tests / Xvfb.
- File size discipline: keep modules focused; per-mode files stay well under 300 lines.

## File Map

| File | Responsibility | Task |
|---|---|---|
| `crates/reprise-core/src/playback.rs` | Constants, log fold, AGC, analyzer | 1–2 |
| `crates/reprise-platform-linux/src/player_effects.rs` | spectrum element: 256 bands @ 16 ms | 3 |
| `crates/reprise-platform-linux/src/player.rs` (+`player/tests.rs`) | parse 256 raw bins | 3 |
| `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs` | Orchestrator: RenderState, tick, inline widget, picker | 4, 6 |
| `…/song_visualizer/paint.rs` (new) | Shape/Paint vocabulary, glow, Cairo renderer, HSL | 5 |
| `…/song_visualizer/modes.rs` (new) | `VisualMode` enum, `ModeCtx`, dispatch, history/dust/rng | 6 |
| `…/song_visualizer/modes/{grid,bars,rings,flow,pulse,particles,neon,tunnel}.rs` (new) | one mode each, incl. its own `#[cfg(test)]` | 8–15 |
| `…/song_visualizer/fullscreen.rs` (new) | fullscreen chrome per design | 7b, 7c |
| `…/song_visualizer/impact.rs` | keep: shockwaves, sparks, flash, kick source | touched 6 |
| `…/song_visualizer_tests.rs` | shared state tests + gallery render | 6, 16 |
| `crates/reprise-gnome/src/ui/strings_audio_analysis.rs` | mode labels + chrome strings | 6, 7b |
| `crates/reprise-gnome/src/ui/now_playing/now_playing.rs` | forward meta/cover/position/hooks | 7a |
| `crates/reprise-gnome/src/ui/playback/player_controller.rs` + `now_playing_wiring.rs` + `player_event_handling.rs` | position callback to panel | 7a |
| `crates/reprise-gnome/src/ui/window/window_now_playing_wiring.rs` | player hooks install | 7a |

## Parallel Lanes (after Task 6)

- **Lane FS:** Task 7a → 7b → 7c (owns `song_visualizer.rs`, `fullscreen.rs`, `now_playing.rs`, wiring files).
- **Lanes M1–M4:** Tasks 8–15, two modes per lane; each mode file is exclusively owned by its lane (tests live inside the mode file — no shared-file conflicts). M1: 8, 9. M2: 10, 11, 12. M3: 13, 14. M4: 15.
- Task 16 runs last, single lane.

Tasks 1–6 are strictly sequential.

---

### Task 1: Core — log-spaced display bands

**Files:**
- Modify: `crates/reprise-core/src/playback.rs` (constants block at ~line 76; add `log_band_edges` near `ema_coeff`)

**Interfaces:**
- Produces: `pub const SPECTRUM_ANALYSIS_BAND_COUNT: usize = 256` (raw FFT bins the platform requests), `pub const SPECTRUM_BAND_COUNT: usize = 64` (display bands in `SpectrumFrame`), `pub const SPECTRUM_INTERVAL_MS: u64 = 16`, private `fn log_band_edges() -> [usize; SPECTRUM_BAND_COUNT + 1]`.

- [ ] **Step 1: Write the failing tests** (append inside the existing `mod spectrum_analyzer_tests`)

```rust
#[test]
fn log_band_edges_cover_every_raw_bin_exactly_once() {
    let edges = log_band_edges();
    assert_eq!(edges[0], 0);
    assert_eq!(edges[SPECTRUM_BAND_COUNT], SPECTRUM_ANALYSIS_BAND_COUNT);
    for band in 0..SPECTRUM_BAND_COUNT {
        assert!(
            edges[band] < edges[band + 1],
            "band {band} must be non-empty and strictly increasing"
        );
    }
}

#[test]
fn log_band_edges_keep_bass_resolution_and_widen_highs() {
    let edges = log_band_edges();
    // The lowest display bands map single raw bins (kick sub stays isolated) …
    assert_eq!(edges[1] - edges[0], 1);
    assert_eq!(edges[2] - edges[1], 1);
    // … while the topmost band folds a wide raw range.
    assert!(edges[SPECTRUM_BAND_COUNT] - edges[SPECTRUM_BAND_COUNT - 1] >= 8);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p reprise-core --lib log_band_edges`
Expected: compile error `cannot find function log_band_edges` (that is the valid RED for a new symbol).

- [ ] **Step 3: Implement constants + edges**

Replace the constants block:

```rust
/// Raw FFT band count requested from the platform analyzer. Linear in
/// frequency (an FFT property), so most musical detail crowds the bottom
/// bins — [`SpectrumAnalyzer`] folds these into [`SPECTRUM_BAND_COUNT`]
/// log-spaced display bands before anything reaches a frontend.
pub const SPECTRUM_ANALYSIS_BAND_COUNT: usize = 256;
/// Log-spaced display bands carried by [`SpectrumFrame`]. This is the only
/// resolution frontends see.
pub const SPECTRUM_BAND_COUNT: usize = 64;
/// Target interval between spectrum messages. 16 ms (~60 Hz) matches the
/// display refresh so every rendered frame can carry fresh data. The platform
/// analyzer builds its element from this; the analyzer's envelope time
/// constants derive from it so the feel is independent of the exact rate.
pub const SPECTRUM_INTERVAL_MS: u64 = 16;
const SPECTRUM_FLOOR_DB: f32 = -80.0;
```

Add near `ema_coeff`:

```rust
/// Raw-bin edges of the log-spaced display bands: display band `d` folds raw
/// FFT bins `edges[d]..edges[d+1]`. Strictly increasing and complete — every
/// raw bin belongs to exactly one display band. Low bands map 1:1 (kick sub
/// sits alone in band 0), high bands widen geometrically so mids and highs
/// spread across the full display width instead of crowding the left edge.
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

Note: this task leaves `SpectrumAnalyzer::ingest` still taking `[f32; SPECTRUM_BAND_COUNT]` — the existing analyzer tests will FAIL TO COMPILE until Task 2 lands. Tasks 1 and 2 therefore commit together only if the tree must stay green between them: run only the two new tests in Step 4, and treat Task 2 as the completing half. Do NOT push between Tasks 1 and 2.

- [ ] **Step 4: Run the new tests**

Run: `cargo test -p reprise-core --lib log_band_edges 2>&1 | tail -5`
Expected: both new tests PASS (other analyzer tests may still fail to compile until Task 2 — acceptable mid-flight, not at commit time; commit happens at end of Task 2).

---

### Task 2: Core — AGC + gamma, `ingest` takes raw bins

**Files:**
- Modify: `crates/reprise-core/src/playback.rs` (analyzer at ~line 196, tests module)

**Interfaces:**
- Consumes: `log_band_edges`, `SPECTRUM_ANALYSIS_BAND_COUNT` from Task 1.
- Produces: `pub fn SpectrumAnalyzer::ingest(&mut self, decibels: [f32; SPECTRUM_ANALYSIS_BAND_COUNT]) -> SpectrumFrame`. Frame's `bands()` are now post-AGC display values; `level`/`bass`/`beat`/`dynamics` are computed pre-AGC (true loudness). `SpectrumFrame::from_decibels` keeps its display-sized signature (neutral constructor for tests/stateless callers).

- [ ] **Step 1: Rewrite the analyzer test module for the new contract**

Replace the whole `mod spectrum_analyzer_tests` body with (keep the two Task-1 tests):

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
    assert!(!frame.beat().fired, "steady tone must not keep firing beats");
    assert!(
        (frame.level() - 0.75).abs() < 0.05,
        "level tracks true loudness (pre-AGC), got {}",
        frame.level()
    );
    // Auto-gain: every display band uses the full range on a steady tone.
    assert!(frame.bands().iter().all(|&band| band > 0.95));
}

#[test]
fn agc_preserves_contrast_when_the_music_gets_quieter() {
    let mut analyzer = SpectrumAnalyzer::new();
    ingest_n(&mut analyzer, [-20.0; SPECTRUM_ANALYSIS_BAND_COUNT], 60);
    let quiet = ingest_n(&mut analyzer, [-40.0; SPECTRUM_ANALYSIS_BAND_COUNT], 3);
    // -40 dB against a -20 dB adapted gain must read clearly lower, not ~equal.
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
    assert!(hit.beat().fired, "a slam after silence must register a beat");
    assert!(hit.beat().strength > 0.0);
    assert!(hit.level() > 0.9, "attack is instant, got {}", hit.level());
}

#[test]
fn level_releases_gradually_after_impulse() {
    let mut analyzer = SpectrumAnalyzer::new();
    ingest_n(&mut analyzer, SILENCE, 20);
    let hit = analyzer.ingest(FULL);
    let after = analyzer.ingest(SILENCE);
    assert!(after.level() < hit.level());
    assert!(after.level() > 0.1, "release is gradual, got {}", after.level());
}

#[test]
fn silence_then_sustained_loud_spikes_dynamics() {
    let mut analyzer = SpectrumAnalyzer::new();
    ingest_n(&mut analyzer, SILENCE, 40);
    let frame = ingest_n(&mut analyzer, FULL, 4);
    assert!(frame.dynamics() > 0.3, "drop after lull, got {}", frame.dynamics());
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
    assert!(beats <= 3, "smooth build-up fired {beats} beats");
}

#[test]
fn all_outputs_stay_finite_and_bounded() {
    let mut analyzer = SpectrumAnalyzer::new();
    for step in 0..200 {
        let db = [-80.0_f32 + (step % 80) as f32; SPECTRUM_ANALYSIS_BAND_COUNT];
        let frame = analyzer.ingest(db);
        assert!(frame.bands().iter().all(|band| band.is_finite() && (0.0..=1.0).contains(band)));
        assert!(frame.level().is_finite() && (0.0..=1.0).contains(&frame.level()));
        assert!(frame.bass().is_finite() && (0.0..=1.0).contains(&frame.bass()));
        assert!((0.0..=1.0).contains(&frame.beat().strength));
        assert!((-1.0..=1.0).contains(&frame.dynamics()));
    }
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p reprise-core --lib spectrum_analyzer 2>&1 | rg "error|FAILED" | head -5`
Expected: compile errors (`ingest` signature mismatch, `SPECTRUM_ANALYSIS_BAND_COUNT`-sized arrays).

- [ ] **Step 3: Implement**

(a) Rename `normalize_bands` → generic `normalize_db` and keep `from_decibels` display-sized:

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

In `from_decibels`, call `normalize_db(decibels)` and update its doc line to: "Bands-only frame with neutral scalars, taking display-resolution decibels directly (no log fold, no auto-gain)."

(b) Add constants after `BEAT_STRENGTH_OVERSHOOT`:

```rust
/// Per-band auto-gain: each display band slowly tracks its own recent maximum
/// and is normalized against it, so every band uses the full visual range —
/// this is what makes quiet-vs-hit differences *look* dramatic instead of a
/// 20 % wiggle at the top of a compressed dB scale.
const AGC_HALF_LIFE_MS: f32 = 8000.0;
/// Auto-gain never amplifies below this reference: silence and noise stay at
/// rest instead of being blown up to full scale.
const AGC_FLOOR: f32 = 0.10;
/// Contrast curve applied to the auto-gained display value.
const DISPLAY_GAMMA: f32 = 1.4;
```

(c) Extend the struct + `new`:

```rust
pub struct SpectrumAnalyzer {
    edges: [usize; SPECTRUM_BAND_COUNT + 1],
    agc: [f32; SPECTRUM_BAND_COUNT],
    agc_decay: f32,
    prev_bands: [f32; SPECTRUM_BAND_COUNT],
    // …existing fields unchanged…
}
```

In `new()` add:

```rust
edges: log_band_edges(),
agc: [AGC_FLOOR; SPECTRUM_BAND_COUNT],
agc_decay: 0.5_f32.powf(SPECTRUM_INTERVAL_MS as f32 / AGC_HALF_LIFE_MS),
```

(d) Replace `ingest`:

```rust
/// Consume one raw-decibel frame, advance the internal state, and emit the
/// enriched, bounded [`SpectrumFrame`] for this instant. The scalars
/// (`level`, `bass`, `beat`, `dynamics`) are derived from the pre-auto-gain
/// fold so they track true loudness; only the display `bands` get AGC+gamma.
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

- [ ] **Step 4: Run all core playback tests**

Run: `cargo test -p reprise-core --lib "playback::" 2>&1 | tail -5`
Expected: all PASS (including the `from_decibels` test and the two Task-1 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/playback.rs
git commit -m "feat(core): log-spaced display bands with per-band auto-gain

256 raw FFT bins fold into 64 log-spaced display bands; per-band AGC +
gamma give every band its full visual range so hits read dramatically.
Beats/level/bass/dynamics stay on pre-AGC values. Interval 16ms."
```

---

### Task 3: Platform — 256-band element + parse

**Files:**
- Modify: `crates/reprise-platform-linux/src/player_effects.rs:33-44` (bands property)
- Modify: `crates/reprise-platform-linux/src/player.rs:20-40` (`spectrum_decibels_from_structure`)
- Test: `crates/reprise-platform-linux/src/player/tests.rs` (`ac_10_*` spectrum tests)

**Interfaces:**
- Consumes: `SPECTRUM_ANALYSIS_BAND_COUNT`, `SpectrumAnalyzer::ingest([f32; 256])` from Task 2.
- Produces: `spectrum_decibels_from_structure` returns `Option<[f32; SPECTRUM_ANALYSIS_BAND_COUNT]>`.

- [ ] **Step 1: Update the two tests**

In `ac_10_audio_filter_contains_a_disabled_bounded_spectrum_analyzer` change the assertion to:

```rust
assert_eq!(
    spectrum.property::<u32>("bands"),
    reprise_core::playback::SPECTRUM_ANALYSIS_BAND_COUNT as u32
);
```

Replace `ac_10_spectrum_messages_project_exactly_one_bounded_frame` with:

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
    assert!(frame
        .bands()
        .iter()
        .all(|band| band.is_finite() && (0.0..=1.0).contains(band)));
    assert!(spectrum_decibels_from_structure(&gst::Structure::new_empty("other")).is_none());
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p reprise-platform-linux ac_10 2>&1 | rg "error|FAILED" | head -5`
Expected: compile error (array size mismatch in `spectrum_decibels_from_structure`).

- [ ] **Step 3: Implement**

In `player.rs`, change the import to `SPECTRUM_ANALYSIS_BAND_COUNT` (keep `SpectrumAnalyzer`) and:

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

In `player_effects.rs` change the `bands` property to `SPECTRUM_ANALYSIS_BAND_COUNT`:

```rust
.property(
    "bands",
    u32::try_from(reprise_core::playback::SPECTRUM_ANALYSIS_BAND_COUNT)
        .expect("the fixed analysis band count fits u32"),
)
```

(`SPECTRUM_INTERVAL_NS` already derives from `SPECTRUM_INTERVAL_MS` — no change.)

- [ ] **Step 4: Run platform tests**

Run: `cargo test -p reprise-platform-linux 2>&1 | tail -3`
Expected: all PASS (including the live `ac_10_enabled_player_emits_live_spectrum_frames`).

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-platform-linux
git commit -m "feat(platform): request 256 spectrum bands and feed raw bins to the analyzer"
```

---

### Task 4: GNOME — 64-band consumers + snap attack

**Files:**
- Modify: `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs` (constants, `set_profile`)

**Interfaces:**
- Consumes: `SPECTRUM_BAND_COUNT = 64` (arrays resize automatically via the constant).

- [ ] **Step 1: Snap attack constants**

```rust
/// Bands snap up almost instantly (attack) and fall slowly (release): the
/// asymmetry is what makes transients punch instead of averaging away.
const BAND_ATTACK: f32 = 0.9;
const BAND_RELEASE: f32 = 0.14;
const SCALAR_ATTACK: f32 = 0.9;
const SCALAR_RELEASE: f32 = 0.16;
```

- [ ] **Step 2: Fix the static-profile fold for 64 bands** — in `set_profile` replace `dimensions[index / 8]` with:

```rust
let dimension = dimensions[index / (SPECTRUM_BAND_COUNT / 4)] as f32 / 100.0;
```

- [ ] **Step 3: Build + run visualizer tests**

Run: `cargo build -p reprise-gnome && cargo test -p reprise-gnome song_visualizer 2>&1 | tail -3`
Expected: build clean, all tests PASS (bars now renders 64 columns).

- [ ] **Step 4: Commit**

```bash
git add crates/reprise-gnome
git commit -m "feat(visualizer): consume 64 log bands with snap attack"
```

---

### Task 5: Paint — shape vocabulary + glow renderer

**Files:**
- Create: `crates/reprise-gnome/src/ui/now_playing/song_visualizer/paint.rs`
- Modify: `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs` (add `mod paint;` under `mod impact;`)

**Interfaces:**
- Produces (all `pub(super)` unless noted):

```rust
pub(super) enum Paint {
    Accent { alpha: f64 },
    Rgba { r: f64, g: f64, b: f64, alpha: f64 },
    Hsla { hue: f64, sat: f64, light: f64, alpha: f64 },
    /// Horizontal hue sweep hue-70 → hue+70 across x0..x1, ends fading to 0.
    HueSweep { hue: f64, alpha: f64, x0: f64, x1: f64 },
}
pub(super) enum Geom {
    Polyline { points: Vec<(f64, f64)>, closed: bool },
    Arc { cx: f64, cy: f64, r: f64, a0: f64, a1: f64 },
    Disc { cx: f64, cy: f64, r: f64 },
    Rect { x: f64, y: f64, w: f64, h: f64 },
    /// Filled radial gradient (core glows, washes): paint alpha at center → 0 at r.
    RadialGlow { cx: f64, cy: f64, r: f64 },
}
pub(super) struct Shape {
    pub geom: Geom,
    pub paint: Paint,
    /// Stroke width; 0.0 means fill (Disc/Rect/RadialGlow are always filled).
    pub width: f64,
    /// 0.0 = none; otherwise an extra under-stroke pass at width*3 and
    /// alpha*glow*0.35 fakes bloom (Cairo has no shadowBlur).
    pub glow: f64,
    pub dash: Option<(f64, f64)>,
}
pub(super) fn draw_shapes(cr: &gtk4::cairo::Context, shapes: &[Shape], accent: (f64, f64, f64));
pub(super) fn hsla_to_rgb(hue: f64, sat: f64, light: f64) -> (f64, f64, f64);
pub(super) fn rgb_hue(rgb: (f64, f64, f64)) -> f64;   // 0..360, 250.0 for gray
#[cfg(test)] pub(super) fn shapes_are_finite_and_sane(shapes: &[Shape], width: f64, height: f64) -> bool;
```

- [ ] **Step 1: Write failing tests** (inside `paint.rs`, `#[cfg(test)] mod tests`)

```rust
use super::*;

#[test]
fn hsla_roundtrip_produces_valid_rgb_and_hue() {
    for hue in [0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 359.0] {
        let rgb = hsla_to_rgb(hue, 0.85, 0.6);
        assert!([rgb.0, rgb.1, rgb.2].iter().all(|c| (0.0..=1.0).contains(c)));
        let back = rgb_hue(rgb);
        let delta = (back - hue).abs().min(360.0 - (back - hue).abs());
        assert!(delta < 2.0, "hue {hue} came back as {back}");
    }
}

#[test]
fn sanity_check_accepts_bounded_and_rejects_nan() {
    let ok = Shape {
        geom: Geom::Disc { cx: 10.0, cy: 10.0, r: 3.0 },
        paint: Paint::Accent { alpha: 0.5 },
        width: 0.0,
        glow: 0.0,
        dash: None,
    };
    assert!(shapes_are_finite_and_sane(&[ok], 100.0, 100.0));
    let bad = Shape {
        geom: Geom::Disc { cx: f64::NAN, cy: 10.0, r: 3.0 },
        paint: Paint::Accent { alpha: 0.5 },
        width: 0.0,
        glow: 0.0,
        dash: None,
    };
    assert!(!shapes_are_finite_and_sane(&[bad], 100.0, 100.0));
}
```

- [ ] **Step 2: Run to verify RED** — `cargo test -p reprise-gnome paint:: 2>&1 | rg "error" | head -3` → module missing.

- [ ] **Step 3: Implement `paint.rs`**

```rust
//! Shape vocabulary + Cairo renderer for the visualizer modes.
//!
//! Modes build `Vec<Shape>` (pure, testable); this module resolves paints
//! (accent / HSL / horizontal hue sweeps) and draws them, faking bloom with a
//! wide translucent under-stroke because Cairo has no shadowBlur.

use std::f64::consts::TAU;

// (enums/structs exactly as in the Interfaces block above)

pub(super) fn hsla_to_rgb(hue: f64, sat: f64, light: f64) -> (f64, f64, f64) {
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

pub(super) fn rgb_hue(rgb: (f64, f64, f64)) -> f64 {
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

fn resolve(paint: &Paint, accent: (f64, f64, f64)) -> (f64, f64, f64, f64) {
    match paint {
        Paint::Accent { alpha } => (accent.0, accent.1, accent.2, *alpha),
        Paint::Rgba { r, g, b, alpha } => (*r, *g, *b, *alpha),
        Paint::Hsla { hue, sat, light, alpha } => {
            let (r, g, b) = hsla_to_rgb(*hue, *sat, *light);
            (r, g, b, *alpha)
        }
        // HueSweep resolved separately (gradient source) — mid color here.
        Paint::HueSweep { hue, alpha, .. } => {
            let (r, g, b) = hsla_to_rgb(*hue, 0.85, 0.62);
            (r, g, b, *alpha)
        }
    }
}

fn set_source(cr: &gtk4::cairo::Context, paint: &Paint, accent: (f64, f64, f64), alpha_scale: f64) {
    if let Paint::HueSweep { hue, alpha, x0, x1 } = paint {
        let gradient = gtk4::cairo::LinearGradient::new(*x0, 0.0, *x1, 0.0);
        let a = (alpha * alpha_scale).clamp(0.0, 1.0);
        for (offset, hue_offset, stop_alpha) in [
            (0.0, -70.0, 0.0),
            (0.15, -70.0, a),
            (0.5, 0.0, a),
            (0.85, 70.0, a),
            (1.0, 70.0, 0.0),
        ] {
            let (r, g, b) = hsla_to_rgb(hue + hue_offset, 0.85, 0.62);
            gradient.add_color_stop_rgba(offset, r, g, b, stop_alpha);
        }
        let _ = cr.set_source(&gradient);
        return;
    }
    let (r, g, b, a) = resolve(paint, accent);
    cr.set_source_rgba(r, g, b, (a * alpha_scale).clamp(0.0, 1.0));
}

fn trace(cr: &gtk4::cairo::Context, geom: &Geom) {
    match geom {
        Geom::Polyline { points, closed } => {
            let Some(first) = points.first() else { return };
            cr.move_to(first.0, first.1);
            for point in &points[1..] {
                cr.line_to(point.0, point.1);
            }
            if *closed {
                cr.close_path();
            }
        }
        Geom::Arc { cx, cy, r, a0, a1 } => cr.arc(*cx, *cy, *r, *a0, *a1),
        Geom::Disc { cx, cy, r } => cr.arc(*cx, *cy, *r, 0.0, TAU),
        Geom::Rect { x, y, w, h } => cr.rectangle(*x, *y, *w, *h),
        Geom::RadialGlow { .. } => {}
    }
}

pub(super) fn draw_shapes(cr: &gtk4::cairo::Context, shapes: &[Shape], accent: (f64, f64, f64)) {
    cr.set_line_cap(gtk4::cairo::LineCap::Round);
    cr.set_line_join(gtk4::cairo::LineJoin::Round);
    for shape in shapes {
        if let Geom::RadialGlow { cx, cy, r } = shape.geom {
            let (cr_r, cr_g, cr_b, a) = resolve(&shape.paint, accent);
            let gradient = gtk4::cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, r.max(1.0));
            gradient.add_color_stop_rgba(0.0, cr_r, cr_g, cr_b, a.clamp(0.0, 1.0));
            gradient.add_color_stop_rgba(1.0, cr_r, cr_g, cr_b, 0.0);
            if cr.set_source(&gradient).is_ok() {
                cr.arc(cx, cy, r.max(1.0), 0.0, TAU);
                let _ = cr.fill();
            }
            continue;
        }
        match shape.dash {
            Some((on, off)) => cr.set_dash(&[on, off], 0.0),
            None => cr.set_dash(&[], 0.0),
        }
        // Fake bloom: wide translucent under-pass first.
        if shape.glow > 0.0 && shape.width > 0.0 {
            set_source(cr, &shape.paint, accent, shape.glow * 0.35);
            cr.set_line_width(shape.width * 3.0);
            trace(cr, &shape.geom);
            let _ = cr.stroke();
        }
        set_source(cr, &shape.paint, accent, 1.0);
        if shape.width > 0.0 {
            cr.set_line_width(shape.width);
            trace(cr, &shape.geom);
            let _ = cr.stroke();
        } else {
            trace(cr, &shape.geom);
            let _ = cr.fill();
        }
    }
    cr.set_dash(&[], 0.0);
}

#[cfg(test)]
pub(super) fn shapes_are_finite_and_sane(shapes: &[Shape], width: f64, height: f64) -> bool {
    let bound = 4.0 * width.max(height);
    let ok = |v: f64| v.is_finite() && v.abs() <= bound;
    shapes.iter().all(|shape| {
        (match &shape.geom {
            Geom::Polyline { points, .. } => points.iter().all(|p| ok(p.0) && ok(p.1)),
            Geom::Arc { cx, cy, r, a0, a1 } => {
                ok(*cx) && ok(*cy) && ok(*r) && *r >= 0.0 && a0.is_finite() && a1.is_finite()
            }
            Geom::Disc { cx, cy, r } => ok(*cx) && ok(*cy) && ok(*r) && *r >= 0.0,
            Geom::Rect { x, y, w, h } => ok(*x) && ok(*y) && ok(*w) && ok(*h) && *w >= 0.0 && *h >= 0.0,
            Geom::RadialGlow { cx, cy, r } => ok(*cx) && ok(*cy) && ok(*r) && *r >= 0.0,
        }) && shape.width.is_finite()
            && shape.width >= 0.0
            && (0.0..=1.0).contains(&shape.glow)
    })
}
```

Derive nothing on the types beyond what compiles; add `#[derive(Clone, Debug)]` on `Paint`, `Geom`, `Shape`.

- [ ] **Step 4: Run** — `cargo test -p reprise-gnome paint:: 2>&1 | tail -3` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/now_playing/song_visualizer/paint.rs crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs
git commit -m "feat(visualizer): shape/paint vocabulary with hue sweeps and fake bloom"
```

---

### Task 6: Modes scaffold — enum, ModeCtx, state, inline picker

**Files:**
- Create: `crates/reprise-gnome/src/ui/now_playing/song_visualizer/modes.rs`
- Create: `crates/reprise-gnome/src/ui/now_playing/song_visualizer/modes/` with 8 stub files (each initially delegating to the Bars port in Task 9's file — see Step 3)
- Modify: `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs` (RenderState, tick, draw func, inline picker row)
- Modify: `crates/reprise-gnome/src/ui/strings_audio_analysis.rs` (labels)
- Modify: `crates/reprise-gnome/src/ui/now_playing/song_visualizer/impact.rs` (expose kick)
- Test: `crates/reprise-gnome/src/ui/now_playing/song_visualizer_tests.rs`

**Interfaces:**
- Produces:

```rust
// modes.rs
pub(in crate::ui) enum VisualMode { Grid, Bars, Rings, Flow, Pulse, Particles, Neon, Tunnel }
impl VisualMode {
    pub(in crate::ui) const ALL: [Self; 8];
    pub(super) fn id(self) -> &'static str;          // "grid".."tunnel"
    pub(super) fn label(self) -> &'static str;       // strings::SONG_VISUALS_MODE_*
}
pub(super) const GRID_COLS: usize = 44;
pub(super) const GRID_ROWS: usize = 26;
pub(super) const GRID_ROW_TICKS: u32 = 5;            // ≈83 ms per history row
pub(super) const DUST_COUNT: usize = 120;
pub(super) struct HistoryRing { /* rows, head, len, timer */ }
impl HistoryRing {
    pub(super) fn advance(&mut self, bands: &[f32; SPECTRUM_BAND_COUNT]);  // per tick
    pub(super) fn frac(&self) -> f64;                 // 0..1 progress to next row
    pub(super) fn rows_newest_first(&self) -> impl Iterator<Item = &[f32; GRID_COLS]>;
}
#[derive(Clone, Copy)]
pub(super) struct Dust { pub nx: f64, pub ny: f64, pub r: f64, pub a: f64, pub tw: f64, pub ph: f64, dx: f64, dy: f64 }
pub(super) fn make_dust() -> [Dust; DUST_COUNT];      // deterministic xorshift
pub(super) fn advance_dust(dust: &mut [Dust; DUST_COUNT], level: f64);
pub(super) struct ModeCtx<'a> {
    pub bands: &'a [f32; SPECTRUM_BAND_COUNT],
    pub peaks: &'a [f32; SPECTRUM_BAND_COUNT],
    pub level: f64, pub bass: f64, pub mid: f64, pub high: f64,
    pub kick: f64,                                    // beat envelope 1→0
    pub clock: f64,                                   // seconds, ambient drift
    pub accent: (f64, f64, f64),
    pub history: &'a HistoryRing,
    pub dust: &'a [Dust; DUST_COUNT],
    pub impact: &'a super::impact::ImpactState,       // shockwaves for rings/pulse
    pub width: f64, pub height: f64,
}
pub(super) fn band(bands: &[f32; SPECTRUM_BAND_COUNT], f: f64) -> f64;   // fraction sample
pub(super) fn build_scene(mode: VisualMode, ctx: &ModeCtx) -> Vec<paint::Shape>;
```

- RenderState gains: `mode: VisualMode`, `mid: f32`, `high: f32`, `kick: f64`, `clock: f64`, `history: HistoryRing`, `dust: [Dust; DUST_COUNT]`. `advance_state` additionally: `mid`/`high` = `envelope`-style over `mean(bands[20..44])` / `mean(bands[44..64])` (instant up, `SCALAR_RELEASE` down), `kick *= 0.90`, `clock += 1.0/60.0` while playing, `history.advance(&current)`, `advance_dust(&mut dust, level)`.
- `set_spectrum` additionally: on `beat.fired` → `state.kick = state.kick.max(0.6 + 0.4 * strength)`.
- Impact stays the source of shockwaves/flash; sparks remain wired only where a mode asks for them (Bars).
- Strings: `SONG_VISUALS_MODE_GRID = N_!("Grid")`, `_BARS = N_!("Bars")`, `_RINGS = N_!("Rings")`, `_FLOW = N_!("Flow")`, `_PULSE = N_!("Pulse")`, `_PARTICLES = N_!("Particles")`, `_NEON = N_!("Neon")`, `_TUNNEL = N_!("Tunnel")`.
- Inline picker: a `gtk4::FlowBox` (max-children-per-line 4, halign center, css class `reprise-song-visual-modes`) of grouped `ToggleButton`s (same group mechanics as the pre-Bars-only `preset_controls` — see git history `9c10cc594a^:crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs` lines 222-254); selecting sets `state.mode` and `queue_registered_areas`. Appended to the inline `root` box under the canvas.
- Draw func becomes: accent wash (`RadialGlow` center, alpha `0.05 + 0.11 * level`) → `build_scene(mode, ctx)` → `draw_shapes` → drop flash overlay (`impact.flash()`, unchanged soft cap).

- [ ] **Step 1: Write failing tests** (rewrite `song_visualizer_tests.rs`: keep the impact + state tests, replace scene tests)

```rust
#[test]
fn every_mode_builds_a_finite_sane_scene() {
    let state = lively_state();               // helper: RenderState with ramp bands, kick, history rows pushed
    for mode in VisualMode::ALL {
        let ctx = state.mode_ctx((0.2, 0.7, 0.7), 548.0, 300.0);
        let shapes = modes::build_scene(mode, &ctx);
        assert!(!shapes.is_empty(), "{mode:?} must draw something");
        assert!(paint::shapes_are_finite_and_sane(&shapes, 548.0, 300.0), "{mode:?}");
    }
}

#[test]
fn mode_labels_are_stable() {
    assert_eq!(
        VisualMode::ALL.map(VisualMode::label).map(strings::text),
        ["Grid", "Bars", "Rings", "Flow", "Pulse", "Particles", "Neon", "Tunnel"]
    );
}

#[test]
fn history_ring_is_bounded_and_rolls() {
    let mut ring = HistoryRing::default();
    let bands = [0.5_f32; SPECTRUM_BAND_COUNT];
    for _ in 0..(GRID_ROW_TICKS * (GRID_ROWS as u32 + 10)) {
        ring.advance(&bands);
    }
    assert_eq!(ring.rows_newest_first().count(), GRID_ROWS);
    assert!((0.0..=1.0).contains(&ring.frac()));
}

#[test]
fn dust_stays_in_unit_box_forever() {
    let mut dust = make_dust();
    for _ in 0..5000 {
        advance_dust(&mut dust, 1.0);
    }
    assert!(dust.iter().all(|p| (-0.05..=1.05).contains(&p.nx) && (-0.05..=1.05).contains(&p.ny)));
}
```

`lively_state()` and `RenderState::mode_ctx(accent, w, h)` are new test/helper code in `song_visualizer.rs`:

```rust
impl RenderState {
    fn mode_ctx(&self, accent: (f64, f64, f64), width: f64, height: f64) -> ModeCtx<'_> {
        ModeCtx {
            bands: &self.current,
            peaks: &self.peaks,
            level: f64::from(self.level),
            bass: f64::from(self.bass),
            mid: f64::from(self.mid),
            high: f64::from(self.high),
            kick: self.kick,
            clock: self.clock,
            accent,
            history: &self.history,
            dust: &self.dust,
            impact: &self.impact,
            width,
            height,
        }
    }
}
```

- [ ] **Step 2: RED** — `cargo test -p reprise-gnome song_visualizer 2>&1 | rg error | head -5`.

- [ ] **Step 3: Implement** — `modes.rs` per the interface block. Key code:

```rust
pub(super) fn band(bands: &[f32; SPECTRUM_BAND_COUNT], f: f64) -> f64 {
    let idx = (f.clamp(0.0, 1.0) * (SPECTRUM_BAND_COUNT - 1) as f64).round() as usize;
    f64::from(bands[idx.min(SPECTRUM_BAND_COUNT - 1)])
}

pub(super) struct HistoryRing {
    rows: [[f32; GRID_COLS]; GRID_ROWS],
    len: usize,
    head: usize,
    timer: u32,
}
impl Default for HistoryRing { /* zeroed */ }
impl HistoryRing {
    pub(super) fn advance(&mut self, bands: &[f32; SPECTRUM_BAND_COUNT]) {
        self.timer += 1;
        if self.timer < GRID_ROW_TICKS {
            return;
        }
        self.timer = 0;
        let mut row = [0.0_f32; GRID_COLS];
        let half = (GRID_COLS - 1) as f64 / 2.0;
        for (col, slot) in row.iter_mut().enumerate() {
            // Mirror around the center like the design mock: the mountain is
            // symmetric, bass in the middle.
            let f = (col as f64 - half).abs() / half;
            *slot = bands[((f * 0.77 * (SPECTRUM_BAND_COUNT - 1) as f64) as usize)
                .min(SPECTRUM_BAND_COUNT - 1)];
        }
        self.head = (self.head + GRID_ROWS - 1) % GRID_ROWS;
        self.rows[self.head] = row;
        self.len = (self.len + 1).min(GRID_ROWS);
    }
    pub(super) fn frac(&self) -> f64 {
        f64::from(self.timer) / f64::from(GRID_ROW_TICKS)
    }
    pub(super) fn rows_newest_first(&self) -> impl Iterator<Item = &[f32; GRID_COLS]> {
        (0..self.len).map(move |i| &self.rows[(self.head + i) % GRID_ROWS])
    }
}

fn xorshift(state: &mut u32) -> f64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    f64::from(x) / f64::from(u32::MAX)
}

pub(super) fn make_dust() -> [Dust; DUST_COUNT] {
    let mut rng = 0x9e37_79b9_u32;
    std::array::from_fn(|i| {
        let floor = i % 5 < 2;
        let r1 = xorshift(&mut rng);
        let big = xorshift(&mut rng) < 0.12;
        Dust {
            nx: xorshift(&mut rng),
            ny: if floor { 0.72 + xorshift(&mut rng) * 0.26 } else { xorshift(&mut rng) * 0.7 },
            r: if big { 2.6 + r1 * 1.6 } else { 0.8 + r1 * 1.4 },
            a: if floor { 0.10 + xorshift(&mut rng) * 0.16 } else { 0.08 + xorshift(&mut rng) * 0.22 },
            tw: 0.4 + xorshift(&mut rng) * 1.6,
            ph: xorshift(&mut rng) * 7.0,
            dx: (xorshift(&mut rng) - 0.5) * 0.06,
            dy: -(0.02 + xorshift(&mut rng) * 0.04),
        }
    })
}

pub(super) fn advance_dust(dust: &mut [Dust; DUST_COUNT], level: f64) {
    const DT: f64 = 1.0 / 60.0;
    for p in dust.iter_mut() {
        p.nx += p.dx * DT;
        p.ny += p.dy * DT * (1.0 + 5.0 * level);
        if p.ny < -0.02 { p.ny = 1.02; }
        if p.nx < -0.02 { p.nx = 1.02; } else if p.nx > 1.02 { p.nx = -0.02; }
    }
}

pub(super) fn build_scene(mode: VisualMode, ctx: &ModeCtx) -> Vec<paint::Shape> {
    match mode {
        VisualMode::Grid => grid::scene(ctx),
        VisualMode::Bars => bars::scene(ctx),
        VisualMode::Rings => rings::scene(ctx),
        VisualMode::Flow => flow::scene(ctx),
        VisualMode::Pulse => pulse::scene(ctx),
        VisualMode::Particles => particles::scene(ctx),
        VisualMode::Neon => neon::scene(ctx),
        VisualMode::Tunnel => tunnel::scene(ctx),
    }
}
```

Create the 8 stub files; each stub initially returns the Task-9 Bars scene body (copy it into `bars.rs` first, stubs call `super::bars::scene(ctx)`), so the whole tree compiles and the every-mode test passes before Tasks 8–15 replace stubs. Also: move the current `bars_scene` + peaks logic into `modes/bars.rs` as the first real port (see Task 9 for the final shape — the stub phase may keep the current 64-column bar code adapted to return `Vec<Shape>`).

Orchestrator changes in `song_visualizer.rs`: delete `Scene`/`Bar`/`SceneInput`/`bars_scene`/`draw_scene` bodies in favor of `Vec<Shape>` + `paint::draw_shapes`; keep `RenderState` smoothing/peaks; add the new fields; add the FlowBox picker (`mode_controls(state, areas) -> gtk4::FlowBox`, ToggleButton group like the historical `preset_controls`); wash + flash drawing stays in the draw func using `RadialGlow` shapes.

- [ ] **Step 4: GREEN** — `cargo test -p reprise-gnome song_visualizer 2>&1 | tail -3` → all PASS.
- [ ] **Step 5: Gallery smoke** — run the (updated in this task) `render_gallery` ignored test rendering all 8 modes to PPM (loop `VisualMode::ALL`, filename `visualizer-{id}.ppm`); convert + eyeball.
- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(visualizer): 8-mode scaffold with shared ModeCtx and inline picker"
```

---

### Task 7a: Fullscreen plumbing — position, seek, volume, cover, next-up

**Files:**
- Modify: `crates/reprise-gnome/src/ui/playback/player_controller.rs` (~line 302: add `now_playing_panel_position_changed` callback slot following the `now_playing_panel_state_changed` pattern)
- Modify: `crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs` (~line 192: add `set_on_now_playing_panel_position_changed`)
- Modify: `crates/reprise-gnome/src/ui/playback/player_event_handling.rs` (~line 33: inside the `PlayerEvent::Position` arm, invoke the new callback)
- Modify: `crates/reprise-gnome/src/ui/now_playing/now_playing.rs` (forward position/cover/next-up; replace `set_visual_transport`)
- Modify: `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs` (hooks + stored display state)
- Modify: `crates/reprise-gnome/src/ui/window/window_now_playing_wiring.rs` (install hooks)

**Interfaces:**
- Produces on `SongVisualizer` (all `pub(in crate::ui)`):

```rust
pub struct PlayerHooks {
    pub previous: Rc<dyn Fn()>,
    pub play_pause: Rc<dyn Fn()>,
    pub stop: Rc<dyn Fn()>,
    pub next: Rc<dyn Fn()>,
    pub seek_to_ms: Rc<dyn Fn(i64)>,
    pub set_volume: Rc<dyn Fn(f64)>,
    pub initial_volume: f64,
}
fn set_player_hooks(&self, hooks: PlayerHooks)        // replaces set_transport
fn set_position(&self, position_ms: i64, duration_ms: i64)
fn set_cover(&self, texture: Option<gtk4::gdk::Texture>)
fn set_next_up(&self, line: Option<String>)           // pre-formatted "Up next: …"
fn set_queue_position(&self, index: usize, total: usize)
```

- `NowPlayingPanel` mirrors: `set_visual_player_hooks(hooks)`, and internally forwards position (new panel method `set_position`), cover (piggyback where `cover_loader.load_into` fires at `now_playing.rs:665` — after loading the panel cover, also fetch the texture for the visualizer via a second `load_into` on a hidden `gtk4::Image` OR read the resulting paintable off `self.widgets.cover` post-load; implementer: use `self.widgets.cover.paintable()` polled in the same generation callback if available, else add a 1-wide `gtk4::Image` sibling loaded with `ThumbnailSize::Full` and read its paintable), queue position + next-up from `set_up_next_model` (`QueueViewModel { ids, sections }` from `crates/reprise-gnome/src/ui/track_list/queue_sections.rs:43`; current index = `ids.iter().position(|id| *id == current_track_id)`; next-up title via rusqlite on the panel's `conn`: `SELECT title FROM tracks WHERE id = ?1`, `Option<String>` on any error → `None`).
- Wiring in `window_now_playing_wiring.rs` replaces the `set_visual_transport` block:

```rust
let hook = |player: &Rc<PlayerController>, action: fn(&PlayerController)| -> Rc<dyn Fn()> {
    let weak = Rc::downgrade(player);
    Rc::new(move || {
        if let Some(player) = weak.upgrade() {
            action(&player);
        }
    })
};
let seek_weak = Rc::downgrade(player);
let volume_weak = Rc::downgrade(player);
panel.set_visual_player_hooks(crate::ui::now_playing::PlayerHooks {
    previous: hook(player, PlayerController::previous),
    play_pause: hook(player, PlayerController::toggle_pause),
    stop: hook(player, PlayerController::reset_to_stopped),
    next: hook(player, PlayerController::next),
    seek_to_ms: Rc::new(move |ms| {
        if let Some(player) = seek_weak.upgrade() {
            player.seek(ms);
        }
    }),
    set_volume: Rc::new(move |volume| {
        if let Some(player) = volume_weak.upgrade() {
            player.player.set_volume(volume);
            player.sync_volume_indicator(volume);
            player.volume.set(volume);
            player.update_mpris_volume(volume);
        }
    }),
    initial_volume: player.volume.get(),
});
let panel_weak = Rc::downgrade(panel);
player.set_on_now_playing_panel_position_changed(move |position_ms, duration_ms| {
    if let Some(panel) = panel_weak.upgrade() {
        panel.set_position(position_ms, duration_ms);
    }
});
```

Known v1 limitation (accepted): the fullscreen volume slider initializes from `initial_volume` and tracks its own changes; external volume changes (bar, MPRIS) do not update it while open.

- [ ] **Step 1:** Implement callback slot + wiring exactly as above (no unit test possible without a display; the live test is Task 16's Xvfb pass). Ensure `cargo build -p reprise-gnome` clean.
- [ ] **Step 2:** `cargo test -p reprise-gnome 2>&1 | tail -3` → all PASS.
- [ ] **Step 3: Commit** — `git commit -am "feat(visualizer): player hooks, position, cover and queue plumbing for fullscreen"`

---

### Task 7b: Fullscreen chrome per design

**Files:**
- Create: `crates/reprise-gnome/src/ui/now_playing/song_visualizer/fullscreen.rs`
- Modify: `crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs` (`toggle_fullscreen` delegates; css() additions)
- Modify: `crates/reprise-gnome/src/ui/strings_audio_analysis.rs`

**Interfaces:**
- Produces: `pub(super) fn build(visualizer: &SongVisualizer, parent: &adw::ApplicationWindow) -> gtk4::Window` — constructs the whole overlay; `SongVisualizer::toggle_fullscreen` only opens/closes it. Live-update handles stored on the visualizer as today (`fullscreen_meta`, `fullscreen_play_pause`) plus new `fullscreen_position: Rc<RefCell<Option<(gtk4::Label, gtk4::Label, gtk4::Label, gtk4::DrawingArea /*seek*/)>>>` and `fullscreen_backdrop: Rc<RefCell<Option<gtk4::Picture>>>`.

**Layout (port of the mock, GTK terms):**

```
gtk4::Overlay
├─ backdrop: gtk4::Picture            (blurred cover: texture downscaled to 24px wide
│                                      pixbuf → upscaled by Picture ContentFit::Cover;
│                                      css opacity .45; hidden when no cover)
├─ canvas: gtk4::DrawingArea          (vexpand; draw func FIRST paints the vignette:
│                                      RadialGlow dark #0f101c a=0.45→0.87 handled as
│                                      two cairo radial gradients, THEN wash+scene)
├─ header (Box, valign Start, css .reprise-song-visual-chrome):
│    timecode Label (halign Start, margin-start 28, tabular, .reprise-fs-timecode)
│    center column: stateLabel (uppercase small accent), title (36px bold),
│                   meta "Artist · Album" (subtitle)
└─ bottom (Box vertical, valign End, css .reprise-song-visual-chrome):
     row1: cover thumb (84px gtk4::Picture, rounded) + "TRACK i / n" + next-up label
           …spacer… mute Button + volume Scale (110px)
     row2: timeCur Label · seek (gtk4::Scale, hexpand, css .reprise-fs-seek) · timeTotal
     row3: transport (prev 46 · play/pause 60 primary · stop 46 · next 46, circular)
     row4: mode pills (same FlowBox builder as inline — reuse `mode_controls`)
     row5: hint Label (dim): "Space pause · ←/→ seek · 1–8 mode · F fullscreen"
```

Header/bottom get the scrim via CSS `background: linear-gradient(...)` (GTK4 CSS supports linear-gradient). Seek: `gtk4::Scale` 0..1000; on `change-value` → `hooks.seek_to_ms(fraction * duration_ms)`; guard against feedback with an `updating: Cell<bool>` while `set_position` writes it. Strings (new): `SONG_VISUALS_STATE_PLAYING = N_!("Playing")`, `_PAUSED = N_!("Paused")`, `_STOPPED = N_!("Stopped")`, `SONG_VISUALS_TRACK_POS = N_!("TRACK {index} / {total}")`, `SONG_VISUALS_NEXT_UP = N_!("Up next: {title}")`, `SONG_VISUALS_MUTE = N_!("Mute")`, `SONG_VISUALS_VOLUME = N_!("Volume")`, `SONG_VISUALS_SEEK = N_!("Seek")`, `SONG_VISUALS_FULLSCREEN_HINT` re-worded to `N_!("Space pause · ←/→ seek · 1–8 mode · F fullscreen")`.

CSS additions in `css()` (accent stays `@reprise_player_accent`):

```css
.reprise-fs-header-scrim { background: linear-gradient(to bottom, alpha(#0b0c15,0.55), alpha(#0b0c15,0)); }
.reprise-fs-bottom-scrim { background: linear-gradient(to top, alpha(#0b0c15,0.6), alpha(#0b0c15,0)); }
.reprise-fs-timecode { font-size: 13px; letter-spacing: 0.08em; color: alpha(#ffffff,0.45); }
.reprise-fs-state { font-size: 12px; letter-spacing: 0.22em; color: alpha(@reprise_player_accent,0.85); }
.reprise-fs-title { font-size: 36px; font-weight: 600; color: #ffffff; }
.reprise-fs-meta { font-size: 16px; color: alpha(#ffffff,0.7); }
.reprise-fs-pill { /* pills: 999px radius, alpha(#ffffff,0.06) bg, thin border */ }
.reprise-fs-pill:checked { border-color: alpha(@reprise_player_accent,0.8); background: alpha(@reprise_player_accent,0.12); color: #ffffff; }
/* transport buttons: keep existing .reprise-song-visual-transport-btn/-primary */
```

- [ ] **Step 1:** Move current fullscreen construction into `fullscreen.rs::build`, extend to the layout above.
- [ ] **Step 2:** `cargo build -p reprise-gnome` clean; `cargo test -p reprise-gnome song_visualizer` PASS (css test updated for new classes: assert `.reprise-fs-title` and both scrims present).
- [ ] **Step 3: Commit** — `git commit -am "feat(visualizer): fullscreen chrome after the design mock"`

---

### Task 7c: Keyboard + auto-hide v2

**Files:**
- Modify: `crates/reprise-gnome/src/ui/now_playing/song_visualizer/fullscreen.rs`

**Behavior (port):** Space → play_pause; ←/→ → seek ±5 s (clamp 0..duration); ↑/↓ → volume ±0.05 (clamp 0..1, updates slider); F/F11/Escape → close (F mirrors the mock's fullscreen toggle — here the window IS the fullscreen, so F closes); N/P → next/previous; digits 1–8 → select mode (index into `VisualMode::ALL`, activates the matching pill). Every key wakes the chrome. Auto-hide: 3 s idle → fade chrome + hide cursor, but ONLY while `playback == Playing`; on Paused/Stopped the chrome stays visible and the timer is disarmed; pointer-leave hides immediately (when playing). Reuse/extend `install_chrome_autohide` (rename `install_chrome_autohide` → keep name; add `playing: Rc<dyn Fn() -> bool>` parameter reading `state.playback`).

- [ ] **Step 1:** Implement; `cargo build -p reprise-gnome` clean.
- [ ] **Step 2:** Headless check: `xvfb-run -a cargo test -p reprise-gnome --  --ignored ac_10_visual_widget` PASS.
- [ ] **Step 3: Commit** — `git commit -am "feat(visualizer): fullscreen keyboard control and playing-only auto-hide"`

---

### Tasks 8–15: The eight modes

Common shape for every mode task (shown once, applies to all):

- **Files:** Create/replace `…/song_visualizer/modes/<id>.rs` (stub from Task 6). Tests live inside the file: `#[cfg(test)] mod tests` builds a `ModeCtx` via the shared `lively_state()`-style helper (re-export a `pub(super) fn test_ctx…` from `modes.rs` in Task 6 if not already) and asserts (a) non-empty, (b) `paint::shapes_are_finite_and_sane`, (c) the mode-specific invariants listed below.
- **Steps:** write test → `cargo test -p reprise-gnome modes::<id>` RED → implement → GREEN → render this mode via the gallery test and eyeball the PPM → `git commit -m "feat(visualizer): <id> mode"`.
- All sampling uses `band(ctx.bands, f)`; sizes derive from `ctx.width/height`; `m = width.min(height)`.

### Task 8: Grid (perspective spectral mountain — ref image 2)

Invariants: exactly `rows+per-4-column` polylines when history is full; crest shape has `glow > 0`; all y between `0` and `height`.

```rust
pub(super) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let rows: Vec<&[f32; GRID_COLS]> = ctx.history.rows_newest_first().collect();
    if rows.len() < 2 { return vec![]; }
    let (w, h) = (ctx.width, ctx.height);
    let horizon = h * 0.30;
    let near_y = h * 0.94;
    let amp = h * 0.30;
    let frac = ctx.history.frac();
    let mut lines: Vec<(Vec<(f64, f64)>, f64, f64)> = Vec::new(); // (points, near, far_fade)
    for (i, row) in rows.iter().enumerate() {
        let age = ((i as f64 + frac) / GRID_ROWS as f64).min(1.0);
        let near = age.powf(1.6);
        let y0 = horizon + near * (near_y - horizon);
        let half = w * 0.30 + near * w * 0.68;
        let far_fade = (age * GRID_ROWS as f64 / 1.5).min(1.0);
        let row_amp = amp * (0.35 + 0.65 * near) * far_fade;
        let points = (0..GRID_COLS)
            .map(|c| {
                let px = w / 2.0 + ((c as f64 / (GRID_COLS - 1) as f64) - 0.5) * 2.0 * half;
                (px, (y0 - f64::from(row[c]) * row_amp).clamp(0.0, h))
            })
            .collect();
        lines.push((points, near, far_fade));
    }
    let mut shapes = Vec::new();
    for (points, near, far_fade) in lines.iter().rev() {
        shapes.push(Shape {
            geom: Geom::Polyline { points: points.clone(), closed: false },
            paint: Paint::Rgba { r: 0.815, g: 0.831, b: 0.894, alpha: (0.10 + 0.42 * near) * far_fade },
            width: 1.0 + near * 0.8, glow: 0.0, dash: None,
        });
    }
    for c in (0..GRID_COLS).step_by(4) {
        let points = lines.iter().map(|(pts, ..)| pts[c]).collect();
        shapes.push(Shape {
            geom: Geom::Polyline { points, closed: false },
            paint: Paint::Rgba { r: 0.815, g: 0.831, b: 0.894, alpha: 0.14 },
            width: 1.0, glow: 0.0, dash: None,
        });
    }
    if let Some((crest, ..)) = lines.get(2.min(lines.len() - 1)) {
        shapes.push(Shape {
            geom: Geom::Polyline { points: crest.clone(), closed: false },
            paint: Paint::Accent { alpha: 0.35 + 0.45 * ctx.kick },
            width: 1.6, glow: 0.8, dash: None,
        });
    }
    shapes
}
```

### Task 9: Bars (glow bars, ref current + mock)

Invariants: 44 bar shapes; total length grows with band values; sparks from `ctx.impact` appended as `Disc`s.

```rust
pub(super) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let n = 44;
    let mut shapes: Vec<Shape> = (0..n)
        .map(|i| {
            let v = band(ctx.bands, i as f64 / n as f64 * 0.73);
            let px = (i as f64 + 0.5) * w / n as f64;
            let len = (v * h * 0.8).max(4.0);
            Shape {
                geom: Geom::Polyline { points: vec![(px, h - 2.0), (px, h - len)], closed: false },
                paint: Paint::Accent { alpha: 0.28 + 0.62 * v },
                width: (m * 0.006).max(4.0), glow: v, dash: None,
            }
        })
        .collect();
    let center = (w / 2.0, h / 2.0);
    for spark in ctx.impact.particles() {
        shapes.push(Shape {
            geom: Geom::Disc {
                cx: center.0 + spark.angle.cos() * spark.dist,
                cy: center.1 + spark.angle.sin() * spark.dist,
                r: 1.4 + spark.life_frac * 2.6,
            },
            paint: Paint::Accent { alpha: spark.life_frac },
            width: 0.0, glow: 0.0, dash: None,
        });
    }
    shapes
}
```

### Task 10: Rings

Invariants: 7 ring `Arc`s + shockwave arcs from `ctx.impact.shockwaves()` + 1 `RadialGlow` core.

```rust
pub(super) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let (cx, cy) = (w / 2.0, h * 0.46);
    let mut shapes = Vec::new();
    for wave in ctx.impact.shockwaves() {
        shapes.push(Shape {
            geom: Geom::Arc { cx, cy, r: m * 0.12 + wave.progress * m * 0.55, a0: 0.0, a1: TAU },
            paint: Paint::Accent { alpha: (1.0 - wave.progress) * wave.strength * 0.5 },
            width: 1.5, glow: 0.0, dash: None,
        });
    }
    for i in 0..7 {
        let v = band(ctx.bands, i as f64 / 7.0 * 0.63 + 0.06);
        shapes.push(Shape {
            geom: Geom::Arc { cx, cy, r: m * (0.07 + i as f64 * 0.052) + v * m * 0.075, a0: 0.0, a1: TAU },
            paint: Paint::Accent { alpha: 0.14 + 0.6 * v },
            width: 2.0 + v * 2.5, glow: v * 0.6, dash: None,
        });
    }
    shapes.push(Shape {
        geom: Geom::RadialGlow { cx, cy, r: m * 0.07 + ctx.level * m * 0.05 },
        paint: Paint::Accent { alpha: 0.5 + 0.4 * ctx.kick },
        width: 0.0, glow: 0.0, dash: None,
    });
    shapes
}
```

### Task 11: Flow

Invariants: 3 polylines spanning ≥ 90 % width; y clamped inside canvas.

```rust
pub(super) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let cy = h * 0.52;
    (0..3)
        .map(|layer| {
            let l = layer as f64;
            let points = (0..=(w as usize / 6))
                .map(|step| {
                    let px = step as f64 * 6.0;
                    let f = px / w;
                    let v = band(ctx.bands, f * 0.83);
                    let amp = 6.0 + v * h * 0.24 * (1.0 - l * 0.22);
                    let y = cy
                        + ((px * 0.006 * (1.0 + l * 0.35)) + ctx.clock * (1.3 + l * 0.6) + l * 2.1).sin() * amp
                        + ((px * 0.017) - ctx.clock * 2.4 + l).sin() * amp * 0.4;
                    (px, y.clamp(0.0, h))
                })
                .collect();
            Shape {
                geom: Geom::Polyline { points, closed: false },
                paint: Paint::Accent { alpha: 0.55 - l * 0.16 },
                width: 2.2 - l * 0.5, glow: 0.5 - l * 0.15, dash: None,
            }
        })
        .collect()
}
```

### Task 12: Pulse

Invariants: shockwaves + `RadialGlow` + core `Arc` + 16 orbit `Disc`s.

```rust
pub(super) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let (cx, cy) = (w / 2.0, h * 0.47);
    let mut shapes = Vec::new();
    for wave in ctx.impact.shockwaves() {
        shapes.push(Shape {
            geom: Geom::Arc { cx, cy, r: m * 0.15 + wave.progress * m * 0.55, a0: 0.0, a1: TAU },
            paint: Paint::Accent { alpha: (1.0 - wave.progress) * wave.strength * 0.4 },
            width: 2.0, glow: 0.0, dash: None,
        });
    }
    let r = m * 0.13 + ctx.level * m * 0.10 + ctx.kick * m * 0.04;
    shapes.push(Shape {
        geom: Geom::RadialGlow { cx, cy, r: r * 1.9 },
        paint: Paint::Accent { alpha: 0.4 },
        width: 0.0, glow: 0.0, dash: None,
    });
    shapes.push(Shape {
        geom: Geom::Arc { cx, cy, r, a0: 0.0, a1: TAU },
        paint: Paint::Accent { alpha: 0.85 },
        width: 2.5, glow: 0.7, dash: None,
    });
    for i in 0..16 {
        let angle = i as f64 / 16.0 * TAU + ctx.clock * 0.55;
        let v = band(ctx.bands, 0.06 + i as f64 * 0.045);
        let orbit = r + m * 0.05 + v * m * 0.13;
        shapes.push(Shape {
            geom: Geom::Disc { cx: cx + angle.cos() * orbit, cy: cy + angle.sin() * orbit, r: 2.2 + v * 4.5 },
            paint: Paint::Accent { alpha: 0.35 + 0.6 * v },
            width: 0.0, glow: 0.0, dash: None,
        });
    }
    shapes
}
```

### Task 13: Particles (dust field + dotted mirror waveform — ref image 3)

Invariants: ≥ `DUST_COUNT` discs; column dot chains alternate above/below center; edge columns fade (alpha → 0 near x edges).

```rust
pub(super) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let cy = h * 0.52;
    let t = ctx.clock;
    let mut shapes: Vec<Shape> = ctx.dust.iter().map(|p| {
        let tw = 0.4 + 0.6 * (0.5 + 0.5 * (t * p.tw + p.ph).sin());
        Shape {
            geom: Geom::Disc { cx: p.nx * w, cy: p.ny * h, r: p.r },
            paint: Paint::Accent { alpha: p.a * tw },
            width: 0.0, glow: 0.0, dash: None,
        }
    }).collect();
    let edge = w * 0.05;
    let span = w - 2.0 * edge;
    let mut px = edge;
    while px <= w - edge {
        let f = (px - edge) / span;
        let ef = (f.min(1.0 - f) * 10.0).min(1.0);
        let v = band(ctx.bands, f * 0.94);
        let sgn = (px * 0.016 + t * 2.1).sin() + 0.55 * (px * 0.037 - t * 3.3).sin();
        let len = v.powf(1.35) * h * 0.34 * sgn * ef;
        let dots = ((len.abs() / 6.0) as usize).clamp(1, 22);
        for d in 0..=dots {
            let fr = d as f64 / dots as f64;
            shapes.push(Shape {
                geom: Geom::Disc { cx: px, cy: cy + len * fr, r: 1.1 + (1.0 - fr) * 1.3 + v * 0.8 },
                paint: Paint::Accent { alpha: (0.15 + 0.75 * v) * (1.0 - fr * 0.65) * ef },
                width: 0.0, glow: 0.0, dash: None,
            });
        }
        shapes.push(Shape {
            geom: Geom::Disc { cx: px, cy, r: 1.4 + v * 1.6 },
            paint: Paint::Rgba { r: 0.91, g: 0.925, b: 0.965, alpha: (0.25 + 0.55 * v) * ef },
            width: 0.0, glow: 0.0, dash: None,
        });
        px += 8.0;
    }
    shapes
}
```

### Task 14: Neon (hue-sweep segment meter + envelope lines — ref image 4)

Invariants: segment `Rect`s use `Paint::HueSweep`; 2 of 4 envelope polylines have `dash: Some((2.0, 6.0))`.

```rust
pub(super) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let cy = h * 0.5;
    let hue = paint::rgb_hue(ctx.accent);
    let mut shapes = Vec::new();
    let seg = (w * 0.008).max(9.0);
    let gap = seg * 0.8;
    let n = ((w * 0.8) / (seg + gap)) as usize;
    for i in 0..n {
        let f = i as f64 / (n - 1).max(1) as f64;
        let v = band(ctx.bands, f * 0.92);
        let bh = 3.0 + v * h * 0.065;
        shapes.push(Shape {
            geom: Geom::Rect { x: w * 0.1 + i as f64 * (seg + gap), y: cy - bh / 2.0, w: seg * 0.55, h: bh },
            paint: Paint::HueSweep { hue, alpha: 0.9, x0: w * 0.08, x1: w * 0.92 },
            width: 0.0, glow: 0.0, dash: None,
        });
    }
    let lines = [(-h * 0.075, 0.8, None, 1.0), (h * 0.075, 0.8, None, 1.0),
                 (-h * 0.135, 0.45, Some((2.0, 6.0)), 0.7), (h * 0.135, 0.45, Some((2.0, 6.0)), 0.7)];
    for (li, (off, alpha, dash, amp)) in lines.into_iter().enumerate() {
        let sign = if off < 0.0 { -1.0 } else { 1.0 };
        let points = (0..=((w * 0.84) as usize / 5))
            .map(|step| {
                let px = w * 0.08 + step as f64 * 5.0;
                let f = (px - w * 0.08) / (w * 0.84);
                let v = band(ctx.bands, f * 0.92);
                let y = cy + off - sign * v.powf(1.6) * h * 0.05 * amp
                    - sign * (px * 0.05 + ctx.clock * 4.0 + li as f64).sin() * v * h * 0.012;
                (px, y)
            })
            .collect();
        shapes.push(Shape {
            geom: Geom::Polyline { points, closed: false },
            paint: Paint::HueSweep { hue, alpha, x0: w * 0.08, x1: w * 0.92 },
            width: if dash.is_some() { 1.2 } else { 1.8 },
            glow: if dash.is_some() { 0.3 } else { 0.6 },
            dash,
        });
    }
    shapes
}
```

### Task 15: Tunnel (hue-rotating ring tunnel)

Invariants: 8 depth rings alternate closed wavy polylines (even) and ≤ 44 tick arcs (odd); ring radii strictly increase with depth; center mini-bars present.

```rust
pub(super) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let min_r = m * 0.09;
    let max_r = (w * w + h * h).sqrt() * 0.62;
    let hue = paint::rgb_hue(ctx.accent);
    let rings = 8;
    let band_at = |a: f64| {
        let f = a.min(TAU - a) / std::f64::consts::PI;
        band(ctx.bands, f * 0.86)
    };
    let mut shapes = Vec::new();
    for k in (0..rings).rev() {
        let prog = k as f64 / rings as f64;
        let r0 = min_r * (max_r / min_r).powf(prog) * (1.0 + ctx.kick * 0.025);
        let fade = (prog * 5.0).min(1.0) * ((1.0 - prog) * 2.5 + 0.25).min(1.0);
        if fade <= 0.02 { continue; }
        let ring_hue = hue + ((((k * 47) % 140) + 140) % 140) as f64 - 70.0;
        if k % 2 == 0 {
            let points = (0..=76)
                .map(|s| {
                    let a = s as f64 / 76.0 * TAU;
                    let rr = r0 * (1.0 + band_at(a) * 0.13);
                    (cx + a.cos() * rr, cy + a.sin() * rr)
                })
                .collect();
            shapes.push(Shape {
                geom: Geom::Polyline { points, closed: true },
                paint: Paint::Hsla { hue: ring_hue, sat: 0.85, light: 0.62, alpha: 0.85 * fade },
                width: 2.0 + prog * 3.5, glow: 0.7, dash: None,
            });
        } else {
            for s in 0..44 {
                let a = (s as f64 / 44.0 * TAU + k as f64 * 0.3 + ctx.clock * 0.1).rem_euclid(TAU);
                let v = band_at(a);
                let dash_len = (0.25 + v * 0.85) * (TAU / 44.0) * 0.42;
                shapes.push(Shape {
                    geom: Geom::Arc { cx, cy, r: r0, a0: a, a1: a + dash_len },
                    paint: Paint::Hsla { hue: ring_hue, sat: 0.85, light: 0.52 + v * 0.16, alpha: (0.35 + 0.6 * v) * fade },
                    width: 3.0 + prog * 4.0 + v * 2.0, glow: 0.5, dash: None,
                });
            }
        }
    }
    let bars = 38;
    let span = w * 0.075;
    for i in 0..bars {
        let f = i as f64 / (bars - 1) as f64;
        let v = band(ctx.bands, f * 0.92);
        let bh = 1.5 + v * m * 0.035;
        shapes.push(Shape {
            geom: Geom::Rect { x: cx - span + f * span * 2.0, y: cy - bh / 2.0, w: 1.6, h: bh },
            paint: Paint::Hsla { hue: hue - 50.0 + f * 100.0, sat: 0.85, light: 0.64, alpha: 0.4 + 0.6 * v },
            width: 0.0, glow: 0.0, dash: None,
        });
    }
    shapes
}
```

---

### Task 16: Final pass — gallery, lint, docs, live check

**Files:**
- Modify: `song_visualizer_tests.rs` (gallery loops `VisualMode::ALL`), `docs/superpowers/specs/2026-07-21-audio-reactive-visualizer-design.md` (deviations section)

- [ ] **Step 1:** `REPRISE_VIS_OUT=<scratchpad> cargo test -p reprise-gnome render_gallery -- --ignored --nocapture` → 8 PPMs; convert to PNG; eyeball each mode against the design mock and the three reference images. Fix obvious visual bugs (empty scene, off-center, wrong scale) inline.
- [ ] **Step 2:** `cargo fmt && cargo clippy --workspace --all-targets 2>&1 | rg "^warning|^error" | head` → clean (fix or justify).
- [ ] **Step 3:** `cargo test --workspace 2>&1 | tail -5` → green (known flaky suite caveats aside — rerun single-process on suspicion, per project memory).
- [ ] **Step 4:** Live smoke under Xvfb (never on the desktop): launch app with `REPRISE_AUDIO_SINK=fakesink` under `xvfb-run`, play a fixture, open Now Playing → Visual → fullscreen via injected F11 (xdotool), screenshot, verify chrome renders and one mode animates.
- [ ] **Step 5:** Update the spec's "Implementation notes / deviations": 8 design modes supersede the single-Bars decision (user-provided design mock, 2026-07-21); fullscreen chrome per mock; dropped mock artifacts (BPM fake, drag-drop covers, colorSource toggle, frame timecode); volume-slider one-way sync limitation.
- [ ] **Step 6:** Commit: `git commit -am "docs(visualizer): record design-mock implementation deviations"`.

---

## Self-Review Notes

- Spec coverage: reactivity/"träge" → Tasks 1–4 (AGC is the core fix); "Details/Auflösung" → 256→64 log bands (1–3); design mock chrome → 7a–7c; 8 modes incl. all three reference images → 8–15; mini-view mode selection → Task 6 picker; WOW/glow → Task 5 bloom + per-mode glow.
- Type consistency: `Shape/Geom/Paint` names used identically across Tasks 5–15; `ModeCtx` fields fixed in Task 6 and consumed verbatim later; `PlayerHooks` defined once (7a) and consumed in 7b/7c.
- Known deferred items (explicitly out of scope): persisting the selected mode across sessions; fullscreen volume slider reflecting external changes; "Als Nächstes" hidden below 1150 px width (always shown, ellipsized, in v1).
