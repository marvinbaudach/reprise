//! The portable visual engine: owns every piece of reactive state a
//! visualizer needs (eased bands, level envelope, membrane, drop flash,
//! accent palette) and turns [`SpectrumFrame`]s into a resolution-independent
//! [`Scene`] a frontend can draw however it likes. No frontend ever touches
//! easing constants or Grid geometry directly — it only feeds frames in
//! via [`VisualEngine::ingest`], advances via [`VisualEngine::tick`],
//! and reads a [`Scene`] back out via [`VisualEngine::scene`].
//!
//! The tick loop always advances by a fixed `1/60` s step, never a
//! wall-clock delta, so behavior (and this module's tests) stay
//! deterministic across platforms and frame-rate hiccups.

use crate::playback::{SpectrumFrame, SPECTRUM_BAND_COUNT};

use super::color::{hue_shift, secondary_accent};
use super::impact::ImpactState;
use super::membrane::Membrane;
use super::modes;
use super::scene::{Fill, Geom, Rgba, Scene, Shape};

/// Bands rise fast (attack) and fall slowly (release): the asymmetry is what
/// makes transients punch instead of averaging away. Fast so the visuals stay
/// responsive; the noise-floor jitter is suppressed upstream (the analyzer's
/// gamma + AGC floor squash faint bands) rather than by slowing the rise, so
/// this can stay high without reintroducing flicker.
const BAND_ATTACK: f32 = 0.75;
/// Per-band release floor: band 0 (bass) lingers, high bands sparkle. Snappier
/// than a gentle gauge so bands track fine musical detail instead of gliding
/// smoothly between values.
const BAND_RELEASE_MIN: f32 = 0.13;
const BAND_RELEASE_SPAN: f32 = 0.15;
const SCALAR_ATTACK: f32 = 0.9;
const SCALAR_RELEASE: f32 = 0.22;
/// Below this an eased value reads as "arrived" for settle detection.
const SETTLE_EPSILON: f32 = 0.002;
/// Resting band profile. Zero (not a faint idle shimmer): the membrane is
/// driven by the current bands every tick, so anything above
/// [`super::membrane::Membrane`]'s rest epsilon would keep it perpetually
/// "live" and the engine would never settle once playback stops.
const NEUTRAL_PROFILE: [f32; SPECTRUM_BAND_COUNT] = [0.0; SPECTRUM_BAND_COUNT];
/// Secondary accent hue offset when no cover-derived accent is available.
const FALLBACK_ACCENT2_HUE_SHIFT: f32 = 42.0;

/// The per-frame render inputs the Grid scene builder needs. Borrowed from
/// the engine for the lifetime of one [`VisualEngine::scene`] call — the
/// renderer never owns or mutates engine state, it only reads it.
pub struct GridCtx<'a> {
    pub accent: (f32, f32, f32),
    pub accent2: (f32, f32, f32),
    pub membrane: &'a Membrane,
    pub width: f32,
    pub height: f32,
}

impl GridCtx<'_> {
    /// Solid fill in the primary (app cover) accent color.
    pub fn accent_fill(&self, alpha: f32) -> Fill {
        let (r, g, b) = self.accent;
        Fill::Solid(Rgba { r, g, b, a: alpha })
    }

    /// Solid fill in the secondary (complementary/cover) accent color.
    pub fn accent2_fill(&self, alpha: f32) -> Fill {
        let (r, g, b) = self.accent2;
        Fill::Solid(Rgba { r, g, b, a: alpha })
    }
}

