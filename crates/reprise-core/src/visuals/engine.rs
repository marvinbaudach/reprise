//! The portable visual engine: owns every piece of reactive state a
//! visualizer needs (eased bands, envelopes, water, dust, impact overlay,
//! accent palette) and turns [`SpectrumFrame`]s into a resolution-independent
//! [`Scene`] a frontend can draw however it likes. No frontend ever touches
//! easing constants or per-mode geometry directly — it only feeds frames in
//! via [`VisualEngine::ingest`], steps the clock via [`VisualEngine::tick`],
//! and reads a [`Scene`] back out via [`VisualEngine::scene`].
//!
//! The tick loop always advances by a fixed `1/60` s step, never a
//! wall-clock delta, so behavior (and this module's tests) stay
//! deterministic across platforms and frame-rate hiccups.

use crate::playback::{SpectrumFrame, SPECTRUM_BAND_COUNT};

use super::color::{hsla_to_rgb, hue_shift, secondary_accent};
use super::dust::{advance_dust, make_dust, Dust, DUST_COUNT};
use super::impact::ImpactState;
use super::modes;
use super::scene::{Fill, Geom, Rgba, Scene, Shape};
use super::water::WaterGrid;

/// Bands rise fast (attack) and fall slowly (release): the asymmetry is what
/// makes transients punch instead of averaging away. Not fully instant, so a
/// single jittery frame is smoothed over a few steps rather than snapped to —
/// beats still punch through the separate water splash on beat detection.
const BAND_ATTACK: f32 = 0.72;
/// Per-band release floor: band 0 (bass) lingers, high bands sparkle. Snappier
/// than a gentle gauge so bands track fine musical detail instead of gliding
/// smoothly between values.
const BAND_RELEASE_MIN: f32 = 0.13;
const BAND_RELEASE_SPAN: f32 = 0.15;
const SCALAR_ATTACK: f32 = 0.9;
const SCALAR_RELEASE: f32 = 0.22;
/// Peak-hold markers fall slowly so the frequency picture stays legible.
const PEAK_DECAY: f32 = 0.02;
/// `mid`/`high` envelopes: instant rise, slow release, same shape as `kick`.
const MID_HIGH_RELEASE: f32 = 0.22;
/// Below this an eased value reads as "arrived" for settle detection.
const SETTLE_EPSILON: f32 = 0.002;
/// Fixed physics step: the tick loop always advances by this much, never by
/// a wall-clock delta.
const DT: f32 = 1.0 / 60.0;
/// Resting band profile used before any cover-derived static profile is set.
/// Zero (not a faint idle shimmer): the water surface is driven by the
/// current bands every tick, so anything above [`super::water::WaterGrid`]'s
/// rest epsilon would keep it perpetually "live" and the engine would never
/// settle once playback stops.
const NEUTRAL_PROFILE: [f32; SPECTRUM_BAND_COUNT] = [0.0; SPECTRUM_BAND_COUNT];
/// `mid` folds this band range of the (eased) current spectrum.
const MID_RANGE: std::ops::Range<usize> = 20..44;
/// `high` folds this band range of the (eased) current spectrum.
const HIGH_RANGE: std::ops::Range<usize> = 44..64;
/// Number of `set_static_profile` dimension bytes folded per band group.
const PROFILE_GROUP: usize = SPECTRUM_BAND_COUNT / 4;
/// Secondary accent hue offset when no cover-derived accent is available.
const FALLBACK_ACCENT2_HUE_SHIFT: f32 = 42.0;

/// Which of the 8 visual treatments the engine currently renders.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VisualMode {
    Grid,
    #[default]
    Bars,
    Flow,
    Pulse,
    Particles,
    Neon,
}

impl VisualMode {
    pub const ALL: [Self; 6] = [
        Self::Grid,
        Self::Bars,
        Self::Flow,
        Self::Pulse,
        Self::Particles,
        Self::Neon,
    ];

    /// Stable, lowercase identifier: used for widget names and persisted
    /// settings, so it must never change once shipped.
    pub fn id(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::Bars => "bars",
            Self::Flow => "flow",
            Self::Pulse => "pulse",
            Self::Particles => "particles",
            Self::Neon => "neon",
        }
    }
}

