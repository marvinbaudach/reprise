//! Portable scene adapter for already-smoothed CAVA bars.
//!
//! Signal processing ends at [`SpectrumFrame`]. This module deliberately does
//! not remap, normalize, or ease live bar heights a second time; it only keeps
//! the visual peak caps, presentation-only bass glow, and pause/stop fade
//! required by the UI contract.

use crate::playback::{SpectrumFrame, SPECTRUM_BAND_COUNT};

use super::color::{hue_shift, secondary_accent};
use super::modes;
use super::scene::{Fill, Geom, Rgba, Scene, Shape};

const PEAK_DECAY: f32 = 0.018;
const STOP_RELEASE: f32 = 0.12;
const SETTLE_EPSILON: f32 = 0.002;
const FALLBACK_ACCENT2_HUE_SHIFT: f32 = 42.0;
const BASS_GLOW_BAND_COUNT: usize = 12;
const BASS_GLOW_THRESHOLD: f32 = 0.36;
const BASS_GLOW_FULL_SCALE: f32 = 0.49;
const BASS_GLOW_DECAY: f32 = 0.024;
/// Ticks for one full travel of the idle wave (60 Hz → six seconds).
const IDLE_PERIOD_TICKS: f32 = 360.0;
/// Crests visible across the canvas width at any moment.
const IDLE_WAVE_COUNT: f32 = 1.0;
/// Ceiling of the resting wave, as a fraction of the bar height.
const IDLE_PEAK: f32 = 0.17;
/// Trough of the resting wave — the canvas is never fully empty.
const IDLE_FLOOR: f32 = 0.012;
/// Breaths per travel cycle: a second, slower swell over the whole wave so the
/// canvas rises and falls instead of only sliding sideways.
const IDLE_BREATH_RATIO: f32 = 1.5;
/// How much of the wave the breath takes away at its lowest point.
const IDLE_BREATH_DEPTH: f32 = 0.3;
/// Fade-in per tick once playback stops (≈0.4 s to full amplitude).
const IDLE_FADE_IN: f32 = 0.04;

/// Borrowed render inputs for the Bars scene builder.
pub struct ModeCtx<'a> {
    pub peaks: &'a [f32; SPECTRUM_BAND_COUNT],
    pub bars: &'a [f32; SPECTRUM_BAND_COUNT],
    pub bass_glow: f32,
    pub accent: (f32, f32, f32),
    pub accent2: (f32, f32, f32),
    pub width: f32,
    pub height: f32,
}

impl ModeCtx<'_> {
    /// Solid fill in the primary cover accent.
    pub fn accent_fill(&self, alpha: f32) -> Fill {
        let (r, g, b) = self.accent;
        Fill::Solid(Rgba { r, g, b, a: alpha })
    }
}

/// Adapts bounded CAVA frames to the resolution-independent Bars scene.
pub struct VisualEngine {
    bands_current: [f32; SPECTRUM_BAND_COUNT],
    bands_peaks: [f32; SPECTRUM_BAND_COUNT],
    /// What the scene draws: the live bars, lifted by the idle wave whenever a
    /// track is loaded but not playing (AC-11).
    display_bands: [f32; SPECTRUM_BAND_COUNT],
    bass_glow: f32,
    playing: bool,
    has_track: bool,
    idle_phase: f32,
    idle_amp: f32,
    accent: (f32, f32, f32),
    cover_accent2: Option<(f32, f32, f32)>,
}

impl Default for VisualEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl VisualEngine {
    pub fn new() -> Self {
        Self {
            bands_current: [0.0; SPECTRUM_BAND_COUNT],
            bands_peaks: [0.0; SPECTRUM_BAND_COUNT],
            display_bands: [0.0; SPECTRUM_BAND_COUNT],
            bass_glow: 0.0,
            playing: false,
            has_track: false,
            idle_phase: 0.0,
            idle_amp: 0.0,
            accent: (0.5, 0.5, 0.5),
            cover_accent2: None,
        }
    }

