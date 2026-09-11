//! Two soft clouds drifting behind the cover, cut from the cover itself.
//!
//! The mockup draws this as two layers of radial gradients in the cover's three
//! dominant colours. Measured against this library that failed once already,
//! and the module this one replaces carried the finding: half the covers are
//! greyscale or near-black and yield no palette at all, and the ones that do
//! are usually monochrome artwork, so the colour came out as one flat tone over
//! a backdrop made of the same tone — invisible. The artwork itself always has
//! structure, even in black and white. So the mockup's gradient stops survive
//! as *masks* — where a cloud sits, how wide it is, how strong it is — while
//! what shines through them is the blurred cover. Same honesty rule as the
//! bloom, and it works on every record instead of two in five.
//!
//! Cost is the bloom's bargain: both masked fields are rasterized once per
//! cover; per frame there is a translate, a scale, a rotate and one
//! `paint_with_alpha` each, then the scrim. Nothing is re-rasterized when the
//! clock moves.
//!
//! The clock is the only thing that moves the clouds. No spectrum reading
//! reaches them — the bloom next door is where the music is allowed in.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::cairo;
use gtk4::prelude::*;

use crate::ui::cover_glow;
use crate::ui::style::tokens;

/// The field, and how far it hangs off each edge, as multiples of the cover.
///
/// The mockup is drawn against a 240 px cover; this panel's is 168. Carrying
/// the numbers as ratios rather than pixels is what lets the panel keep its own
/// size — the same way the module this replaces carried its disc as `520/168`.
/// The overhang is widest on the right: the weight leans outward, away from the
/// track list.
const FIELD_HEIGHT_PER_COVER: f64 = 440.0 / 240.0;
const OVERHANG_TOP_PER_COVER: f64 = 60.0 / 240.0;
const OVERHANG_LEFT_PER_COVER: f64 = 40.0 / 240.0;
const OVERHANG_RIGHT_PER_COVER: f64 = 90.0 / 240.0;

/// One turn of the drift, front and back. Neither may fall under 16 s.
const BACK_PERIOD_S: f64 = 16.0;
const FRONT_PERIOD_S: f64 = 20.0;
/// Half of the front layer's period, which is what sets the two against each
/// other: at rest one lies at the start of the path and the other at its end.
const FRONT_OFFSET_S: f64 = 10.0;

/// The path both layers walk, from one end to the other.
const DRIFT_X: (f64, f64) = (-0.10, 0.08);
const DRIFT_Y: (f64, f64) = (-0.06, 0.06);
const DRIFT_SCALE: (f64, f64) = (1.30, 1.45);
const DRIFT_ROTATION_DEG: (f64, f64) = (0.0, 6.0);

/// The house blur: the cover arrives as a 32 px raster and painting it across
/// the field is what blurs it — there is no blur node anywhere in this path.
/// The mockup's 48 px and 54 px survive as the *ratio* between the two layers,
/// not as absolutes: the front layer is painted from a proportionally smaller
/// raster, so it stays the softer of the two.
const BACK_BLUR_EDGE: i32 = cover_glow::BLUR_EDGE;
const FRONT_BLUR_EDGE: i32 = 28; // 32 × 48/54, rounded

/// How long a change of track takes to arrive in the light.
///
/// The clouds do not cut to the new cover: the outgoing pair fades out under
/// the incoming one over a second, so a track change reads as the colour
/// turning rather than as a jump.
const COVER_FADE_S: f64 = 1.0;

/// Edge of a cached field raster. The masks are baked into it at this size, so
/// their falloff stays smooth however far the drift stretches it.
const FIELD_RASTER_EDGE: i32 = 320;

/// The vertical fade that hands the panel back to the text.
///
/// Fully opaque from 55 % of the field down, so the title, the artist, the
/// lyrics and the segment control all sit on quiet ground. Dark and light run
/// the same numbers and differ only in the colour, which is what turns the
/// glow into a wash of colour on a light panel without a second set of values.
const SCRIM_MID_Y: f64 = 0.40;
const SCRIM_MID_ALPHA: f64 = 0.15;
const SCRIM_FULL_Y: f64 = 0.55;

/// One gradient stop of the mockup: where a cloud sits in the field, how far it
/// reaches, and how much of the cover it lets through at its centre.
#[derive(Clone, Copy)]
pub(super) struct Blob {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) alpha: f64,
    pub(super) radius: f64,
}

