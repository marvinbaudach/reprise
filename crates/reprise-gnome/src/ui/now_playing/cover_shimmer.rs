//! A soft disc of the cover itself, turning behind it at a theme-aware rate.
//!
//! The mockup draws this as a conic gradient of the cover's three dominant
//! colours. Measured against this library that failed: half the covers are
//! greyscale or near-black and yield no palette at all, and the ones that do
//! are usually monochrome artwork, so the sweep came out as one flat tone over
//! a backdrop made of the same tone — invisible. The artwork itself always has
//! structure, even in black and white, so the disc is the blurred cover rather
//! than colours extracted from it. Same honesty rule as the bloom, and it works
//! on every record instead of two in five.
//! The two owner-approved arms were accepted by eye in the running app. The
//! dark arm turns every 25 seconds at 0.48 resting opacity; the quieter light
//! arm turns every 40 seconds at 0.40 against its denser bloom.
//!
//! Cost is the bloom's bargain: the masked disc is rasterized once per cover;
//! per frame there is a translate, a rotate and one `paint_with_alpha`. The
//! clock is the backdrop's — this module owns no timer.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::cairo;
use gtk4::prelude::*;

use crate::ui::cover_glow;
use crate::ui::style::tokens;

/// Approved dark resting opacity, raised from 0.34 with the turn rate.
const SHIMMER_REST_OPACITY: f64 = 0.48;
/// Owner-approved light resting opacity, accepted by eye in the running app.
/// It stays thinner against the denser light-mode bloom.
const LIGHT_SHIMMER_REST_OPACITY: f64 = 0.40;
const SHIMMER_OPACITY_PER_PRESSURE: f64 = 0.14;
const SHIMMER_OPACITY_PER_SWELL: f64 = 0.16;
/// The mockup's 520 px disc over its 168 px cover.
const SHIMMER_DIAMETER_PER_COVER: f64 = 520.0 / 168.0;
/// Centre of the disc, measured down from the top of the band.
const SHIMMER_CENTRE_Y: f64 = 100.0;
/// The disc is clipped to the same artwork band as the cover and bloom.
const SHIMMER_BAND_HEIGHT: f64 = tokens::NOW_PLAYING_ARTWORK_BAND as f64;
/// Approved dark turn period. At a minute the disc measured 0.04x the bloom.
const SHIMMER_TURN_S: f64 = 25.0;
/// Owner-approved light turn period, accepted by eye in the running app. The
/// slower turn balances the denser bloom and greater contrast of a dark disc on
/// light ground, where the same rotation reads as more salient, not faster.
const LIGHT_SHIMMER_TURN_S: f64 = 40.0;
/// `radial-gradient(circle closest-side, #000 12%, transparent 68%)`.
const SHIMMER_MASK_SOLID: f64 = 0.12;
const SHIMMER_MASK_CLEAR: f64 = 0.68;
/// Edge of the cached raster. The cover arrives as a 32 px blur and is painted
/// up to this before the mask is baked in, so the mask's falloff stays smooth
/// while the blur itself costs what it costs in `cover_glow`.
const SHIMMER_SURFACE_EDGE: i32 = 260;
/// A reading below this threshold cannot visibly change the light.
const LIGHT_EPSILON: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ShimmerModel {
    turn_s: f64,
    rest_opacity: f64,
}

/// Both arms are owner-approved from judgment in the running app. The light arm
/// is slower and thinner because its bloom rests at 0.14 instead of 0.06 and
/// reacts at 0.26 / 0.24 instead of 0.15 / 0.16. A dark blurred disc on light
/// ground also carries more contrast, making the same rotation more salient.
fn shimmer_model(is_dark: bool) -> ShimmerModel {
    if is_dark {
        ShimmerModel {
            turn_s: SHIMMER_TURN_S,
            rest_opacity: SHIMMER_REST_OPACITY,
        }
    } else {
        ShimmerModel {
            turn_s: LIGHT_SHIMMER_TURN_S,
            rest_opacity: LIGHT_SHIMMER_REST_OPACITY,
        }
    }
}

fn previous_turn_s_if_changed(previous: Option<f64>, current: f64) -> Option<f64> {
    previous.filter(|previous| *previous != current)
}

pub(super) fn shimmer_opacity(pressure: f64, swell: f64, is_dark: bool) -> f64 {
    shimmer_model(is_dark).rest_opacity
        + SHIMMER_OPACITY_PER_PRESSURE * pressure.clamp(0.0, 1.0)
        + SHIMMER_OPACITY_PER_SWELL * swell.clamp(0.0, 1.0)
}