/// The per-frame render inputs a mode's `scene` builder needs. Borrowed from
/// the engine for the lifetime of one [`VisualEngine::scene`] call — modes
/// never own or mutate engine state, they only read it.
pub struct ModeCtx<'a> {
    /// UI-smoothed, post-AGC display bands (`0..=1`).
    pub bands: &'a [f32; SPECTRUM_BAND_COUNT],
    /// Slow-falling peak-hold markers for `bands`.
    pub peaks: &'a [f32; SPECTRUM_BAND_COUNT],
    pub level: f32,
    pub bass: f32,
    pub mid: f32,
    pub high: f32,
    pub kick: f32,
    pub clock: f32,
    pub accent: (f32, f32, f32),
    pub accent2: (f32, f32, f32),
    pub water: &'a WaterGrid,
    pub dust: &'a [Dust; DUST_COUNT],
    pub impact: &'a ImpactState,
    pub width: f32,
    pub height: f32,
}

impl ModeCtx<'_> {
    /// Sample `bands` at a fractional position `0.0..=1.0` across the full
    /// spectrum width.
    pub fn band(&self, f: f32) -> f32 {
        let last = self.bands.len() - 1;
        let index = (f.clamp(0.0, 1.0) * last as f32).round() as usize;
        self.bands[index.min(last)]
    }

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

    pub fn hsla_fill(&self, hue: f32, sat: f32, light: f32, alpha: f32) -> Fill {
        let (r, g, b) = hsla_to_rgb(hue, sat, light);
        Fill::Solid(Rgba { r, g, b, a: alpha })
    }

    /// Design's neon gradient: hue−70 → hue+70 across `x0..x1`, ends fade to
    /// transparent so the sweep reads as a glow rather than a hard band.
    pub fn hue_sweep_fill(&self, hue: f32, alpha: f32, x0: f32, x1: f32) -> Fill {
        const SWEEP_SAT: f32 = 0.82;
        const SWEEP_LIGHT: f32 = 0.6;
        let stop = |h: f32, a: f32| {
            let (r, g, b) = hsla_to_rgb(h, SWEEP_SAT, SWEEP_LIGHT);
            Rgba { r, g, b, a }
        };
        Fill::HGradient {
            x0,
            x1,
            stops: vec![
                (0.0, stop(hue - 70.0, 0.0)),
                (0.5, stop(hue, alpha)),
                (1.0, stop(hue + 70.0, 0.0)),
            ],
        }
    }
}

fn mean_range(bands: &[f32; SPECTRUM_BAND_COUNT], range: std::ops::Range<usize>) -> f32 {
    let slice = &bands[range];
    slice.iter().sum::<f32>() / slice.len() as f32
}

/// Instant-up, slow-release envelope: jumps straight to a louder reading,
/// eases back down otherwise. Shared shape for `mid`/`high`.
fn envelope_up(current: f32, raw: f32, release: f32) -> f32 {
    if raw > current {
        raw
    } else {
        current + (raw - current) * release
    }
}