pub(super) const BACK_BLOBS: [Blob; 2] = [
    Blob {
        x: 0.40,
        y: 0.35,
        alpha: 0.60,
        radius: 0.50,
    },
    Blob {
        x: 0.82,
        y: 0.55,
        alpha: 0.55,
        radius: 0.50,
    },
];

pub(super) const FRONT_BLOBS: [Blob; 2] = [
    Blob {
        x: 0.75,
        y: 0.25,
        alpha: 0.45,
        radius: 0.45,
    },
    Blob {
        x: 0.30,
        y: 0.80,
        alpha: 0.40,
        radius: 0.40,
    },
];

/// Where a layer has drifted to, how far it is stretched, how far it is turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Drift {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) scale: f64,
    pub(super) rotation_deg: f64,
}

/// How far along the path a layer is at `elapsed_s`, from 0 to 1 and back.
///
/// A period is the whole round trip, not one leg of it: 16 s means the layer
/// leaves, arrives and returns inside 16 s. The mockup's `ease-in-out` on each
/// leg is taken as a smoothstep, which parts from `cubic-bezier(.42,0,.58,1)`
/// by well under a pixel of travel across a field this size.
///
/// The mockup also asks the front layer to run `reverse`. On a keyframe list
/// whose first and last poses are the same, playing it backwards yields the
/// identical sequence — CSS included — so there is nothing here to reverse.
/// What actually holds the layers apart is the offset, and it is a real half
/// period: see [`FRONT_OFFSET_S`].
pub(super) fn drift_progress(elapsed_s: f64, period_s: f64, offset_s: f64) -> f64 {
    if period_s <= 0.0 || period_s.is_nan() {
        return 0.0;
    }
    // Wrapped before it is scaled, so a session running for days cannot lose
    // the fraction into a stutter.
    let turn = ((elapsed_s + offset_s) / period_s).rem_euclid(1.0);
    let leg = if turn < 0.5 {
        turn * 2.0
    } else {
        (1.0 - turn) * 2.0
    };
    leg * leg * (3.0 - 2.0 * leg)
}

/// The pose at `elapsed_s`, interpolated along the path.
pub(super) fn drift_at(elapsed_s: f64, period_s: f64, offset_s: f64) -> Drift {
    let p = drift_progress(elapsed_s, period_s, offset_s);
    Drift {
        x: lerp(DRIFT_X, p),
        y: lerp(DRIFT_Y, p),
        scale: lerp(DRIFT_SCALE, p),
        rotation_deg: lerp(DRIFT_ROTATION_DEG, p),
    }
}

fn lerp((from, to): (f64, f64), p: f64) -> f64 {
    from + (to - from) * p
}

/// Scrim opacity at `y` ∈ [0, 1] of the field.
pub(super) fn scrim_alpha(y: f64) -> f64 {
    if y <= 0.0 {
        return 0.0;
    }
    if y >= SCRIM_FULL_Y {
        return 1.0;
    }
    if y <= SCRIM_MID_Y {
        return SCRIM_MID_ALPHA * (y / SCRIM_MID_Y);
    }
    let across = (y - SCRIM_MID_Y) / (SCRIM_FULL_Y - SCRIM_MID_Y);
    SCRIM_MID_ALPHA + (1.0 - SCRIM_MID_ALPHA) * across
}

/// How far the incoming cover has arrived, `since_s` after the change.
///
/// Linear on purpose. The two rasters are painted one over the other, so an
/// eased pair would both be part-way out at the midpoint and the light would
/// dip there — a crossfade wants its halves to sum, not to ease.
pub(super) fn cover_fade(since_s: f64) -> f64 {
    if since_s <= 0.0 || since_s.is_nan() {
        return 0.0;
    }
    (since_s / COVER_FADE_S).clamp(0.0, 1.0)
}

/// What a change of cover does to the pair that is still on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FadeStep {
    /// No clock to carry a fade: take the change at once.
    Cut,
    /// The pair on screen steps aside to fade out under the new one.
    Handover,
    /// The pair already fading stays, and the fade begins again from here.
    Restart,
    /// Nothing on screen and nothing arriving.
    Idle,
}

/// Decides that step, away from the widget so it can be checked without one.
///
/// A track change arrives in two calls: the panel clears the cover the moment
/// the track changes, and the decoded texture follows once the loader has it.
/// The clear is what puts the outgoing pair up to fade — so the second call
/// must not hand that pair on as *its* own outgoing one. Doing exactly that
/// threw the fade away and left a hard cut, which is why this decision is a
/// function of its own rather than three conditions inside a setter.
pub(super) fn fade_step(running: bool, incoming: bool, outgoing: bool, fading: bool) -> FadeStep {
    if !running {
        return FadeStep::Cut;
    }
    if outgoing {
        return FadeStep::Handover;
    }
    if incoming && fading {
        return FadeStep::Restart;
    }
    FadeStep::Idle
}