/// Rotation at `elapsed_s`, wrapped so a long session cannot lose precision.
pub(super) fn shimmer_angle(elapsed_s: f64, is_dark: bool) -> f64 {
    std::f64::consts::TAU * (elapsed_s / shimmer_model(is_dark).turn_s).rem_euclid(1.0)
}

/// Mask alpha at `r` ∈ [0, 1] of the disc's radius.
pub(super) fn shimmer_mask(r: f64) -> f64 {
    if r <= SHIMMER_MASK_SOLID {
        return 1.0;
    }
    if r >= SHIMMER_MASK_CLEAR {
        return 0.0;
    }
    (SHIMMER_MASK_CLEAR - r) / (SHIMMER_MASK_CLEAR - SHIMMER_MASK_SOLID)
}

#[derive(Clone, Copy, Default)]
struct Phase {
    started_at_us: i64,
    phase_us: i64,
    elapsed_us: i64,
}

impl Phase {
    /// Returns whether `elapsed_us` changed, meaning a redraw is due.
    fn advance(&mut self, frame_time_us: i64) -> bool {
        if self.started_at_us == 0 {
            self.started_at_us = frame_time_us;
        }
        let elapsed_us = self
            .phase_us
            .saturating_add(frame_time_us.saturating_sub(self.started_at_us));
        if self.elapsed_us == elapsed_us {
            return false;
        }
        self.elapsed_us = elapsed_us;
        true
    }

    /// Folds the running segment, returning whether anything was folded.
    fn hold(&mut self) -> bool {
        if self.started_at_us == 0 {
            return false;
        }
        self.started_at_us = 0;
        self.phase_us = self.elapsed_us;
        true
    }

    /// Re-anchors the running segment at the same angle for a new turn rate.
    fn retime(&mut self, old_turn_s: f64, new_turn_s: f64, frame_time_us: i64) {
        let elapsed_us = if self.started_at_us == 0 {
            self.elapsed_us
        } else {
            self.phase_us
                .saturating_add(frame_time_us.saturating_sub(self.started_at_us))
        };
        let elapsed_s = elapsed_us as f64 / 1_000_000.0;
        let new_elapsed_s = (elapsed_s / old_turn_s).rem_euclid(1.0) * new_turn_s;
        let new_elapsed_us = (new_elapsed_s * 1_000_000.0).round() as i64;

        self.started_at_us = frame_time_us;
        self.phase_us = new_elapsed_us;
        self.elapsed_us = new_elapsed_us;
    }

    fn elapsed_s(self) -> f64 {
        self.elapsed_us as f64 / 1_000_000.0
    }
}

struct Inner {
    surface: RefCell<Option<cairo::ImageSurface>>,
    /// Cover generation the cached disc was built from; the panel bumps it once
    /// per rendered track, exactly as `cover_bloom` keys its own cache.
    generation: Cell<Option<u64>>,
    pressure: Cell<f64>,
    swell: Cell<f64>,
    phase: Cell<Phase>,
    /// `None` until the first tick observes the actual theme, so only an
    /// observed rate transition can retime the phase.
    last_turn_s: Cell<Option<f64>>,
    pinned: Cell<bool>,
}

#[derive(Clone)]
pub(super) struct CoverShimmer {
    area: gtk4::DrawingArea,
    inner: Rc<Inner>,
}

impl CoverShimmer {
    pub(super) fn new() -> Self {
        let area = gtk4::DrawingArea::new();
        area.add_css_class("reprise-now-playing-shimmer");
        area.set_can_target(false);
        area.set_can_focus(false);
        area.set_visible(false);
        let inner = Rc::new(Inner {
            surface: RefCell::new(None),
            generation: Cell::new(None),
            pressure: Cell::new(0.0),
            swell: Cell::new(0.0),
            phase: Cell::new(Phase::default()),
            last_turn_s: Cell::new(None),
            pinned: Cell::new(true),
        });
        area.set_draw_func({
            let inner = inner.clone();
            move |_, cr, width, height| draw(cr, width, height, &inner)
        });
        Self { area, inner }
    }

    pub(super) fn widget(&self) -> &gtk4::DrawingArea {
        &self.area
    }

