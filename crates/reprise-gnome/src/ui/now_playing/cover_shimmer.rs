//! A soft disc of the cover itself, turning behind it once a minute.
//!
//! The mockup draws this as a conic gradient of the cover's three dominant
//! colours. Measured against this library that failed: half the covers are
//! greyscale or near-black and yield no palette at all, and the ones that do
//! are usually monochrome artwork, so the sweep came out as one flat tone over
//! a backdrop made of the same tone — invisible. The artwork itself always has
//! structure, even in black and white, so the disc is the blurred cover rather
//! than colours extracted from it. Same honesty rule as the bloom, and it works
//! on every record instead of two in five.
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

const SHIMMER_REST_OPACITY: f64 = 0.34;
const SHIMMER_OPACITY_PER_PRESSURE: f64 = 0.14;
const SHIMMER_OPACITY_PER_SWELL: f64 = 0.16;
/// The mockup's 520 px disc over its 168 px cover.
const SHIMMER_DIAMETER_PER_COVER: f64 = 520.0 / 168.0;
/// Centre of the disc, measured down from the top of the band.
const SHIMMER_CENTRE_Y: f64 = 100.0;
/// The mockup clips the disc to this band from the top of the panel head.
const SHIMMER_BAND_HEIGHT: f64 = 340.0;
/// One turn a minute.
const SHIMMER_TURN_S: f64 = 60.0;
/// `radial-gradient(circle closest-side, #000 12%, transparent 68%)`.
const SHIMMER_MASK_SOLID: f64 = 0.12;
const SHIMMER_MASK_CLEAR: f64 = 0.68;
/// Edge of the cached raster. The cover arrives as a 32 px blur and is painted
/// up to this before the mask is baked in, so the mask's falloff stays smooth
/// while the blur itself costs what it costs in `cover_glow`.
const SHIMMER_SURFACE_EDGE: i32 = 260;
/// A reading below this threshold cannot visibly change the light.
const LIGHT_EPSILON: f64 = 0.01;

pub(super) fn shimmer_opacity(pressure: f64, swell: f64) -> f64 {
    SHIMMER_REST_OPACITY
        + SHIMMER_OPACITY_PER_PRESSURE * pressure.clamp(0.0, 1.0)
        + SHIMMER_OPACITY_PER_SWELL * swell.clamp(0.0, 1.0)
}

/// Rotation at `elapsed_s`, wrapped so a long session cannot lose precision.
pub(super) fn shimmer_angle(elapsed_s: f64) -> f64 {
    std::f64::consts::TAU * (elapsed_s / SHIMMER_TURN_S).rem_euclid(1.0)
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

struct Inner {
    surface: RefCell<Option<cairo::ImageSurface>>,
    /// Cover generation the cached disc was built from; the panel bumps it once
    /// per rendered track, exactly as `cover_bloom` keys its own cache.
    generation: Cell<Option<u64>>,
    pressure: Cell<f64>,
    swell: Cell<f64>,
    started_at_us: Cell<i64>,
    frame_time_us: Cell<i64>,
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
            started_at_us: Cell::new(0),
            frame_time_us: Cell::new(0),
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
        if frame_time_us <= 0 || !crate::ui::motion::animations_enabled() {
            self.inner.started_at_us.set(0);
            if self.inner.frame_time_us.replace(0) != 0 {
                self.area.queue_draw();
            }
            return;
        }
        let started_at_us = self.inner.started_at_us.get();
        let started_at_us = if started_at_us == 0 {
            self.inner.started_at_us.set(frame_time_us);
            frame_time_us
        } else {
            started_at_us
        };
        let elapsed_us = frame_time_us.saturating_sub(started_at_us);
        if self.inner.frame_time_us.replace(elapsed_us) != elapsed_us {
            self.area.queue_draw();
        }
    }

    pub(super) fn set_pinned(&self, pinned: bool) {
        self.inner.pinned.set(pinned);
        self.area.set_visible(!pinned);
        if pinned {
            self.inner.pressure.set(0.0);
            self.inner.swell.set(0.0);
            self.inner.started_at_us.set(0);
            self.inner.frame_time_us.set(0);
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
    let elapsed_s = inner.frame_time_us.get() as f64 / 1_000_000.0;
    cr.save().ok();
    cr.rectangle(
        0.0,
        0.0,
        f64::from(width),
        SHIMMER_BAND_HEIGHT.min(f64::from(height)),
    );
    cr.clip();
    cr.translate(f64::from(width) / 2.0, SHIMMER_CENTRE_Y);
    cr.rotate(shimmer_angle(elapsed_s));
    cr.scale(scale, scale);
    let centre = f64::from(SHIMMER_SURFACE_EDGE) / 2.0;
    if cr.set_source_surface(surface, -centre, -centre).is_ok() {
        cr.source().set_filter(cairo::Filter::Bilinear);
        cr.paint_with_alpha(shimmer_opacity(inner.pressure.get(), inner.swell.get()))
            .ok();
    }
    cr.restore().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_24_the_shimmer_opacity_matches_the_backdrop_it_lies_on() {
        // Straight from the mockup: 0.34 + 0.14·pres + 0.16·sw.
        assert!((shimmer_opacity(0.0, 0.0) - 0.34).abs() < 1e-9);
        assert!((shimmer_opacity(1.0, 0.0) - 0.48).abs() < 1e-9);
        assert!((shimmer_opacity(1.0, 1.0) - 0.64).abs() < 1e-9);
        assert!((shimmer_opacity(-1.0, 4.0) - 0.50).abs() < 1e-9);
    }

    #[test]
    fn ac_24_the_shimmer_turns_once_a_minute() {
        // "eine Umdrehung pro Minute" — and it must not jump at the wrap.
        assert!((shimmer_angle(0.0) - 0.0).abs() < 1e-9);
        assert!((shimmer_angle(15.0) - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!((shimmer_angle(30.0) - std::f64::consts::PI).abs() < 1e-9);
        assert!((shimmer_angle(60.0) - shimmer_angle(0.0)).abs() < 1e-9);
        assert!((shimmer_angle(61.0) - shimmer_angle(1.0)).abs() < 1e-9);
        // A long session must not lose precision into a stutter.
        assert!((shimmer_angle(86_400.0) - shimmer_angle(0.0)).abs() < 1e-6);
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
}