/// The field's own geometry on a panel `width` wide, in panel coordinates.
pub(super) fn field(width: f64, cover: f64) -> (f64, f64, f64, f64) {
    let left = -OVERHANG_LEFT_PER_COVER * cover;
    let top = -OVERHANG_TOP_PER_COVER * cover;
    let field_width = width + (OVERHANG_LEFT_PER_COVER + OVERHANG_RIGHT_PER_COVER) * cover;
    let field_height = FIELD_HEIGHT_PER_COVER * cover;
    (left, top, field_width, field_height)
}

struct Inner {
    back: RefCell<Option<cairo::ImageSurface>>,
    front: RefCell<Option<cairo::ImageSurface>>,
    /// The pair the last cover left behind, still fading out under the new one.
    leaving_back: RefCell<Option<cairo::ImageSurface>>,
    leaving_front: RefCell<Option<cairo::ImageSurface>>,
    /// Reading of the drift clock when the current cover arrived.
    arrived_at_us: Cell<i64>,
    /// Cover generation the cached fields were built from; the panel bumps it
    /// once per rendered track, exactly as `cover_bloom` keys its own cache.
    generation: Cell<Option<u64>>,
    started_at_us: Cell<i64>,
    frame_time_us: Cell<i64>,
    pinned: Cell<bool>,
}

#[derive(Clone)]
pub(super) struct CoverCloud {
    area: gtk4::DrawingArea,
    inner: Rc<Inner>,
}