    #[cfg(test)]
    pub(super) fn drawn_angle_for_test(&self) -> Option<f64> {
        self.inner.surface.borrow().as_ref()?;
        let elapsed_s = self.inner.phase.get().elapsed_s();
        let is_dark = libadwaita::StyleManager::default().is_dark();
        Some(shimmer_angle(elapsed_s, is_dark))
    }

    /// The cover the disc is cut from, or `None` for external media, a
    /// placeholder, or no track. Without artwork the disc stays dark: a light
    /// whose colour is not in the record is the dishonesty this whole layer
    /// exists to avoid.
    pub(super) fn set_cover(&self, texture: Option<&gtk4::gdk::Texture>, generation: u64) {
        match texture {
            Some(texture) => {
                if self.inner.generation.get() == Some(generation) {
                    return;
                }
                *self.inner.surface.borrow_mut() = build_surface(texture);
                self.inner.generation.set(Some(generation));
            }
            None => {
                *self.inner.surface.borrow_mut() = None;
                self.inner.generation.set(None);
            }
        }
        self.area.queue_draw();
    }

    pub(super) fn set_light(&self, pressure: f64, swell: f64) {
        if self.inner.pinned.get() {
            return;
        }
        let pressure = pressure.clamp(0.0, 1.0);
        let swell = swell.clamp(0.0, 1.0);
        if (self.inner.pressure.get() - pressure).abs() < LIGHT_EPSILON
            && (self.inner.swell.get() - swell).abs() < LIGHT_EPSILON
        {
            return;
        }
        self.inner.pressure.set(pressure);
        self.inner.swell.set(swell);
        self.area.queue_draw();
    }

    pub(super) fn set_frame_time(&self, frame_time_us: i64) {
        if self.inner.pinned.get() {
            return;
        }
        if frame_time_us <= 0 {
            if self.hold_phase() {
                self.area.queue_draw();
            }
            return;
        }
        let is_dark = libadwaita::StyleManager::default().is_dark();
        let turn_s = shimmer_model(is_dark).turn_s;
        let mut phase = self.inner.phase.get();
        let old_turn_s = self.inner.last_turn_s.replace(Some(turn_s));
        let retimed = previous_turn_s_if_changed(old_turn_s, turn_s);
        if let Some(old_turn_s) = retimed {
            phase.retime(old_turn_s, turn_s, frame_time_us);
        }
        if !crate::ui::motion::animations_enabled() {
            let held = phase.hold();
            if retimed.is_some() || held {
                self.inner.phase.set(phase);
                self.area.queue_draw();
            }
            return;
        }
        let changed = if retimed.is_some() {
            true
        } else {
            phase.advance(frame_time_us)
        };
        self.inner.phase.set(phase);
        if changed {
            self.area.queue_draw();
        }
    }

    fn hold_phase(&self) -> bool {
        let mut phase = self.inner.phase.get();
        let held = phase.hold();
        if held {
            self.inner.phase.set(phase);
        }
        held
    }

    pub(super) fn set_pinned(&self, pinned: bool) {
        self.inner.pinned.set(pinned);
        self.area.set_visible(!pinned);
        if pinned {
            self.inner.pressure.set(0.0);
            self.inner.swell.set(0.0);
            self.hold_phase();
        }
        self.area.queue_draw();
    }
}

fn build_surface(texture: &gtk4::gdk::Texture) -> Option<cairo::ImageSurface> {
    let blurred = cover_glow::blurred_surface(texture)?;
    let surface = cairo::ImageSurface::create(
        cairo::Format::ARgb32,
        SHIMMER_SURFACE_EDGE,
        SHIMMER_SURFACE_EDGE,
    )
    .ok()?;
    let cr = cairo::Context::new(&surface).ok()?;
    let centre = f64::from(SHIMMER_SURFACE_EDGE) / 2.0;

    // The 32 px blur painted across the whole disc: bilinear over an 8x upscale
    // is what makes it a blur at all, exactly as in `cover_bloom`.
    let scale = f64::from(SHIMMER_SURFACE_EDGE) / f64::from(cover_glow::BLUR_EDGE);
    cr.save().ok();
    cr.scale(scale, scale);
    if cr.set_source_surface(&blurred, 0.0, 0.0).is_ok() {
        cr.source().set_filter(cairo::Filter::Bilinear);
        cr.source().set_extend(cairo::Extend::Pad);
        cr.paint().ok();
    }
    cr.restore().ok();

    let mask = cairo::RadialGradient::new(centre, centre, 0.0, centre, centre, centre);
    mask.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, shimmer_mask(0.0));
    mask.add_color_stop_rgba(SHIMMER_MASK_SOLID, 0.0, 0.0, 0.0, 1.0);
    mask.add_color_stop_rgba(SHIMMER_MASK_CLEAR, 0.0, 0.0, 0.0, 0.0);
    mask.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
    cr.set_operator(cairo::Operator::DestIn);
    cr.set_source(&mask).ok();
    cr.paint().ok();
    Some(surface)
}