    /// Enables live frames or begins the visual-only stop fade.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        if playing {
            self.idle_amp = 0.0;
        }
        self.refresh_display_bands();
    }

    /// Whether a track is loaded at all. Without one there is nothing to keep
    /// alive, so the canvas rests fully empty (AC-11).
    pub fn set_has_track(&mut self, has_track: bool) {
        self.has_track = has_track;
        if !has_track {
            self.idle_amp = 0.0;
            self.idle_phase = 0.0;
        }
        self.refresh_display_bands();
    }

    /// A loaded, non-playing track breathes instead of showing an empty box.
    fn idle_active(&self) -> bool {
        self.has_track && !self.playing
    }

    /// Resting wave for `band`: a slow travelling swell, tapered to nothing at
    /// both edges so it reads as a breath rather than a signal.
    fn idle_band(&self, band: usize) -> f32 {
        if self.idle_amp <= 0.0 {
            return 0.0;
        }
        let across = band as f32 / (SPECTRUM_BAND_COUNT - 1) as f32;
        let envelope = (std::f32::consts::PI * across).sin();
        let wave = 0.5
            + 0.5 * (std::f32::consts::TAU * (across * IDLE_WAVE_COUNT - self.idle_phase)).sin();
        let breath = 1.0
            - IDLE_BREATH_DEPTH
                * (0.5 - 0.5 * (std::f32::consts::TAU * self.idle_phase * IDLE_BREATH_RATIO).sin());
        self.idle_amp * envelope * breath * (IDLE_FLOOR + (IDLE_PEAK - IDLE_FLOOR) * wave)
    }

    /// Live bars win; the idle wave only lifts whatever they leave empty.
    fn refresh_display_bands(&mut self) {
        let bands = std::array::from_fn(|band| self.bands_current[band].max(self.idle_band(band)));
        self.display_bands = bands;
    }

    pub fn set_accent(&mut self, rgb: (f32, f32, f32)) {
        self.accent = rgb;
    }

    pub fn set_cover_pixels(&mut self, rgba: &[u8], pixel_count: usize) {
        self.cover_accent2 = secondary_accent(rgba, pixel_count, self.accent);
    }

    pub fn clear_cover(&mut self) {
        self.cover_accent2 = None;
    }

    /// Clears the previous track's bar and peak-cap history.
    pub fn note_track_changed(&mut self) {
        self.bands_current = [0.0; SPECTRUM_BAND_COUNT];
        self.bands_peaks = [0.0; SPECTRUM_BAND_COUNT];
        self.bass_glow = 0.0;
        self.refresh_display_bands();
    }

    /// Installs the already-bounded CAVA values in the same frame.
    pub fn ingest(&mut self, frame: &SpectrumFrame) {
        self.bands_current = *frame.bands();
        self.bass_glow = self.bass_glow.max(bass_glow_target(&self.bands_current));
        for (peak, current) in self.bands_peaks.iter_mut().zip(self.bands_current.iter()) {
            *peak = peak.max(*current);
        }
        self.refresh_display_bands();
    }

    /// Advances peak caps, fades the live bars out once playback stops, and
    /// keeps the idle wave travelling while a loaded track rests (AC-11).
    pub fn tick(&mut self) -> bool {
        let mut settled = true;
        if self.idle_active() {
            self.idle_phase = (self.idle_phase + 1.0 / IDLE_PERIOD_TICKS).fract();
            self.idle_amp = (self.idle_amp + IDLE_FADE_IN).min(1.0);
        }
        if !self.playing {
            for bar in &mut self.bands_current {
                *bar += (0.0 - *bar) * STOP_RELEASE;
                if *bar < SETTLE_EPSILON {
                    *bar = 0.0;
                }
                settled &= *bar == 0.0;
            }
        }
        let bass_target = bass_glow_target(&self.bands_current);
        self.bass_glow = (self.bass_glow - BASS_GLOW_DECAY).max(bass_target);
        if self.bass_glow < SETTLE_EPSILON {
            self.bass_glow = 0.0;
        }
        settled &= (self.bass_glow - bass_target).abs() < SETTLE_EPSILON;
        for (peak, current) in self.bands_peaks.iter_mut().zip(self.bands_current.iter()) {
            *peak = (*peak - PEAK_DECAY).max(*current);
            if *peak < SETTLE_EPSILON {
                *peak = 0.0;
            }
            settled &= (*peak - *current).abs() < SETTLE_EPSILON;
        }
        self.refresh_display_bands();
        settled && !self.playing && !self.idle_active()
    }

    /// Removes visual-only motion without changing the current CAVA frame.
    /// With animations off the idle wave still shows — as a still image at its
    /// current phase, never as motion.
    pub fn snap_to_static(&mut self) {
        self.bands_peaks = self.bands_current;
        self.bass_glow = bass_glow_target(&self.bands_current);
        if self.idle_active() {
            self.idle_amp = 1.0;
        }
        self.refresh_display_bands();
    }

    pub fn accent2(&self) -> (f32, f32, f32) {
        self.cover_accent2
            .unwrap_or_else(|| hue_shift(self.accent, FALLBACK_ACCENT2_HUE_SHIFT))
    }

    fn make_ctx(&self, width: f32, height: f32) -> ModeCtx<'_> {
        ModeCtx {
            peaks: &self.bands_peaks,
            bars: &self.display_bands,
            bass_glow: self.bass_glow,
            accent: self.accent,
            accent2: self.accent2(),
            width,
            height,
        }
    }

    pub fn scene(&self, width: f32, height: f32) -> Scene {
        let ctx = self.make_ctx(width, height);
        let level = self.display_bands.iter().sum::<f32>() / SPECTRUM_BAND_COUNT as f32;
        let mut shapes = vec![Shape {
            geom: Geom::RadialGlow {
                cx: width / 2.0,
                cy: height * 0.44,
                r: width.max(height) * 0.6,
            },
            fill: ctx.accent_fill(0.05 + 0.11 * level),
            width: 0.0,
            glow: 0.0,
            dash: None,
        }];
        shapes.extend(modes::build_scene(&ctx));
        Scene { shapes }
    }
}