/// Owns every piece of state Grid needs across frames: eased spectrum bands,
/// the level envelope, membrane surface, drop flash, and accent palette. Frontends
/// drive it with [`ingest`](Self::ingest)/[`tick`](Self::tick) and
/// read a [`Scene`] back with [`scene`](Self::scene) — no frontend ever
/// touches easing constants or Grid geometry directly.
pub struct VisualEngine {
    bands_current: [f32; SPECTRUM_BAND_COUNT],
    bands_target: [f32; SPECTRUM_BAND_COUNT],
    level_current: f32,
    level_target: f32,
    playing: bool,
    membrane: Membrane,
    impact: ImpactState,
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
            bands_target: [0.0; SPECTRUM_BAND_COUNT],
            level_current: 0.0,
            level_target: 0.0,
            playing: false,
            membrane: Membrane::new(),
            impact: ImpactState::new(),
            accent: (0.5, 0.5, 0.5),
            cover_accent2: None,
        }
    }

    /// Pauses drift (the tick loop stops feeding fresh frames in) and
    /// retargets the bands toward rest so a paused or stopped visual settles
    /// instead of freezing mid-motion.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        if !playing {
            self.bands_target = NEUTRAL_PROFILE;
            self.level_target = 0.0;
        }
    }

    /// Primary accent color, typically the app's cover-derived accent.
    pub fn set_accent(&mut self, rgb: (f32, f32, f32)) {
        self.accent = rgb;
    }

    /// Derives a secondary accent from cover art pixels (falls back to a hue
    /// shift of the primary accent when no distinct second hue is found).
    pub fn set_cover_pixels(&mut self, rgba: &[u8], pixel_count: usize) {
        self.cover_accent2 = secondary_accent(rgba, pixel_count, self.accent);
    }

    pub fn clear_cover(&mut self) {
        self.cover_accent2 = None;
    }

    /// Track switched: reset the membrane and drop flash so leftover motion
    /// from the previous track does not bleed in.
    pub fn note_track_changed(&mut self) {
        self.membrane = Membrane::new();
        self.impact = ImpactState::new();
    }

    /// Feed one enriched spectrum frame in: retargets the eased bands/level
    /// and fires the discrete impact/membrane side effects for beats and drops.
    pub fn ingest(&mut self, frame: &SpectrumFrame) {
        self.bands_target = *frame.bands();
        self.level_target = frame.level();
        let beat = frame.beat();
        if beat.fired {
            // Scale the cone impulse by how hard the beat landed, so big beats
            // push the cloth deeply and soft ones barely vibrate.
            self.membrane.splash(beat.strength);
        }
        self.impact.spawn_drop(frame.dynamics());
    }

    /// One 60 Hz step. Returns `true` once every eased value has arrived at
    /// its target, the impact overlay and membrane are at rest, and
    /// playback is stopped — the frontend may stop ticking at that point.
    pub fn tick(&mut self) -> bool {
        let mut bands_settled = true;
        let last = (SPECTRUM_BAND_COUNT - 1) as f32;
        for (index, (current, &target)) in self
            .bands_current
            .iter_mut()
            .zip(self.bands_target.iter())
            .enumerate()
        {
            let delta = target - *current;
            let release = BAND_RELEASE_MIN + BAND_RELEASE_SPAN * (index as f32 / last);
            let coeff = if delta > 0.0 { BAND_ATTACK } else { release };
            let next = *current + delta * coeff;
            bands_settled &= (target - next).abs() < SETTLE_EPSILON;
            *current = next;
        }

        let level_delta = self.level_target - self.level_current;
        let level_coeff = if level_delta > 0.0 {
            SCALAR_ATTACK
        } else {
            SCALAR_RELEASE
        };
        self.level_current += level_delta * level_coeff;
        let level_settled = (self.level_target - self.level_current).abs() < SETTLE_EPSILON;

        self.impact.advance();
        self.membrane.advance(&self.bands_current);

        bands_settled
            && level_settled
            && self.impact.is_idle()
            && self.membrane.is_still()
            && !self.playing
    }

    /// Snaps every eased value straight to rest for reduced-motion
    /// frontends: no interpolation, no lingering membrane/impact ornaments.
    pub fn snap_to_static(&mut self) {
        self.bands_current = self.bands_target;
        self.level_current = self.level_target;
        self.membrane.reset();
        self.impact = ImpactState::new();
    }

    /// Secondary accent: a cover-derived distinct hue when available,
    /// otherwise the primary accent hue-shifted by a fixed offset.
    pub fn accent2(&self) -> (f32, f32, f32) {
        self.cover_accent2
            .unwrap_or_else(|| hue_shift(self.accent, FALLBACK_ACCENT2_HUE_SHIFT))
    }

    fn make_ctx(&self, width: f32, height: f32) -> GridCtx<'_> {
        GridCtx {
            accent: self.accent,
            accent2: self.accent2(),
            membrane: &self.membrane,
            width,
            height,
        }
    }

    /// Builds the resolution-independent scene for this instant: an accent
    /// wash first, then the Grid shapes, then a soft flash overlay while a
    /// drop/slam is still decaying.
    pub fn scene(&self, width: f32, height: f32) -> Scene {
        let ctx = self.make_ctx(width, height);
        let mut shapes = Vec::new();
        shapes.push(Shape {
            geom: Geom::RadialGlow {
                cx: width / 2.0,
                cy: height * 0.44,
                r: width.max(height) * 0.6,
            },
            fill: ctx.accent_fill(0.05 + 0.11 * self.level_current),
            width: 0.0,
            glow: 0.0,
            dash: None,
        });
        shapes.extend(modes::build_scene(&ctx));
        let flash = self.impact.flash();
        if flash > 0.0 {
            shapes.push(Shape {
                geom: Geom::RadialGlow {
                    cx: width / 2.0,
                    cy: height / 2.0,
                    r: width.max(height),
                },
                fill: ctx.accent_fill(flash.min(0.15)),
                width: 0.0,
                glow: 0.0,
                dash: None,
            });
        }
        Scene { shapes }
    }
}

