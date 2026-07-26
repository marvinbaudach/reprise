//! Portable scene adapter for already-smoothed CAVA bars.
//!
//! Signal processing ends at [`SpectrumFrame`]. This module deliberately does
//! not remap, normalize, or ease live bar heights a second time; it only keeps
//! the visual peak caps and the pause/stop fade required by the UI contract.

use crate::playback::{SpectrumFrame, SPECTRUM_BAND_COUNT};

use super::color::{hue_shift, secondary_accent};
use super::modes;
use super::scene::{Fill, Geom, Rgba, Scene, Shape};

const PEAK_DECAY: f32 = 0.018;
const STOP_RELEASE: f32 = 0.12;
const SETTLE_EPSILON: f32 = 0.002;
const FALLBACK_ACCENT2_HUE_SHIFT: f32 = 42.0;

/// Borrowed render inputs for the Bars scene builder.
pub struct ModeCtx<'a> {
    pub peaks: &'a [f32; SPECTRUM_BAND_COUNT],
    pub bars: &'a [f32; SPECTRUM_BAND_COUNT],
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
    playing: bool,
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
            playing: false,
            accent: (0.5, 0.5, 0.5),
            cover_accent2: None,
        }
    }

    /// Enables live frames or begins the visual-only stop fade.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
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
    }

    /// Installs the already-bounded CAVA values in the same frame.
    pub fn ingest(&mut self, frame: &SpectrumFrame) {
        self.bands_current = *frame.bands();
        for (peak, current) in self.bands_peaks.iter_mut().zip(self.bands_current.iter()) {
            *peak = peak.max(*current);
        }
    }

    /// Advances peak caps and, only while stopped, fades the static bars out.
    pub fn tick(&mut self) -> bool {
        let mut settled = true;
        if !self.playing {
            for bar in &mut self.bands_current {
                *bar += (0.0 - *bar) * STOP_RELEASE;
                if *bar < SETTLE_EPSILON {
                    *bar = 0.0;
                }
                settled &= *bar == 0.0;
            }
        }
        for (peak, current) in self.bands_peaks.iter_mut().zip(self.bands_current.iter()) {
            *peak = (*peak - PEAK_DECAY).max(*current);
            if *peak < SETTLE_EPSILON {
                *peak = 0.0;
            }
            settled &= (*peak - *current).abs() < SETTLE_EPSILON;
        }
        settled && !self.playing
    }

    /// Removes visual-only motion without changing the current CAVA frame.
    pub fn snap_to_static(&mut self) {
        self.bands_peaks = self.bands_current;
    }

    pub fn accent2(&self) -> (f32, f32, f32) {
        self.cover_accent2
            .unwrap_or_else(|| hue_shift(self.accent, FALLBACK_ACCENT2_HUE_SHIFT))
    }

    fn make_ctx(&self, width: f32, height: f32) -> ModeCtx<'_> {
        ModeCtx {
            peaks: &self.bands_peaks,
            bars: &self.bands_current,
            accent: self.accent,
            accent2: self.accent2(),
            width,
            height,
        }
    }

    pub fn scene(&self, width: f32, height: f32) -> Scene {
        let ctx = self.make_ctx(width, height);
        let level = self.bands_current.iter().sum::<f32>() / SPECTRUM_BAND_COUNT as f32;
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
    use crate::visuals::color;

    #[test]
    fn bars_builds_a_finite_sane_nonempty_scene() {
        let scene = lively_engine().scene(548.0, 300.0);
        assert!(scene.shapes.len() > 1);
        assert!(scene.is_finite_and_sane(548.0, 300.0));
    }

    #[test]
    fn ac_21_ingest_uses_cava_values_without_a_second_envelope() {
        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        let bars = std::array::from_fn(|index| index as f32 / SPECTRUM_BAND_COUNT as f32);

        engine.ingest(&SpectrumFrame::from_cava_bars(bars));

        assert_eq!(engine.bands_current, bars);
    }

    #[test]
    fn ac_11_continuous_motion_ceases_when_playback_stops() {
        let mut engine = lively_engine();
        assert!(!engine.tick());
        engine.set_playing(false);

        assert!((0..500).any(|_| engine.tick()));
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