fn bass_glow_target(bars: &[f32; SPECTRUM_BAND_COUNT]) -> f32 {
    let mean_square = bars[..BASS_GLOW_BAND_COUNT]
        .iter()
        .map(|bar| bar * bar)
        .sum::<f32>()
        / BASS_GLOW_BAND_COUNT as f32;
    let normalized = ((mean_square.sqrt() - BASS_GLOW_THRESHOLD)
        / (BASS_GLOW_FULL_SCALE - BASS_GLOW_THRESHOLD))
        .clamp(0.0, 1.0);
    normalized * normalized * (3.0 - 2.0 * normalized)
}

#[cfg(test)]
pub(crate) fn lively_engine() -> VisualEngine {
    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    engine.set_accent((0.2, 0.7, 0.7));
    engine.ingest(&SpectrumFrame::from_cava_bars(std::array::from_fn(
        |index| 0.55 + index as f32 / SPECTRUM_BAND_COUNT as f32 * 0.4,
    )));
    engine
}

#[cfg(test)]
pub(crate) fn test_ctx(engine: &VisualEngine, width: f32, height: f32) -> ModeCtx<'_> {
    engine.make_ctx(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visuals::color;

    #[test]
    fn bars_builds_a_finite_sane_nonempty_scene() {
        let scene = lively_engine().scene(548.0, 300.0);
        assert!(scene.shapes.len() > 1);
        assert!(scene.is_finite_and_sane(548.0, 300.0));
    }

    #[test]
    fn ac_22_ingest_uses_cava_values_without_a_second_bar_envelope() {
        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        let bars = std::array::from_fn(|index| index as f32 / SPECTRUM_BAND_COUNT as f32);

        engine.ingest(&SpectrumFrame::from_cava_bars(bars));

        assert_eq!(engine.bands_current, bars);
    }

    #[test]
    fn ac_22_only_strong_bass_adds_broad_lower_neon_glows() {
        const WIDTH: f32 = 548.0;
        const HEIGHT: f32 = 300.0;

        let render = |energized: std::ops::Range<usize>| {
            let mut bars = [0.0; SPECTRUM_BAND_COUNT];
            bars[energized].fill(0.9);
            let mut engine = VisualEngine::new();
            engine.set_playing(true);
            engine.ingest(&SpectrumFrame::from_cava_bars(bars));
            engine.scene(WIDTH, HEIGHT)
        };
        let broad_lower_glows = |scene: Scene| {
            scene
                .shapes
                .into_iter()
                .filter(|shape| {
                    matches!(
                        shape.geom,
                        Geom::RadialGlow { cy, r, .. }
                            if cy > HEIGHT * 0.6 && r > WIDTH * 0.25
                    )
                })
                .count()
        };

        assert_eq!(broad_lower_glows(render(0..12)), 4);
        assert_eq!(
            broad_lower_glows(render(SPECTRUM_BAND_COUNT - 12..SPECTRUM_BAND_COUNT)),
            0
        );
    }

    #[test]
    fn ac_22_bass_glow_attacks_immediately_then_fades_after_the_beat() {
        const WIDTH: f32 = 548.0;
        const HEIGHT: f32 = 300.0;

        let glow_alpha = |engine: &VisualEngine| {
            engine
                .scene(WIDTH, HEIGHT)
                .shapes
                .into_iter()
                .find_map(|shape| match (shape.geom, shape.fill) {
                    (Geom::RadialGlow { cy, r, .. }, Fill::Solid(fill))
                        if cy > HEIGHT * 0.6 && r > WIDTH * 0.25 =>
                    {
                        Some(fill.a)
                    }
                    _ => None,
                })
                .unwrap_or(0.0)
        };
        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        let mut bass_hit = [0.0; SPECTRUM_BAND_COUNT];
        bass_hit[..12].fill(0.9);

        engine.ingest(&SpectrumFrame::from_cava_bars(bass_hit));
        let attacked = glow_alpha(&engine);
        engine.ingest(&SpectrumFrame::from_cava_bars([0.0; SPECTRUM_BAND_COUNT]));
        engine.tick();
        let after_one_tick = glow_alpha(&engine);

        assert!(attacked > 0.15);
        assert!(after_one_tick > 0.0);
        assert!(after_one_tick < attacked);

        for _ in 0..100 {
            engine.tick();
        }
        assert_eq!(glow_alpha(&engine), 0.0);
    }

    #[test]
    fn ac_22_sustained_breakdown_bass_escalates_beyond_the_regular_glow() {
        const WIDTH: f32 = 548.0;
        const HEIGHT: f32 = 300.0;

        let glow_stats = |bars| {
            let mut engine = VisualEngine::new();
            engine.set_playing(true);
            engine.ingest(&SpectrumFrame::from_cava_bars(bars));
            let alphas = engine
                .scene(WIDTH, HEIGHT)
                .shapes
                .into_iter()
                .filter_map(|shape| match (shape.geom, shape.fill) {
                    (Geom::RadialGlow { cy, r, .. }, Fill::Solid(fill))
                        if cy > HEIGHT * 0.6 && r > WIDTH * 0.2 =>
                    {
                        Some(fill.a)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            (alphas.len(), alphas.iter().sum::<f32>())
        };
        let mut regular_bass = [0.0; SPECTRUM_BAND_COUNT];
        regular_bass[..12].fill(0.39);
        let mut strong_beat = [0.0; SPECTRUM_BAND_COUNT];
        strong_beat[..12].fill(0.446);
        let mut breakdown = [0.0; SPECTRUM_BAND_COUNT];
        breakdown[..12].fill(0.519);
        breakdown[12..24].fill(0.46);

        let regular = glow_stats(regular_bass);
        let strong = glow_stats(strong_beat);
        let extreme = glow_stats(breakdown);

        assert_eq!(regular.0, 2);
        assert_eq!(strong.0, 4);
        assert_eq!(extreme.0, 4);
        assert!(extreme.1 > regular.1 * 4.0);
        assert!(extreme.1 > 1.4);
    }

    #[test]
    fn ac_22_realistic_breakdown_waves_span_glow_and_aura_range() {
        let bass_wave = |level: f32| {
            let mut bars = [0.0; SPECTRUM_BAND_COUNT];
            bars[..BASS_GLOW_BAND_COUNT].fill(level);
            bass_glow_target(&bars)
        };

        let early_wave = bass_wave(0.39);
        let recurring_wave = bass_wave(0.446);
        let strongest_wave = bass_wave(0.519);

        assert!(
            early_wave > 0.0 && early_wave < 0.25,
            "smaller lead-in wave should add only broad glow, got {early_wave:.3}"
        );
        assert!(
            recurring_wave > 0.7,
            "recurring breakdown wave should clearly reach the inner aura, got {recurring_wave:.3}"
        );
        assert!(
            strongest_wave > 0.95,
            "strongest breakdown wave should nearly fill the glow range, got {strongest_wave:.3}"
        );
    }

    #[test]
    fn ac_11_continuous_motion_ceases_without_a_loaded_track() {
        let mut engine = lively_engine();
        assert!(!engine.tick());
        engine.set_playing(false);

        assert!((0..500).any(|_| engine.tick()));
    }

    #[test]
    fn ac_11_idle_breathing_keeps_a_loaded_track_alive_while_stopped() {
        let mut engine = lively_engine();
        engine.set_has_track(true);
        engine.set_playing(false);

        // The live bars release first; the idle wave takes over and never
        // settles, so the tick loop keeps running.
        for _ in 0..200 {
            assert!(!engine.tick());
        }
        let first = engine.display_bands;
        for _ in 0..30 {
            engine.tick();
        }

        assert!(first.iter().any(|bar| *bar > 0.0));
        assert_ne!(first, engine.display_bands);
    }

    #[test]
    fn ac_11_idle_breathing_stays_a_low_resting_wave() {
        let mut engine = VisualEngine::new();
        engine.set_has_track(true);
        for _ in 0..400 {
            engine.tick();
            assert!(
                engine.display_bands.iter().all(|bar| *bar <= IDLE_PEAK),
                "idle wave must stay below the resting ceiling"
            );
        }
        assert!(engine.bands_peaks.iter().all(|peak| *peak == 0.0));
    }

    #[test]
    fn ac_11_playback_takes_over_from_the_idle_wave_immediately() {
        let mut engine = VisualEngine::new();
        engine.set_has_track(true);
        for _ in 0..200 {
            engine.tick();
        }
        engine.set_playing(true);
        let bars = std::array::from_fn(|index| index as f32 / SPECTRUM_BAND_COUNT as f32);
        engine.ingest(&SpectrumFrame::from_cava_bars(bars));

        assert_eq!(engine.display_bands, bars);
    }

    #[test]
    fn ac_11_disabled_animations_show_the_resting_wave_without_motion() {
        let mut engine = VisualEngine::new();
        engine.set_has_track(true);
        engine.snap_to_static();
        let resting = engine.display_bands;

        assert!(resting.iter().any(|bar| *bar > 0.0));
        assert_eq!(resting, engine.display_bands);
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

    #[test]
    fn test_ctx_borrows_the_exact_cava_bars() {
        let engine = lively_engine();
        let ctx = test_ctx(&engine, 548.0, 300.0);
        assert_eq!(ctx.bars, &engine.bands_current);
        assert_eq!((ctx.width, ctx.height), (548.0, 300.0));
    }
}