impl CoverCloud {
    pub(super) fn new() -> Self {
        let area = gtk4::DrawingArea::new();
        area.add_css_class("reprise-now-playing-cloud");
        area.set_can_target(false);
        area.set_can_focus(false);
        area.set_visible(false);
        let inner = Rc::new(Inner {
            back: RefCell::new(None),
            front: RefCell::new(None),
            leaving_back: RefCell::new(None),
            leaving_front: RefCell::new(None),
            arrived_at_us: Cell::new(0),
            generation: Cell::new(None),
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

    /// The cover both fields are cut from, or `None` for external media, a
    /// placeholder, or no track. Without artwork the clouds stay dark: a light
    /// whose colour is not in the record is the dishonesty this layer exists to
    /// avoid.
    pub(super) fn set_cover(&self, texture: Option<&gtk4::gdk::Texture>, generation: u64) {
        match texture {
            Some(texture) => {
                if self.inner.generation.get() == Some(generation) {
                    return;
                }
                let back = build_field(texture, BACK_BLUR_EDGE, &BACK_BLOBS);
                let front = build_field(texture, FRONT_BLUR_EDGE, &FRONT_BLOBS);
                self.begin_fade(back, front);
                self.inner.generation.set(Some(generation));
            }
            None => {
                self.begin_fade(None, None);
                self.inner.generation.set(None);
            }
        }
        self.area.queue_draw();
    }

    /// Puts the incoming pair up and keeps the outgoing one to fade under it.
    ///
    /// With the clock stopped — the panel pinned, or animation switched off —
    /// there is no frame to carry a fade, so the change is taken at once
    /// rather than left half-finished on screen.
    fn begin_fade(&self, back: Option<cairo::ImageSurface>, front: Option<cairo::ImageSurface>) {
        let running = !self.inner.pinned.get() && crate::ui::motion::animations_enabled();
        let incoming = back.is_some() || front.is_some();
        let outgoing_back = self.inner.back.replace(back);
        let outgoing_front = self.inner.front.replace(front);
        let outgoing = outgoing_back.is_some() || outgoing_front.is_some();
        let fading = self.inner.leaving_back.borrow().is_some()
            || self.inner.leaving_front.borrow().is_some();
        match fade_step(running, incoming, outgoing, fading) {
            FadeStep::Cut => {
                *self.inner.leaving_back.borrow_mut() = None;
                *self.inner.leaving_front.borrow_mut() = None;
                self.inner.arrived_at_us.set(0);
            }
            FadeStep::Handover => {
                *self.inner.leaving_back.borrow_mut() = outgoing_back;
                *self.inner.leaving_front.borrow_mut() = outgoing_front;
                self.inner.arrived_at_us.set(self.inner.frame_time_us.get());
            }
            FadeStep::Restart => self.inner.arrived_at_us.set(self.inner.frame_time_us.get()),
            FadeStep::Idle => {}
        }
    }

    pub(super) fn set_frame_time(&self, frame_time_us: i64) {
        if self.inner.pinned.get() {
            return;
        }
        // A setting that has switched animation off freezes the clock rather
        // than the clouds: the composition stays, the motion stops.
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
            self.drop_faded_cover(elapsed_us);
            self.area.queue_draw();
        }
    }

    /// Lets go of the outgoing rasters the moment they stop being drawn, so a
    /// long queue does not keep one dead field per track change alive.
    fn drop_faded_cover(&self, elapsed_us: i64) {
        if self.inner.leaving_back.borrow().is_none() && self.inner.leaving_front.borrow().is_none()
        {
            return;
        }
        let since_s =
            elapsed_us.saturating_sub(self.inner.arrived_at_us.get()) as f64 / 1_000_000.0;
        if cover_fade(since_s) >= 1.0 {
            *self.inner.leaving_back.borrow_mut() = None;
            *self.inner.leaving_front.borrow_mut() = None;
        }
    }

    pub(super) fn set_pinned(&self, pinned: bool) {
        self.inner.pinned.set(pinned);
        self.area.set_visible(!pinned);
        if pinned {
            self.inner.started_at_us.set(0);
            self.inner.frame_time_us.set(0);
            self.inner.arrived_at_us.set(0);
            *self.inner.leaving_back.borrow_mut() = None;
            *self.inner.leaving_front.borrow_mut() = None;
        }
        self.area.queue_draw();
    }
}

/// Bakes one layer: the blurred cover painted across the field, then the
/// mockup's radial stops taken out of its alpha.
fn build_field(
    texture: &gtk4::gdk::Texture,
    blur_edge: i32,
    blobs: &[Blob],
) -> Option<cairo::ImageSurface> {
    let blurred = cover_glow::blurred_surface(texture)?;
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, FIELD_RASTER_EDGE, FIELD_RASTER_EDGE)
            .ok()?;
    let cr = cairo::Context::new(&surface).ok()?;
    let edge = f64::from(FIELD_RASTER_EDGE);

    // Bilinear over a large upscale is what makes this a blur at all, exactly
    // as in `cover_bloom`. The smaller the source edge, the softer the result.
    let scale = edge / f64::from(blur_edge);
    cr.save().ok();
    cr.scale(scale, scale);
    if cr.set_source_surface(&blurred, 0.0, 0.0).is_ok() {
        cr.source().set_filter(cairo::Filter::Bilinear);
        cr.source().set_extend(cairo::Extend::Pad);
        cr.paint().ok();
    }
    cr.restore().ok();

    // Every blob is one radial stop of the mockup, cut out of the alpha rather
    // than painted in colour. Drawn into a mask of their own first so two
    // overlapping blobs add up instead of the second clipping the first away.
    let mask =
        cairo::ImageSurface::create(cairo::Format::ARgb32, FIELD_RASTER_EDGE, FIELD_RASTER_EDGE)
            .ok()?;
    let mask_cr = cairo::Context::new(&mask).ok()?;
    for blob in blobs {
        let cx = blob.x * edge;
        let cy = blob.y * edge;
        let radius = blob.radius * edge;
        let stop = cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, radius);
        stop.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, blob.alpha);
        stop.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
        mask_cr.set_source(&stop).ok();
        mask_cr.paint().ok();
    }
    cr.set_operator(cairo::Operator::DestIn);
    if cr.set_source_surface(&mask, 0.0, 0.0).is_ok() {
        cr.paint().ok();
    }
    Some(surface)
}

