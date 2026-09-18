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

    /// The bars actually on screen right now — `display_bands`, not the raw
    /// last-ingested `bands_current`. Those two diverge the moment the engine
    /// is not playing (AC-27's idle blend, or the paused-live projection):
    /// reading the raw bands there would seed a fresh sibling engine with
    /// energy the screen had already decayed away, reintroducing it as a
    /// visible "pop" the instant the sibling adopts it. Lets a fresh sibling
    /// engine (e.g. the panel that has just become live during a swipe)
    /// adopt the shape the viewer actually saw instead of climbing from zero
    /// or jumping from one it never saw.
    pub fn current_bands(&self) -> &[f32; SPECTRUM_BAND_COUNT] {
        &self.display_bands
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
        accent2_of(self.accent)
    }

    fn make_ctx(&self, width: f32, height: f32, accent: (f32, f32, f32)) -> ModeCtx<'_> {
        ModeCtx {
            peaks: &self.bands_peaks,
            bars: &self.display_bands,
            bass_impact: self.glow,
            bass_aura: self.pressure.aura,
            accent,
            accent2: accent2_of(accent),
            width,
            height,
        }
    }

    pub fn scene(&self, width: f32, height: f32) -> Scene {
        self.scene_with_accent(width, height, self.accent)
    }

    /// The same scene painted in another accent, without touching the engine's
    /// own. A surface that mirrors one engine's motion into several panels --
    /// the neighbours during a swipe -- reads it this way, each in its own
    /// cover's colour, while the engine keeps the live panel's.
    pub fn scene_with_accent(&self, width: f32, height: f32, accent: (f32, f32, f32)) -> Scene {
        let ctx = self.make_ctx(width, height, accent);
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

/// The secondary accent every scene derives from its primary one, whether the
/// engine's own or a tint asked for by a mirroring panel.
fn accent2_of(accent: (f32, f32, f32)) -> (f32, f32, f32) {
    hue_shift(accent, FALLBACK_ACCENT2_HUE_SHIFT)
}

#[cfg(test)]
pub(crate) fn test_ctx(engine: &VisualEngine, width: f32, height: f32) -> ModeCtx<'_> {
    engine.make_ctx(width, height, engine.accent)
}

#[cfg(test)]
mod engine_tests;

#[cfg(test)]
mod engine_timing_tests;

#[cfg(test)]
mod peak_visibility_tests;