/// Builds an engine that has just been hammered by a beat-then-slam: playing,
/// accented, 20 silent frames (settles the envelopes at rest) followed by 10
/// full-scale frames (fires a beat and pins the bands high). Shared
/// by Grid tests so they can exercise a "lively" scene without re-deriving
/// this fixture.
#[cfg(test)]
pub(crate) fn lively_engine() -> VisualEngine {
    use crate::playback::{SpectrumAnalyzer, SPECTRUM_ANALYSIS_BAND_COUNT};

    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    engine.set_accent((0.2, 0.7, 0.7));
    let mut analyzer = SpectrumAnalyzer::new();
    for _ in 0..20 {
        engine.ingest(&analyzer.ingest([-80.0; SPECTRUM_ANALYSIS_BAND_COUNT]));
        engine.tick();
    }
    for _ in 0..10 {
        engine.ingest(&analyzer.ingest([0.0; SPECTRUM_ANALYSIS_BAND_COUNT]));
        engine.tick();
    }
    engine
}

/// Borrows a [`GridCtx`] out of an engine for direct Grid testing, without
/// going through the full [`VisualEngine::scene`] wash + flash wrapping.
#[cfg(test)]
pub(crate) fn test_ctx(engine: &VisualEngine, width: f32, height: f32) -> GridCtx<'_> {
    engine.make_ctx(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visuals::color;

    #[test]
    fn grid_builds_a_finite_sane_nonempty_scene() {
        let engine = lively_engine();
        let scene = engine.scene(548.0, 300.0);
        assert!(scene.shapes.len() > 1, "Grid must draw beyond the wash");
        assert!(scene.is_finite_and_sane(548.0, 300.0));
    }

    /// AC-11: continuous motion exists only during playback. A playing engine
    /// fed a fresh impact off silence is still animating (`tick` reports "not
    /// settled"); once stopped it decays to rest within a bounded number of
    /// ticks — the pause/stop "ausklingen" onto the static picture, with no
    /// perpetual tick.
    #[test]
    fn ac_11_continuous_motion_ceases_when_playback_stops() {
        use crate::playback::{SpectrumAnalyzer, SPECTRUM_ANALYSIS_BAND_COUNT};

        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        engine.set_accent((0.2, 0.7, 0.7));
        let mut analyzer = SpectrumAnalyzer::new();
        // A full-scale impact off silence makes every band jump toward its
        // target while the current level lags — the playing engine is
        // unambiguously mid-animation.
        engine.ingest(&analyzer.ingest([0.0; SPECTRUM_ANALYSIS_BAND_COUNT]));
        assert!(
            !engine.tick(),
            "a playing engine fed a fresh impact must still be animating"
        );

        engine.set_playing(false);
        let mut settled = false;
        for _ in 0..5000 {
            if engine.tick() {
                settled = true;
                break;
            }
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

    /// Exercises the `test_ctx` helper Grid tests reuse to build a `GridCtx`
    /// directly, without going through `scene`'s wash and flash wrapping.
    #[test]
    fn test_ctx_borrows_a_matching_grid_ctx() {
        let engine = lively_engine();
        let ctx = test_ctx(&engine, 548.0, 300.0);
        assert_eq!((ctx.width, ctx.height), (548.0, 300.0));
        assert_eq!(ctx.accent, engine.accent);
    }
}