fn draw(cr: &cairo::Context, width: i32, height: i32, inner: &Inner) {
    let surface = inner.surface.borrow();
    let Some(surface) = surface.as_ref() else {
        return;
    };
    let diameter = SHIMMER_DIAMETER_PER_COVER * f64::from(tokens::NOW_PLAYING_COVER_SIZE);
    let scale = diameter / f64::from(SHIMMER_SURFACE_EDGE);
    let elapsed_s = inner.phase.get().elapsed_s();
    let is_dark = libadwaita::StyleManager::default().is_dark();
    cr.save().ok();
    cr.rectangle(
        0.0,
        0.0,
        f64::from(width),
        SHIMMER_BAND_HEIGHT.min(f64::from(height)),
    );
    cr.clip();
    cr.translate(f64::from(width) / 2.0, SHIMMER_CENTRE_Y);
    cr.rotate(shimmer_angle(elapsed_s, is_dark));
    cr.scale(scale, scale);
    let centre = f64::from(SHIMMER_SURFACE_EDGE) / 2.0;
    if cr.set_source_surface(surface, -centre, -centre).is_ok() {
        cr.source().set_filter(cairo::Filter::Bilinear);
        cr.paint_with_alpha(shimmer_opacity(
            inner.pressure.get(),
            inner.swell.get(),
            is_dark,
        ))
        .ok();
    }
    cr.restore().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_24_light_shimmer_is_quieter_while_dark_keeps_its_approved_opacity_model() {
        let dark = shimmer_model(true);
        let light = shimmer_model(false);

        assert_eq!(dark.rest_opacity, 0.48);
        assert_eq!(light.rest_opacity, 0.40);
        assert!((shimmer_opacity(1.0, 0.0, true) - 0.62).abs() < 1e-9);
        assert!((shimmer_opacity(1.0, 1.0, true) - 0.78).abs() < 1e-9);
        assert!((shimmer_opacity(-1.0, 4.0, true) - 0.64).abs() < 1e-9);
        assert!((shimmer_opacity(1.0, 0.0, false) - 0.54).abs() < 1e-9);
        assert!((shimmer_opacity(1.0, 1.0, false) - 0.70).abs() < 1e-9);
        assert!((shimmer_opacity(-1.0, 4.0, false) - 0.56).abs() < 1e-9);
        assert!(light.rest_opacity < dark.rest_opacity);
    }

    #[test]
    fn ac_24_the_shimmer_turn_rate_is_theme_aware() {
        let dark = shimmer_model(true);
        let light = shimmer_model(false);

        assert_eq!(dark.turn_s, 25.0);
        assert_eq!(light.turn_s, 40.0);
        assert!((shimmer_angle(0.0, true) - 0.0).abs() < 1e-9);
        assert!((shimmer_angle(6.25, true) - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!((shimmer_angle(12.5, true) - std::f64::consts::PI).abs() < 1e-9);
        assert!((shimmer_angle(25.0, true) - shimmer_angle(0.0, true)).abs() < 1e-9);
        assert!((shimmer_angle(26.0, true) - shimmer_angle(1.0, true)).abs() < 1e-9);
        assert!((shimmer_angle(86_400.0, true) - shimmer_angle(0.0, true)).abs() < 1e-6);
        assert!((shimmer_angle(10.0, false) - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!((shimmer_angle(20.0, false) - std::f64::consts::PI).abs() < 1e-9);
        assert!((shimmer_angle(40.0, false) - shimmer_angle(0.0, false)).abs() < 1e-9);
    }

    #[test]
    fn npp_18_the_disc_keeps_its_phase_across_a_hold_and_resume() {
        let mut phase = Phase::default();
        phase.advance(1_000_000);
        phase.advance(11_000_000);
        let before = phase.elapsed_s();
        assert!(before > 0.0, "the disc did not start turning");
        assert!(phase.hold());
        phase.advance(12_000_000);
        phase.advance(14_000_000);
        let after = phase.elapsed_s();
        assert!(
            (after - (before + 2.0)).abs() < 1e-6,
            "the disc did not resume from its held phase"
        );
    }

    #[test]
    fn npp_18_a_double_hold_does_not_fold_the_phase_twice() {
        let mut phase = Phase::default();
        phase.advance(1_000_000);
        phase.advance(11_000_000);

        assert!(phase.hold());
        let held = phase.elapsed_s();
        assert!(!phase.hold());
        assert!((phase.elapsed_s() - held).abs() < 1e-9);
    }

    #[test]
    fn npp_18_resuming_after_a_huge_gap_does_not_jump() {
        let mut phase = Phase::default();
        phase.advance(1_000_000);
        phase.advance(11_000_000);
        phase.hold();
        let before = phase.elapsed_s();

        assert!(!phase.advance(500_000_000));
        assert!((phase.elapsed_s() - before).abs() < 1e-9);
    }

    #[test]
    fn npp_18_advance_reports_no_change_when_elapsed_does_not_move() {
        let mut phase = Phase::default();

        assert!(!phase.advance(1_000_000));
        assert!(!phase.advance(1_000_000));
        assert_eq!(phase.elapsed_s(), 0.0);
    }

    #[test]
    fn npp_18_retiming_keeps_the_disc_angle_continuous() {
        let mut phase = Phase::default();
        phase.advance(1_000_000);
        phase.advance(11_000_000);
        let before = shimmer_angle(12.0, true);

        phase.retime(25.0, 40.0, 13_000_000);
        let after = shimmer_angle(phase.elapsed_s(), false);

        assert!((after - before).abs() < 1e-9);
    }

    #[test]
    fn npp_18_only_an_observed_turn_rate_change_requests_retiming() {
        assert_eq!(previous_turn_s_if_changed(None, 40.0), None);
        assert_eq!(previous_turn_s_if_changed(Some(40.0), 40.0), None);
        assert_eq!(previous_turn_s_if_changed(Some(25.0), 40.0), Some(25.0));
    }

    #[test]
    fn ac_24_the_shimmer_is_cut_from_the_artwork_not_from_extracted_colours() {
        // The mockup sweeps three dominant cover colours. Measured against a
        // real library that fails: half the covers are greyscale or near-black
        // and yield no palette at all (chroma below the 0.03 gate), and the
        // ones that do are usually monochrome artwork, so the sweep came out
        // as one flat tone lying on a backdrop of the same tone. The blurred
        // cover always has structure, so that is what turns.
        // Assert on structure, not on words: the doc comment above has to be
        // free to explain what a conic gradient was and why it lost. The
        // needles are split because `include_str!` reads this test too — a
        // literal naming the forbidden symbol would always find itself.
        let source = include_str!("cover_shimmer.rs");
        assert!(source.contains("cover_glow::blurred_surface"));
        let conic_stops = ["fn shimmer", "_stop"].concat();
        assert!(!source.contains(&conic_stops));
        let palette_module = ["cover", "_palette"].concat();
        assert!(!source.contains(&palette_module));
    }

    #[test]
    fn ac_24_the_shimmer_mask_is_solid_inside_and_gone_by_two_thirds() {
        // radial-gradient(circle closest-side, #000 12%, transparent 68%)
        assert!((shimmer_mask(0.0) - 1.0).abs() < 1e-9);
        assert!((shimmer_mask(0.12) - 1.0).abs() < 1e-9);
        assert!((shimmer_mask(0.40) - 0.5).abs() < 0.02);
        assert!((shimmer_mask(0.68) - 0.0).abs() < 1e-9);
        assert!((shimmer_mask(1.0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn ac_24_the_shimmer_disc_is_three_covers_wide() {
        // 520 px against the mockup's 168 px cover.
        assert!((SHIMMER_DIAMETER_PER_COVER - 520.0 / 168.0).abs() < 1e-9);
    }

    #[test]
    fn npp_18_shimmer_mask_is_clear_before_the_artwork_band_ends() {
        let radius = SHIMMER_DIAMETER_PER_COVER * f64::from(tokens::NOW_PLAYING_COVER_SIZE) / 2.0;
        let clear_edge = SHIMMER_CENTRE_Y + SHIMMER_MASK_CLEAR * radius;

        assert_eq!(shimmer_mask(SHIMMER_MASK_CLEAR), 0.0);
        assert!(
            clear_edge <= SHIMMER_BAND_HEIGHT,
            "the shimmer clears at y={clear_edge:.1}, after the {SHIMMER_BAND_HEIGHT:.1}px band"
        );
    }
}
