//! Portable scene adapter for already-smoothed CAVA bars.
//!
//! Signal processing ends at [`SpectrumFrame`]. This module deliberately does
//! not remap, normalize, or ease live bar heights a second time; it only keeps
//! the visual peak caps, presentation-only bass glow, and pause/stop fade
//! required by the UI contract.

use crate::playback::{BassPressure, SpectrumFrame, SPECTRUM_BAND_COUNT};

use super::color::{hue_shift, secondary_accent};
use super::modes;
use super::scene::{Fill, Geom, Rgba, Scene, Shape};

const PEAK_DECAY: f32 = 0.018;
const STOP_RELEASE: f32 = 0.12;
const SETTLE_EPSILON: f32 = 0.002;
const FALLBACK_ACCENT2_HUE_SHIFT: f32 = 42.0;
/// Per-tick release of the glow layer once playback stops.
const GLOW_RELEASE: f32 = 0.06;
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
    /// Glow a rhythmic kick reaches, `0..=1` (AC-23).
    pub bass_impact: f32,
    /// Inner aura of a sustained breakdown, `0..=1` (AC-23).
    pub bass_aura: f32,
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
    /// The absolute bass measurement the glow layer draws from (AC-23). The
    /// engine never derives it from the bars, which CAVA keeps re-normalizing.
    pressure: BassPressure,
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
            pressure: BassPressure::silent(),
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
        self.pressure = BassPressure::silent();
        self.refresh_display_bands();
    }

    /// Installs the already-bounded CAVA values in the same frame. The glow
    /// layer takes the frame's own measurement — attack and release already
    /// live in the detector, so the engine adds no second envelope.
    pub fn ingest(&mut self, frame: &SpectrumFrame) {
        self.bands_current = *frame.bands();
        self.pressure = frame.bass_pressure();
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
        if !self.playing {
            // No fresh measurements arrive once playback stops, so the glow is
            // released here rather than waiting for a frame that never comes.
            for glow in [&mut self.pressure.impact, &mut self.pressure.aura] {
                *glow = (*glow - GLOW_RELEASE).max(0.0);
                settled &= *glow == 0.0;
            }
        }
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
        if self.idle_active() {
            self.idle_amp = 1.0;
        }
        self.refresh_display_bands();
    }

    /// The bass measurement currently driving the glow layer, for surfaces
    /// that show what the visualizer is reacting to.
    pub fn bass_pressure(&self) -> BassPressure {
        self.pressure
    }

    pub fn accent2(&self) -> (f32, f32, f32) {
        self.cover_accent2
            .unwrap_or_else(|| hue_shift(self.accent, FALLBACK_ACCENT2_HUE_SHIFT))
    }

    fn make_ctx(&self, width: f32, height: f32) -> ModeCtx<'_> {
        ModeCtx {
            peaks: &self.bands_peaks,
            bars: &self.display_bands,
            bass_impact: self.pressure.impact,
            bass_aura: self.pressure.aura,
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
    use crate::playback::STEADY_GLOW;
    use crate::visuals::color;

    #[test]
    fn bars_builds_a_finite_sane_nonempty_scene() {
        let scene = lively_engine().scene(548.0, 300.0);
        assert!(scene.shapes.len() > 1);
        assert!(scene.is_finite_and_sane(548.0, 300.0));
    }

    #[test]
    fn ac_23_ingest_uses_cava_values_without_a_second_bar_envelope() {
        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        let bars = std::array::from_fn(|index| index as f32 / SPECTRUM_BAND_COUNT as f32);

        engine.ingest(&SpectrumFrame::from_cava_bars(bars));

        assert_eq!(engine.bands_current, bars);
    }

    const WIDTH: f32 = 548.0;
    const HEIGHT: f32 = 300.0;

    /// One engine holding `bars` on screen and `pressure` in its glow layer.
    fn engine_with(bars: [f32; SPECTRUM_BAND_COUNT], pressure: BassPressure) -> VisualEngine {
        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        engine.ingest(&SpectrumFrame::from_cava_bars(bars).with_bass_pressure(pressure));
        engine
    }

    fn pressure(impact: f32, aura: f32) -> BassPressure {
        BassPressure {
            level_dbfs: -14.0,
            baseline_dbfs: -20.0,
            impact,
            aura,
        }
    }

    /// Alphas of the broad glows that sit low behind the columns.
    fn broad_glow_alphas(engine: &VisualEngine) -> Vec<f32> {
        engine
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
            .collect()
    }

    #[test]
    fn ac_23_loud_cava_bass_bands_alone_never_ignite_the_glow() {
        // The exact failure this replaced: CAVA's auto-sensitivity lifts the
        // low bands during a quiet sung passage until they read like a drop.
        let mut bars = [0.0; SPECTRUM_BAND_COUNT];
        bars[..12].fill(0.95);

        let engine = engine_with(bars, pressure(0.0, 0.0));

        assert!(broad_glow_alphas(&engine).is_empty());
    }

    #[test]
    fn ac_23_the_measured_impact_ignites_the_broad_glows() {
        // Bars stay empty; only the absolute measurement drives the glow.
        let engine = engine_with([0.0; SPECTRUM_BAND_COUNT], pressure(1.0, 0.0));

        assert_eq!(broad_glow_alphas(&engine).len(), 2);
    }

    #[test]
    fn ac_23_a_rhythmic_kick_glows_softer_than_a_full_drop() {
        let kick = engine_with([0.4; SPECTRUM_BAND_COUNT], pressure(STEADY_GLOW, 0.0));
        let drop = engine_with([0.4; SPECTRUM_BAND_COUNT], pressure(1.0, 0.0));

        let kick_alpha = broad_glow_alphas(&kick).iter().sum::<f32>();
        let drop_alpha = broad_glow_alphas(&drop).iter().sum::<f32>();

        assert!(kick_alpha > 0.0, "a rhythmic kick still glows softly");
        assert!(
            kick_alpha < drop_alpha * 0.5,
            "a kick must stay clearly below a full drop, got {kick_alpha:.3} vs {drop_alpha:.3}"
        );
    }

    #[test]
    fn ac_23_only_a_sustained_breakdown_adds_the_inner_auras() {
        let kicking = engine_with([0.4; SPECTRUM_BAND_COUNT], pressure(1.0, 0.0));
        let breakdown = engine_with([0.4; SPECTRUM_BAND_COUNT], pressure(1.0, 1.0));

        assert_eq!(broad_glow_alphas(&kicking).len(), 2);
        assert_eq!(broad_glow_alphas(&breakdown).len(), 4);
    }

    #[test]
    fn ac_23_the_glow_leaves_with_the_track_when_playback_stops() {
        let mut engine = engine_with([0.4; SPECTRUM_BAND_COUNT], pressure(1.0, 1.0));
        engine.set_playing(false);

        for _ in 0..200 {
            engine.tick();
        }

        assert!(broad_glow_alphas(&engine).is_empty());
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
