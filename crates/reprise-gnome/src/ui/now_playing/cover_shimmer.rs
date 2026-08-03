//! A slow conic sweep of the cover's three dominant colours, turning behind it
//! once a minute.
//!
//! The colours come from the artwork (`style::cover_palette`), so the light is
//! the record's own — the same honesty rule the bloom follows. Cairo has no
//! conic gradient, so the disc is rasterized once per palette as flat wedges
//! with the radial mask baked in; per frame there is a translate, a rotate and
//! one `paint_with_alpha`. The clock is the backdrop's — this module owns no
//! timer.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::cairo;
use gtk4::prelude::*;

use crate::ui::style::cover_palette::Palette;
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
/// Edge of the cached raster. Painted up ×2; the disc is a smooth gradient and
/// bilinear costs nothing visible while quartering the rasterization.
const SHIMMER_SURFACE_EDGE: i32 = 260;
/// Wedges in the cached conic. At 260 px this is a 1.4° step — below the
/// resampling filter's own footprint, so no banding survives the upscale.
const SHIMMER_WEDGES: i32 = 256;
/// Adjacent wedges overlap by one hundredth of their angle, avoiding Cairo's
/// antialiasing hairlines without changing the visible gradient.
const SHIMMER_WEDGE_OVERLAP: f64 = 0.01;
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

/// The conic gradient's colour and alpha at `t` ∈ [0, 1] around the disc.
/// Five stops: l1 .52, l2 .40, l3 .30, l2 .42, l1 .52 — the last equals the
/// first, because a conic gradient closes on itself.
pub(super) fn shimmer_stop(palette: Palette, t: f64) -> (f64, f64, f64, f64) {
    let colors = [
        palette.primary,
        palette.second,
        palette.third,
        palette.second,
        palette.primary,
    ];
    let alphas = [0.52, 0.40, 0.30, 0.42, 0.52];
    let scaled = t.clamp(0.0, 1.0) * 4.0;
    let index = (scaled.floor() as usize).min(3);
    let amount = scaled - index as f64;
    let start = colors[index];
    let end = colors[index + 1];
    let channel = |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * amount) / 255.0;
    (
        channel(start.r, end.r),
        channel(start.g, end.g),
        channel(start.b, end.b),
        alphas[index] + (alphas[index + 1] - alphas[index]) * amount,
    )
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
    palette: Cell<Option<Palette>>,
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
            palette: Cell::new(None),
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

    pub(super) fn set_palette(&self, palette: Option<Palette>) {
        if self.inner.palette.get() == palette {
            return;
        }
        self.inner.palette.set(palette);
        *self.inner.surface.borrow_mut() = None;
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

fn build_surface(palette: Palette) -> Option<cairo::ImageSurface> {
    let surface = cairo::ImageSurface::create(
        cairo::Format::ARgb32,
        SHIMMER_SURFACE_EDGE,
        SHIMMER_SURFACE_EDGE,
    )
    .ok()?;
    let cr = cairo::Context::new(&surface).ok()?;
    let centre = f64::from(SHIMMER_SURFACE_EDGE) / 2.0;
    let step = std::f64::consts::TAU / f64::from(SHIMMER_WEDGES);
    let overlap = step * SHIMMER_WEDGE_OVERLAP;
    for index in 0..SHIMMER_WEDGES {
        let t = f64::from(index) / f64::from(SHIMMER_WEDGES);
        let (r, g, b, a) = shimmer_stop(palette, t);
        cr.set_source_rgba(r, g, b, a);
        cr.move_to(centre, centre);
        cr.arc(
            centre,
            centre,
            centre,
            t * std::f64::consts::TAU,
            (t + 1.0 / f64::from(SHIMMER_WEDGES)) * std::f64::consts::TAU + overlap,
        );
        cr.close_path();
        cr.fill().ok();
    }

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
    let Some(palette) = inner.palette.get() else {
        return;
    };
    let needs_surface = inner.surface.borrow().is_none();
    if needs_surface {
        let built = build_surface(palette);
        *inner.surface.borrow_mut() = built;
    }
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
    use crate::ui::style::cover_accent::Rgb;

    fn palette() -> Palette {
        Palette {
            primary: Rgb {
                r: 145,
                g: 132,
                b: 217,
            },
            second: Rgb {
                r: 120,
                g: 140,
                b: 210,
            },
            third: Rgb {
                r: 170,
                g: 125,
                b: 190,
            },
        }
    }

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
    fn ac_24_the_shimmer_sweeps_the_palette_and_closes_on_itself() {
        let palette = palette();
        // The five stops of the mockup's conic gradient.
        let (r0, g0, b0, a0) = shimmer_stop(palette, 0.0);
        assert!((a0 - 0.52).abs() < 1e-9);
        assert!((shimmer_stop(palette, 0.25).3 - 0.40).abs() < 1e-9);
        assert!((shimmer_stop(palette, 0.50).3 - 0.30).abs() < 1e-9);
        assert!((shimmer_stop(palette, 0.75).3 - 0.42).abs() < 1e-9);
        // A conic gradient wraps: the last stop must equal the first, or the
        // disc shows a seam that rotates once a minute.
        let (r1, g1, b1, a1) = shimmer_stop(palette, 1.0);
        assert!((r0 - r1).abs() < 1e-9 && (g0 - g1).abs() < 1e-9);
        assert!((b0 - b1).abs() < 1e-9 && (a0 - a1).abs() < 1e-9);
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