/// Owns every piece of state a visualizer needs across frames: eased spectrum
/// bands and their peak-hold markers, the `level`/`mid`/`high` envelopes, the
/// water surface, dust field, and impact overlay, plus the accent palette.
/// Frontends drive it with [`ingest`](Self::ingest)/[`tick`](Self::tick) and
/// read a [`Scene`] back with [`scene`](Self::scene) — no frontend ever
/// touches easing constants or per-mode geometry directly.
pub struct VisualEngine {
    mode: VisualMode,
    bands_current: [f32; SPECTRUM_BAND_COUNT],
    bands_target: [f32; SPECTRUM_BAND_COUNT],
    bands_peaks: [f32; SPECTRUM_BAND_COUNT],
    /// Resting band profile used whenever playback is stopped/paused —
    /// either the neutral default or a cover-derived shape from
    /// [`set_static_profile`](Self::set_static_profile).
    static_profile: [f32; SPECTRUM_BAND_COUNT],
    level_current: f32,
    level_target: f32,
    bass: f32,
    mid: f32,
    high: f32,
    playing: bool,
    clock: f32,
    water: WaterGrid,
    dust: [Dust; DUST_COUNT],
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
            mode: VisualMode::default(),
            bands_current: [0.0; SPECTRUM_BAND_COUNT],
            bands_target: [0.0; SPECTRUM_BAND_COUNT],
            bands_peaks: [0.0; SPECTRUM_BAND_COUNT],
            static_profile: NEUTRAL_PROFILE,
            level_current: 0.0,
            level_target: 0.0,
            bass: 0.0,
            mid: 0.0,
            high: 0.0,
            playing: false,
            clock: 0.0,
            water: WaterGrid::new(),
            dust: make_dust(),
            impact: ImpactState::new(),
            accent: (0.5, 0.5, 0.5),
            cover_accent2: None,
        }
    }

    pub fn set_mode(&mut self, mode: VisualMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> VisualMode {
        self.mode
    }

    /// Pauses drift (the tick loop stops feeding fresh frames in) and
    /// retargets the bands toward the resting static profile so a paused or
    /// stopped visual settles instead of freezing mid-motion.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        if !playing {
            self.bands_target = self.static_profile;
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

    /// Derives a resting band profile from 4 cover-art-adjacent percentage
    /// bytes (`0..=100`), spreading each byte across an equal quarter of the
    /// display bands. Used so the idle/paused visual reflects the track's
    /// character instead of going perfectly flat.
    pub fn set_static_profile(&mut self, dimensions: &[u8; 4]) {
        let profile: [f32; SPECTRUM_BAND_COUNT] = std::array::from_fn(|index| {
            let group = (index / PROFILE_GROUP).min(dimensions.len() - 1);
            let dimension = f32::from(dimensions[group]) / 100.0;
            (0.08 + dimension * 0.34).clamp(0.0, 1.0)
        });
        self.static_profile = profile;
        self.bands_target = profile;
    }

    pub fn clear_static_profile(&mut self) {
        self.static_profile = NEUTRAL_PROFILE;
        self.bands_target = NEUTRAL_PROFILE;
    }

    /// Track switched: reset the clock, water surface and impact overlay so
    /// leftover ripples/sparks from the previous track don't bleed in.
    pub fn note_track_changed(&mut self) {
        self.clock = 0.0;
        self.water = WaterGrid::new();
        self.impact = ImpactState::new();
    }

    /// Feed one enriched spectrum frame in: retargets the eased bands/level
    /// and fires the discrete impact/water side effects for beats and drops.
    pub fn ingest(&mut self, frame: &SpectrumFrame) {
        self.bands_target = *frame.bands();
        self.level_target = frame.level();
        self.bass = frame.bass();
        let beat = frame.beat();
        if beat.fired {
            self.impact.spawn_beat(beat.strength);
            self.water.splash(frame.level());
        }
        self.impact.spawn_drop(frame.dynamics());
    }

    /// One 60 Hz step. Returns `true` once every eased value has arrived at
    /// its target, the impact overlay and water surface are at rest, and
    /// playback is stopped — the frontend may stop ticking at that point.
    pub fn tick(&mut self) -> bool {
        let mut bands_settled = true;
        let last = (SPECTRUM_BAND_COUNT - 1) as f32;
        for (index, ((current, &target), peak)) in self
            .bands_current
            .iter_mut()
            .zip(self.bands_target.iter())
            .zip(self.bands_peaks.iter_mut())
            .enumerate()
        {
            let delta = target - *current;
            let release = BAND_RELEASE_MIN + BAND_RELEASE_SPAN * (index as f32 / last);
            let coeff = if delta > 0.0 { BAND_ATTACK } else { release };
            let next = *current + delta * coeff;
            bands_settled &= (target - next).abs() < SETTLE_EPSILON;
            *current = next;
            *peak = (peak.max(next) - PEAK_DECAY).max(next);
        }

        let level_delta = self.level_target - self.level_current;
        let level_coeff = if level_delta > 0.0 {
            SCALAR_ATTACK
        } else {
            SCALAR_RELEASE
        };
        self.level_current += level_delta * level_coeff;
        let level_settled = (self.level_target - self.level_current).abs() < SETTLE_EPSILON;

        let mid_raw = mean_range(&self.bands_current, MID_RANGE);
        self.mid = envelope_up(self.mid, mid_raw, MID_HIGH_RELEASE);
        let high_raw = mean_range(&self.bands_current, HIGH_RANGE);
        self.high = envelope_up(self.high, high_raw, MID_HIGH_RELEASE);

        self.impact.advance();
        self.water.advance(&self.bands_current);
        advance_dust(&mut self.dust, self.level_current);

        if self.playing {
            self.clock += DT;
        }

        bands_settled
            && level_settled
            && self.impact.is_idle()
            && self.water.is_still()
            && !self.playing
    }

    /// Snaps every eased value straight to rest for reduced-motion
    /// frontends: no interpolation, no lingering water/impact ornaments.
    pub fn snap_to_static(&mut self) {
        self.bands_current = self.bands_target;
        self.bands_peaks = self.bands_current;
        self.level_current = self.level_target;
        self.mid = mean_range(&self.bands_current, MID_RANGE);
        self.high = mean_range(&self.bands_current, HIGH_RANGE);
        self.water.reset();
        self.impact = ImpactState::new();
    }

    /// Secondary accent: a cover-derived distinct hue when available,
    /// otherwise the primary accent hue-shifted by a fixed offset.
    pub fn accent2(&self) -> (f32, f32, f32) {
        self.cover_accent2
            .unwrap_or_else(|| hue_shift(self.accent, FALLBACK_ACCENT2_HUE_SHIFT))
    }

    fn make_ctx(&self, width: f32, height: f32) -> ModeCtx<'_> {
        ModeCtx {
            bands: &self.bands_current,
            peaks: &self.bands_peaks,
            level: self.level_current,
            bass: self.bass,
            mid: self.mid,
            high: self.high,
            kick: self.impact.kick(),
            clock: self.clock,
            accent: self.accent,
            accent2: self.accent2(),
            water: &self.water,
            dust: &self.dust,
            impact: &self.impact,
            width,
            height,
        }
    }

    /// Builds the resolution-independent scene for this instant: an accent
    /// wash first, then the current mode's shapes, then a soft flash overlay
    /// while a drop/slam is still decaying.
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
        shapes.extend(modes::build_scene(self.mode, &ctx));
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
/// full-scale frames (fires a beat, a kick, and pins the bands high). Shared
/// by every mode's tests (Tasks 11–17) so each one can exercise a "lively"
/// scene without re-deriving this fixture.
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

