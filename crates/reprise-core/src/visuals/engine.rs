//! Portable scene adapter for already-smoothed CAVA bars.
//!
//! Signal processing ends at [`SpectrumFrame`]. This module deliberately does
//! not remap or normalize live bar heights a second time; it only keeps the
//! visual peak caps, presentation-only bass glow, and paused resting motion
//! required by the UI contract.

use std::time::Duration;

use crate::playback::{BassPressure, SpectrumFrame, SPECTRUM_BAND_COUNT};

use super::color::hue_shift;
use super::modes;
use super::scene::{Fill, Geom, Rgba, Scene, Shape};

const PEAK_DECAY: f32 = 0.018;
const NO_TRACK_RELEASE: f32 = 0.12;
const SETTLE_EPSILON: f32 = 0.002;
const FALLBACK_ACCENT2_HUE_SHIFT: f32 = 42.0;
/// Per-tick release of the glow layer once playback stops.
const GLOW_RELEASE: f32 = 0.06;
/// Fixed-step rate retained by [`VisualEngine::tick`].
const SIMULATION_TICKS_PER_SECOND: f32 = 60.0;
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
/// Fade-in per tick once playback pauses (≈0.4 s to full amplitude).
const IDLE_FADE_IN: f32 = 0.04;
/// Last-live-shape floor while playback rests.
const PAUSED_LIVE_FLOOR: f32 = 0.10;
/// How much of the last live band distribution remains in the resting shape.
const PAUSED_LIVE_SHAPE: f32 = 0.20;
/// Height of the travelling wave layered onto the retained live shape.
const PAUSED_LIVE_WAVE: f32 = 0.08;
/// Full travelling crests across the field, so each field third averages one.
const PAUSED_LIVE_WAVE_COUNT: f32 = 3.0;

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

/// One audio frame together with the time it represents.
///
/// The tuple conversion is the elapsed-time path used by live callers. The
/// borrowed-frame conversion retains the established one-tick contract for
/// fixed-rate adapters outside the desktop strand.
#[doc(hidden)]
pub struct VisualIngest<'a> {
    frame: &'a SpectrumFrame,
    elapsed: Duration,
}

impl<'a> From<(&'a SpectrumFrame, Duration)> for VisualIngest<'a> {
    fn from((frame, elapsed): (&'a SpectrumFrame, Duration)) -> Self {
        Self { frame, elapsed }
    }
}

impl<'a> From<&'a SpectrumFrame> for VisualIngest<'a> {
    fn from(frame: &'a SpectrumFrame) -> Self {
        Self {
            frame,
            elapsed: Duration::from_secs_f32(1.0 / SIMULATION_TICKS_PER_SECOND),
        }
    }
}