fn draw(cr: &cairo::Context, width: i32, height: i32, inner: &Inner) {
    let band = f64::from(tokens::NOW_PLAYING_ARTWORK_BAND).min(f64::from(height));
    let width = f64::from(width);
    if width <= 0.0 || band <= 0.0 {
        return;
    }
    let cover = f64::from(tokens::NOW_PLAYING_COVER_SIZE);
    let (field_left, field_top, field_width, field_height) = field(width, cover);
    let elapsed_s = inner.frame_time_us.get() as f64 / 1_000_000.0;

    cr.save().ok();
    cr.rectangle(0.0, 0.0, width, band);
    cr.clip();

    let bounds = (field_left, field_top, field_width, field_height);
    // Read once per frame rather than once per layer: both the operator and the
    // scrim colour come from the same answer.
    let dark = crate::ui::style::accent::is_dark();
    let operator = if dark {
        cairo::Operator::Screen
    } else {
        cairo::Operator::Multiply
    };
    let back_drift = drift_at(elapsed_s, BACK_PERIOD_S, 0.0);
    let front_drift = drift_at(elapsed_s, FRONT_PERIOD_S, FRONT_OFFSET_S);

    // With the clock standing still — paused, or animation switched off — no
    // frame will ever advance a fade, so the change counts as already done
    // rather than leaving the incoming cover stuck at nothing.
    let clock = inner.frame_time_us.get();
    let arrived = if clock > 0 {
        cover_fade((clock.saturating_sub(inner.arrived_at_us.get())) as f64 / 1_000_000.0)
    } else {
        1.0
    };
    let back = inner.back.borrow();
    let front = inner.front.borrow();
    let leaving_back = inner.leaving_back.borrow();
    let leaving_front = inner.leaving_front.borrow();

    // A cover that arrived with nothing to replace is simply up; only a cover
    // that displaced one has to fade in over it.
    let leaving = leaving_back.is_some() || leaving_front.is_some();
    let incoming_alpha = if leaving { arrived } else { 1.0 };

    let mut painted = false;
    // The outgoing cover first and underneath: both pairs drift on the same
    // clock, so what crosses over is the colour and not the movement.
    for (surface, drift) in [
        (leaving_back.as_ref(), back_drift),
        (leaving_front.as_ref(), front_drift),
    ] {
        if let Some(surface) = surface {
            paint_layer(cr, surface, drift, bounds, 1.0 - arrived, operator);
            painted = true;
        }
    }
    for (surface, drift) in [(back.as_ref(), back_drift), (front.as_ref(), front_drift)] {
        if let Some(surface) = surface {
            paint_layer(cr, surface, drift, bounds, incoming_alpha, operator);
            painted = true;
        }
    }
    if painted {
        paint_scrim(cr, width, band, field_top, field_height);
    }
    cr.restore().ok();
}

/// One layer, moved to where the clock says it is.
///
/// The order is the mockup's own — translate, then scale, then rotate, about
/// the field's centre. The cover is not in this path at all: it is a sibling
/// above this widget and cannot be reached from here, which is what guarantees
/// the rule that it never turns and never grows.
fn paint_layer(
    cr: &cairo::Context,
    surface: &cairo::ImageSurface,
    drift: Drift,
    field: (f64, f64, f64, f64),
    alpha: f64,
    operator: cairo::Operator,
) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let (left, top, field_width, field_height) = field;
    cr.save().ok();
    cr.translate(
        left + field_width / 2.0 + drift.x * field_width,
        top + field_height / 2.0 + drift.y * field_height,
    );
    cr.scale(
        drift.scale * field_width / f64::from(FIELD_RASTER_EDGE),
        drift.scale * field_height / f64::from(FIELD_RASTER_EDGE),
    );
    cr.rotate(drift.rotation_deg.to_radians());
    // Laid on as light, not as a picture. The mockup fills these layers with
    // lit colour — `rgba(255,47,160,.6)` — while what is actually available
    // here is the cover, and a cover is mostly dark towards its edges. Painted
    // straight over the panel that reads as dark on dark, which is nothing at
    // all. Screening it against the ground adds the artwork's colour instead of
    // covering with it, which is the same answer the Android side reached.
    //
    // On a light panel screening would do nothing — the ground is already near
    // white — so there the layer multiplies instead, and the glow becomes the
    // wash of colour the design asks for, out of the same pixels.
    cr.set_operator(operator);
    let centre = f64::from(FIELD_RASTER_EDGE) / 2.0;
    if cr.set_source_surface(surface, -centre, -centre).is_ok() {
        cr.source().set_filter(cairo::Filter::Bilinear);
        cr.paint_with_alpha(alpha).ok();
    }
    cr.restore().ok();
}

/// The fade back to the panel, in the panel's own colour.
fn paint_scrim(cr: &cairo::Context, width: f64, band: f64, field_top: f64, field_height: f64) {
    let [r, g, b] = crate::ui::style::accent::sidebar_background_rgb();
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let fade = cairo::LinearGradient::new(0.0, field_top, 0.0, field_top + field_height);
    for step in 0..=STOPS {
        let y = f64::from(step) / f64::from(STOPS);
        fade.add_color_stop_rgba(y, r, g, b, scrim_alpha(y));
    }
    cr.set_source(&fade).ok();
    cr.rectangle(0.0, 0.0, width, band);
    cr.fill().ok();
}

/// The scrim is a bend, not a line, so it is handed to Cairo as stops along it.
const STOPS: i32 = 24;

#[cfg(test)]
#[path = "cover_cloud_tests.rs"]
mod tests;