/// Borrows a [`ModeCtx`] out of an engine for direct per-mode testing
/// (Tasks 11–17), without going through the full [`VisualEngine::scene`]
/// wash + flash wrapping.
#[cfg(test)]
pub(crate) fn test_ctx(engine: &VisualEngine, width: f32, height: f32) -> ModeCtx<'_> {
    engine.make_ctx(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visuals::color;

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
        let max_len = scene
            .shapes
            .iter()
            .filter_map(|s| match &s.geom {
                Geom::Polyline { points, .. } if points.len() == 2 => {
                    Some((points[0].1 - points[1].1).abs())
                }
                _ => None,
            })
            .fold(0.0_f32, f32::max);
        assert!(
            max_len > 150.0,
            "slam should nearly fill the canvas, got {max_len}"
        );
    }

    #[test]
    fn stopped_engine_settles() {
        let mut engine = lively_engine();
        engine.set_playing(false);
        engine.clear_static_profile();
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

    /// Exercises the `test_ctx` helper future mode tasks (11-17) will reuse
    /// to build a `ModeCtx` directly, without going through `scene`'s wash
    /// and flash wrapping.
    #[test]
    fn test_ctx_borrows_a_matching_mode_ctx() {
        let engine = lively_engine();
        let ctx = test_ctx(&engine, 548.0, 300.0);
        assert_eq!((ctx.width, ctx.height), (548.0, 300.0));
        assert_eq!(ctx.bands.len(), SPECTRUM_BAND_COUNT);
        assert_eq!(ctx.accent, engine.accent);
    }
}