impl ModeCtx<'_> {
    /// Solid fill in the primary effective accent.
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
    /// track is loaded but not playing (AC-27).
    display_bands: [f32; SPECTRUM_BAND_COUNT],
    /// The absolute bass measurement the glow layer draws from (AC-23). The
    /// engine never derives it from the bars, which CAVA keeps re-normalizing.
    pressure: BassPressure,
    /// The stage light itself: attacked by `kick`, released per frame. Kept
    /// apart from `pressure` so the analysis readout keeps reporting the raw
    /// detector values rather than what the glow happens to be doing.
    glow: f32,
    playing: bool,
    has_track: bool,
    retain_paused_live_shape: bool,
    idle_phase: f32,
    idle_amp: f32,
    accent: (f32, f32, f32),
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
            glow: 0.0,
            playing: false,
            has_track: false,
            retain_paused_live_shape: true,
            idle_phase: 0.0,
            idle_amp: 0.0,
            accent: (0.5, 0.5, 0.5),
        }
    }

    /// Switches immediately between live authority and the paused projection.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        if playing {
            self.idle_amp = 0.0;
        } else if !self.retain_paused_live_shape {
            // Stored-analysis adapters deliberately keep the generic resting
            // fallback, including its existing cap-free paused projection.
            self.bands_peaks = [0.0; SPECTRUM_BAND_COUNT];
        }
        self.refresh_display_bands();
    }

    /// Whether a track is loaded at all. Without one there is nothing to keep
    /// alive, so the canvas rests fully empty (AC-27).
    pub fn set_has_track(&mut self, has_track: bool) {
        self.has_track = has_track;
        if !has_track {
            self.idle_amp = 0.0;
            self.idle_phase = 0.0;
        }
        self.refresh_display_bands();
    }

    /// Selects whether a non-playing scene may derive its rest shape from the
    /// current bands. Live CAVA clients keep the default; stored-analysis
    /// adapters disable it so their existing generic fallback stays intact.
    pub fn set_retain_paused_live_shape(&mut self, retain: bool) {
        self.retain_paused_live_shape = retain;
        if !retain && !self.playing {
            self.bands_peaks = [0.0; SPECTRUM_BAND_COUNT];
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

    fn has_live_shape(&self) -> bool {
        self.bands_current.iter().any(|band| *band > 0.0)
    }

    fn paused_live_band(&self, band: usize) -> f32 {
        // Deliberately stop at 63/64: unlike the tapered generic idle wave,
        // this untapered field must not duplicate its first phase at the edge.
        let across = band as f32 / SPECTRUM_BAND_COUNT as f32;
        let wave =
            (std::f32::consts::TAU * (across * PAUSED_LIVE_WAVE_COUNT - self.idle_phase)).sin();
        PAUSED_LIVE_FLOOR + PAUSED_LIVE_SHAPE * self.bands_current[band] + PAUSED_LIVE_WAVE * wave
    }

    /// Live bars hand over smoothly to a resting shape derived from their last
    /// distribution. With no live shape, the generic idle wave remains.
    fn refresh_display_bands(&mut self) {
        if !self.idle_active() {
            self.display_bands = self.bands_current;
            return;
        }
        let has_live_shape = self.retain_paused_live_shape && self.has_live_shape();
        let blend = self.idle_amp * self.idle_amp * (3.0 - 2.0 * self.idle_amp);
        let bands = std::array::from_fn(|band| {
            let resting = if has_live_shape {
                self.paused_live_band(band)
            } else {
                self.idle_band(band)
            };
            self.bands_current[band] + (resting - self.bands_current[band]) * blend
        });
        self.display_bands = bands;
    }

    pub fn set_accent(&mut self, rgb: (f32, f32, f32)) {
        self.accent = rgb;
    }

    /// Clears the previous track's bar and peak-cap history.
    pub fn note_track_changed(&mut self) {
        self.bands_current = [0.0; SPECTRUM_BAND_COUNT];
        self.bands_peaks = [0.0; SPECTRUM_BAND_COUNT];
        self.pressure = BassPressure::silent();
        self.glow = 0.0;
        self.refresh_display_bands();
    }

    /// Installs the already-bounded CAVA values in the same frame.
    ///
    /// The caller supplies the elapsed time since its previous audio frame so
    /// live peak-cap decay remains deterministic and independent of redraws.
    ///
    /// The glow is a stage light: a hit throws it to full at once, then it
    /// falls. It is sourced from `kick`, not `impact` — measured over three
    /// real tracks, `impact` tops out at 0.85 on a heavily limited master and
    /// never reaches full at all, while `kick` reaches 1.00 on all three. The
    /// fall is applied here rather than taken from the detector, because
    /// `kick`'s own release is 70 ms: at the 12.6 hits per second a blast beat
    /// produces, passing it straight through would be a 12 Hz strobe.
    pub fn ingest<'a>(&mut self, input: impl Into<VisualIngest<'a>>) {
        let VisualIngest { frame, elapsed } = input.into();
        self.bands_current = *frame.bands();
        self.pressure = frame.bass_pressure();
        self.glow = self.glow.max(self.pressure.kick);
        let elapsed_ticks = elapsed.as_secs_f32() * SIMULATION_TICKS_PER_SECOND;
        if !self.playing && !self.retain_paused_live_shape {
            self.bands_peaks = [0.0; SPECTRUM_BAND_COUNT];
        } else if self.playing {
            for (peak, current) in self.bands_peaks.iter_mut().zip(self.bands_current.iter()) {
                *peak = (*peak - PEAK_DECAY * elapsed_ticks).max(*current);
            }
        }
        self.refresh_display_bands();
    }

    /// Advances presentation state by real elapsed time.
    ///
    /// Frontends call this at their own redraw cadence. The elapsed duration,
    /// rather than the number of rendered frames, keeps AC-27's resting wave
    /// on the same six-second clock under load and at reduced frame rates.
    pub fn advance_by(&mut self, elapsed: Duration) -> bool {
        self.advance_ticks(elapsed.as_secs_f32() * SIMULATION_TICKS_PER_SECOND)
    }

    /// Advances one legacy 60 Hz simulation step.
    pub fn tick(&mut self) -> bool {
        self.advance_ticks(1.0)
    }

    fn advance_ticks(&mut self, elapsed_ticks: f32) -> bool {
        let mut settled = true;
        if self.idle_active() {
            self.idle_phase = (self.idle_phase + elapsed_ticks / IDLE_PERIOD_TICKS).fract();
            self.idle_amp = (self.idle_amp + IDLE_FADE_IN * elapsed_ticks).min(1.0);
        }
        if !self.playing && !self.has_track {
            let release = 1.0 - (1.0 - NO_TRACK_RELEASE).powf(elapsed_ticks);
            for (bar, peak) in self
                .bands_current
                .iter_mut()
                .zip(self.bands_peaks.iter_mut())
            {
                *bar += (0.0 - *bar) * release;
                if *bar < SETTLE_EPSILON {
                    *bar = 0.0;
                }
                // With no loaded track there is no cap to retain. Follow the
                // existing bar release so a settled scene is actually empty.
                *peak = *bar;
                settled &= *bar == 0.0;
            }
        }
        if !self.playing && self.has_track && self.retain_paused_live_shape {
            // Live ingestion owns decay while playing; the presentation clock
            // owns it while paused, so neither state can apply it twice.
            for peak in &mut self.bands_peaks {
                *peak = (*peak - PEAK_DECAY * elapsed_ticks).max(0.0);
                if *peak < SETTLE_EPSILON {
                    *peak = 0.0;
                }
            }
        }
        // The stage light falls on every frame, playing or not: the attack
        // lands in `ingest`, the decay belongs to the render clock. Without a
        // fall here the light would simply latch on at the first hit.
        self.glow = (self.glow - GLOW_RELEASE * elapsed_ticks).max(0.0);
        settled &= self.glow == 0.0;
        if !self.playing {
            // No fresh measurements arrive once playback stops, so the two
            // detector readings are released here as well rather than waiting
            // for a frame that never comes.
            for value in [&mut self.pressure.impact, &mut self.pressure.aura] {
                *value = (*value - GLOW_RELEASE * elapsed_ticks).max(0.0);
                settled &= *value == 0.0;
            }
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
        hue_shift(self.accent, FALLBACK_ACCENT2_HUE_SHIFT)
    }

    fn make_ctx(&self, width: f32, height: f32) -> ModeCtx<'_> {
        ModeCtx {
            peaks: &self.bands_peaks,
            bars: &self.display_bands,
            bass_impact: self.glow,
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
    engine.ingest((
        &SpectrumFrame::from_cava_bars(std::array::from_fn(|index| {
            0.55 + index as f32 / SPECTRUM_BAND_COUNT as f32 * 0.4
        })),
        Duration::from_micros(16_667),
    ));
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

        engine.ingest((
            &SpectrumFrame::from_cava_bars(bars),
            Duration::from_micros(16_667),
        ));

        assert_eq!(engine.bands_current, bars);
    }

    const WIDTH: f32 = 548.0;
    const HEIGHT: f32 = 300.0;

    /// One engine holding `bars` on screen and `pressure` in its glow layer.
    fn engine_with(bars: [f32; SPECTRUM_BAND_COUNT], pressure: BassPressure) -> VisualEngine {
        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        engine.ingest((
            &SpectrumFrame::from_cava_bars(bars).with_bass_pressure(pressure),
            Duration::from_micros(16_667),
        ));
        engine
    }

    fn paused_live_engine() -> VisualEngine {
        let bars = std::array::from_fn(|index| 0.2 + index as f32 * 0.7 / 63.0);
        let mut engine = engine_with(bars, BassPressure::silent());
        engine.set_has_track(true);
        engine.set_playing(false);
        engine
    }

    #[test]
    fn ac_27_paused_live_bands_keep_moving_inside_the_resting_range() {
        let mut engine = paused_live_engine();
        let live = engine.bands_current;
        assert_eq!(engine.display_bands, live, "pause introduced a jump");
        engine.tick();
        assert!(
            engine
                .display_bands
                .iter()
                .zip(live)
                .all(|(paused, live)| (paused - live).abs() < 0.01),
            "the resting shape did not fade in softly"
        );
        for _ in 1..120 {
            engine.tick();
        }

        let mut previous = engine.display_bands;
        let mut changed_samples = 0;
        for _ in 0..8 {
            for _ in 0..30 {
                engine.tick();
            }
            let current = engine.display_bands;
            assert!(
                current.iter().all(|band| (0.04..=0.38).contains(band)),
                "paused live bands left the resting range: {current:?}"
            );
            changed_samples += usize::from(current != previous);
            previous = current;
        }

        assert_eq!(changed_samples, 8, "the paused live scene stopped moving");
    }

    #[test]
    fn ac_27_paused_live_bands_cover_a_clearly_visible_range() {
        let mut engine = paused_live_engine();
        for _ in 0..120 {
            engine.tick();
        }
        let checked_bands = [0, 8, 17, 31, 40, 63];
        let mut minima = [f32::INFINITY; 6];
        let mut maxima = [f32::NEG_INFINITY; 6];

        for _ in 0..IDLE_PERIOD_TICKS as usize {
            engine.tick();
            for (sample, band) in checked_bands.into_iter().enumerate() {
                minima[sample] = minima[sample].min(engine.display_bands[band]);
                maxima[sample] = maxima[sample].max(engine.display_bands[band]);
            }
        }

        for ((band, minimum), maximum) in checked_bands.into_iter().zip(minima).zip(maxima) {
            let span = maximum - minimum;
            assert!(
                span > 0.14,
                "paused band {band} moved through only {span} ({minimum}..={maximum})"
            );
        }
    }

    #[test]
    fn ac_27_paused_live_bands_return_near_their_start_instead_of_drifting() {
        let mut engine = paused_live_engine();
        for _ in 0..120 {
            engine.tick();
        }
        let starting = engine.display_bands[17];
        let mut was_near = true;
        let mut returns = 0;

        for _ in 0..(IDLE_PERIOD_TICKS as usize * 3) {
            engine.tick();
            let is_near = (engine.display_bands[17] - starting).abs() < 0.0015;
            if is_near && !was_near {
                returns += 1;
            }
            was_near = is_near;
        }

        assert!(
            returns >= 5,
            "paused band drifted instead of returning, saw {returns} returns"
        );
    }

    #[test]
    fn ac_27_paused_live_bands_move_out_of_phase() {
        let mut engine = paused_live_engine();
        for _ in 0..120 {
            engine.tick();
        }
        let before = engine.display_bands;

        engine.tick();

        let low_delta = engine.display_bands[8] - before[8];
        let high_delta = engine.display_bands[40] - before[40];
        assert!(
            low_delta.abs() > 0.0001 && high_delta.abs() > 0.0001,
            "chosen bands did not move clearly: {low_delta}, {high_delta}"
        );
        assert!(
            low_delta.signum() != high_delta.signum(),
            "paused bands moved in sync: {low_delta}, {high_delta}"
        );
    }

    #[test]
    fn ac_27_resumed_live_bands_take_over_before_another_ingest() {
        let mut engine = paused_live_engine();
        let live = engine.bands_current;
        for _ in 0..180 {
            engine.tick();
        }
        assert_ne!(engine.display_bands, live);

        engine.set_playing(true);

        assert_eq!(engine.display_bands, live);
    }

    /// A stage light: the hit throws it to full, then it falls.
    #[test]
    fn ac_23_a_bass_hit_throws_the_glow_to_full_and_then_it_falls() {
        let mut engine = VisualEngine::new();
        engine.set_has_track(true);
        engine.set_playing(true);

        // A full kick arrives. The attack is immediate — no easing, no ramp.
        engine.ingest((
            &frame_with(BassPressure {
                kick: 1.0,
                ..pressure(0.0, 0.0)
            }),
            Duration::from_micros(16_667),
        ));
        assert!(
            engine.glow >= 1.0 - f32::EPSILON,
            "the hit did not reach full: {}",
            engine.glow
        );

        // Silence afterwards: it falls, and it falls all the way.
        for _ in 0..2 {
            engine.tick();
        }
        let after_two = engine.glow;
        assert!(after_two < 1.0, "the light latched on: {after_two}");
        for _ in 0..60 {
            engine.tick();
        }
        assert_eq!(engine.glow, 0.0, "the light never went out");
    }

    /// The reason this changed at all: `impact` cannot reach full on a
    /// limited master — measured over three real tracks it tops out at 0.85 —
    /// so the glow must not be sourced from it.
    #[test]
    fn ac_23_the_glow_reads_the_kick_and_not_the_impact() {
        let mut engine = VisualEngine::new();
        engine.set_has_track(true);
        engine.set_playing(true);

        engine.ingest((
            &frame_with(BassPressure {
                kick: 0.0,
                ..pressure(1.0, 1.0)
            }),
            Duration::from_micros(16_667),
        ));
        assert_eq!(
            engine.glow, 0.0,
            "a maxed-out impact must not light the stage on its own"
        );

        engine.ingest((
            &frame_with(BassPressure {
                kick: 0.8,
                ..pressure(0.0, 0.0)
            }),
            Duration::from_micros(16_667),
        ));
        assert!((engine.glow - 0.8).abs() < 1e-6, "got {}", engine.glow);
    }

    fn frame_with(reading: BassPressure) -> SpectrumFrame {
        SpectrumFrame::from_cava_bars([0.0; SPECTRUM_BAND_COUNT]).with_bass_pressure(reading)
    }

    /// A reading whose *attack* is `kick` — what the stage light runs on.
    fn hit(kick: f32, aura: f32) -> BassPressure {
        BassPressure {
            kick,
            ..pressure(0.0, aura)
        }
    }

    fn pressure(impact: f32, aura: f32) -> BassPressure {
        BassPressure {
            level_dbfs: -14.0,
            baseline_dbfs: -20.0,
            impact,
            aura,
            kick: 0.0,
            pressure: 0.0,
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
    fn ac_23_the_measured_kick_ignites_the_broad_glows() {
        // Bars stay empty; only the attack reading drives the stage light.
        let engine = engine_with([0.0; SPECTRUM_BAND_COUNT], hit(1.0, 0.0));

        assert_eq!(broad_glow_alphas(&engine).len(), 2);
    }

    #[test]
    fn ac_23_a_rhythmic_kick_glows_softer_than_a_full_drop() {
        let kick = engine_with([0.4; SPECTRUM_BAND_COUNT], hit(STEADY_GLOW, 0.0));
        let drop = engine_with([0.4; SPECTRUM_BAND_COUNT], hit(1.0, 0.0));

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
        let kicking = engine_with([0.4; SPECTRUM_BAND_COUNT], hit(1.0, 0.0));
        let breakdown = engine_with([0.4; SPECTRUM_BAND_COUNT], hit(1.0, 1.0));

        assert_eq!(broad_glow_alphas(&kicking).len(), 2);
        assert_eq!(broad_glow_alphas(&breakdown).len(), 4);
    }

    #[test]
    fn ac_23_the_glow_leaves_with_the_track_when_playback_stops() {
        let mut engine = engine_with([0.4; SPECTRUM_BAND_COUNT], hit(1.0, 1.0));
        engine.set_playing(false);

        for _ in 0..200 {
            engine.tick();
        }

        assert!(broad_glow_alphas(&engine).is_empty());
    }

    #[test]
    fn accent2_is_always_hue_shifted_from_the_effective_accent() {
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

#[cfg(test)]
mod engine_timing_tests;

#[cfg(test)]
mod peak_visibility_tests;
