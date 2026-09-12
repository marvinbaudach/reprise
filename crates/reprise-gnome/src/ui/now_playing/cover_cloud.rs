//! Six soft drops drifting behind the cover, cut from the cover itself.
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
//! Cost is the bloom's bargain: the six masked clouds are rasterized once per
//! cover; a reusable 320 px scratch unions each light-appearance layer before
//! its one Multiply pass, while dark appearance keeps six independent Screen
//! passes. That is eight colour paints plus two scratch clears in light and six
//! paints in dark per frame, doubled during a cover cross-fade, then the scrim.
//! The cloud rasters and scratch occupy 2.73 MiB normally and 5.08 MiB
//! mid-cross-fade. Nothing is allocated or re-rasterized when the clock moves,
//! and faded cover rasters are released.
//!
//! The clock is the only thing that moves the clouds. No spectrum reading
//! reaches them — the bloom next door is where the music is allowed in.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::cairo;
use gtk4::prelude::*;

use super::cover_cloud_blob::{
    build_blob_rasters, Blob, BlobRasters, BACK_BLOBS, BACK_BLUR_EDGE, BLOBS_PER_LAYER,
    FIELD_RASTER_EDGE, FRONT_BLOBS, FRONT_BLUR_EDGE,
};
use super::cover_scrim::{self, ScrimCache};
#[cfg(test)]
use super::cover_scrim::{build_scrim, paint_scrim};
use crate::ui::style::tokens;

/// The field, and how far it hangs off each edge, as multiples of the cover.
///
/// The mockup is drawn against a 240 px cover; this panel's is 184. Carrying
/// the numbers as ratios rather than pixels is what lets the panel keep its own
/// size — the same way the module this replaces carried its disc as `520/184`.
/// The overhang is widest on the right: the weight leans outward, away from the
/// track list.
const FIELD_HEIGHT_PER_COVER: f64 = 440.0 / 240.0;
const OVERHANG_TOP_PER_COVER: f64 = 60.0 / 240.0;
const OVERHANG_LEFT_PER_COVER: f64 = 40.0 / 240.0;
const OVERHANG_RIGHT_PER_COVER: f64 = 90.0 / 240.0;

/// The full envelope available to each independently moving parameter.
const DRIFT_X: (f64, f64) = (-0.20, 0.16);
const DRIFT_Y: (f64, f64) = (-0.12, 0.12);
const DRIFT_SCALE: (f64, f64) = (1.40, 1.55);

/// The binding speed ceiling from AC-24, in field-fractions per second.
#[cfg(test)]
pub(super) const DRIFT_SPEED_LIMIT: f64 = 0.016;

const SLOW_WAVE_WEIGHT: f64 = 0.65;
const FAST_WAVE_WEIGHT: f64 = 0.35;

/// Minimum spare canvas around the clipped artwork band at every drift extreme.
/// A future panel-width or cloud-anchor change must preserve at least this much
/// room before a hard raster edge can become visible.
#[cfg(test)]
const MIN_VISIBLE_RASTER_MARGIN_PX: f64 = 5.0;

/// Two long waves for one pose parameter, with phase measured in turns.
#[derive(Clone, Copy)]
pub(super) struct DriftAxis {
    pub(super) slow_s: f64,
    pub(super) fast_s: f64,
    pub(super) slow_phase: f64,
    pub(super) fast_phase: f64,
}

/// The independent wave pair for every visible parameter of one drop.
#[derive(Clone, Copy)]
pub(super) struct DriftProfile {
    pub(super) x: DriftAxis,
    pub(super) y: DriftAxis,
    pub(super) scale: DriftAxis,
}

/// How long a change of track takes to arrive in the light.
///
/// The clouds do not cut to the new cover: the outgoing pair fades out under
/// the incoming one over a second, so a track change reads as the colour
/// turning rather than as a jump.
const COVER_FADE_S: f64 = 1.0;

/// Where a drop has drifted to and how far it is breathing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Drift {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) scale: f64,
}

/// One continuous wave, wrapped in turns before scaling so a long session does
/// not lose its fraction into a stutter.
fn wave(elapsed_s: f64, period_s: f64, phase: f64) -> f64 {
    let turn = (elapsed_s / period_s + phase).rem_euclid(1.0);
    (std::f64::consts::TAU * turn).sin()
}

/// Two incommensurable waves whose weighted sum remains inside `-1.0..=1.0`.
fn drift_axis(elapsed_s: f64, axis: DriftAxis) -> f64 {
    SLOW_WAVE_WEIGHT * wave(elapsed_s, axis.slow_s, axis.slow_phase)
        + FAST_WAVE_WEIGHT * wave(elapsed_s, axis.fast_s, axis.fast_phase)
}

/// The pose at `elapsed_s`, with no parameter sharing another's motion.
pub(super) fn drift_at(elapsed_s: f64, profile: DriftProfile) -> Drift {
    Drift {
        x: map_axis(DRIFT_X, drift_axis(elapsed_s, profile.x)),
        y: map_axis(DRIFT_Y, drift_axis(elapsed_s, profile.y)),
        scale: map_axis(DRIFT_SCALE, drift_axis(elapsed_s, profile.scale)),
    }
}

fn map_axis((from, to): (f64, f64), value: f64) -> f64 {
    let centre = (from + to) / 2.0;
    let half_range = (to - from) / 2.0;
    centre + half_range * value
}

#[cfg(test)]
fn raster_margins_at_extremes(
    blob: Blob,
    (left, top, field_width, field_height): (f64, f64, f64, f64),
    (clip_left, clip_top, clip_width, clip_height): (f64, f64, f64, f64),
) -> [f64; 4] {
    // Each edge is tightest when translation moves the canvas inward and the
    // independently bounded scale is smallest.
    let scale = DRIFT_SCALE.0;
    let raster_left = left + field_width * (blob.x + DRIFT_X.1 - scale * blob.x);
    let raster_right = left + field_width * (blob.x + DRIFT_X.0 + scale * (1.0 - blob.x));
    let raster_top = top + field_height * (blob.y + DRIFT_Y.1 - scale * blob.y);
    let raster_bottom = top + field_height * (blob.y + DRIFT_Y.0 + scale * (1.0 - blob.y));
    [
        clip_left - raster_left,
        raster_right - (clip_left + clip_width),
        clip_top - raster_top,
        raster_bottom - (clip_top + clip_height),
    ]
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

#[derive(Clone, Copy, Default)]
struct DriftClock {
    started_at_us: i64,
    carried_us: i64,
    elapsed_us: i64,
}

impl DriftClock {
    /// Advances the active segment, returning whether the drawn pose changed.
    fn advance(&mut self, frame_time_us: i64) -> bool {
        if self.started_at_us == 0 {
            self.started_at_us = frame_time_us;
        }
        let elapsed_us = self
            .carried_us
            .saturating_add(frame_time_us.saturating_sub(self.started_at_us))
            .max(self.elapsed_us);
        if elapsed_us == self.elapsed_us {
            return false;
        }
        self.elapsed_us = elapsed_us;
        true
    }

    /// Folds the active segment into the pose carried across a pause.
    fn hold(&mut self) -> bool {
        if self.started_at_us == 0 {
            return false;
        }
        self.started_at_us = 0;
        self.carried_us = self.elapsed_us;
        true
    }

    fn elapsed_us(self) -> i64 {
        self.elapsed_us
    }

    fn elapsed_s(self) -> f64 {
        self.elapsed_us as f64 / 1_000_000.0
    }
}

struct LayerScratch {
    surface: cairo::ImageSurface,
    context: cairo::Context,
}

impl LayerScratch {
    fn new() -> Option<Self> {
        let surface = cairo::ImageSurface::create(
            cairo::Format::ARgb32,
            FIELD_RASTER_EDGE,
            FIELD_RASTER_EDGE,
        )
        .ok()?;
        let context = cairo::Context::new(&surface).ok()?;
        Some(Self { surface, context })
    }
}

struct Inner {
    back: RefCell<Option<BlobRasters>>,
    front: RefCell<Option<BlobRasters>>,
    /// The two raster sets the last cover left behind, still fading out.
    leaving_back: RefCell<Option<BlobRasters>>,
    leaving_front: RefCell<Option<BlobRasters>>,
    /// Reused per layer so light appearance reaches the panel with at most two
    /// Multiply passes, while retaining each cloud's independently posed mask.
    layer_scratch: Option<LayerScratch>,
    /// Reading of the drift clock when the current cover arrived.
    arrived_at_us: Cell<i64>,
    /// Cover generation the cached fields were built from; the panel bumps it
    /// once per rendered track, exactly as `cover_bloom` keys its own cache.
    generation: Cell<Option<u64>>,
    drift_clock: Cell<DriftClock>,
    last_drawn_pose: Cell<Option<([Drift; BLOBS_PER_LAYER], [Drift; BLOBS_PER_LAYER])>>,
    scrim: RefCell<Option<ScrimCache>>,
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
            layer_scratch: LayerScratch::new(),
            arrived_at_us: Cell::new(0),
            generation: Cell::new(None),
            drift_clock: Cell::new(DriftClock::default()),
            last_drawn_pose: Cell::new(None),
            scrim: RefCell::new(None),
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
    pub(super) fn drawn_pose_for_test(
        &self,
    ) -> Option<([Drift; BLOBS_PER_LAYER], [Drift; BLOBS_PER_LAYER])> {
        self.inner.last_drawn_pose.get()
    }

    #[cfg(test)]
    pub(super) fn has_leaving_pair_for_test(&self) -> bool {
        let has_back = self.inner.leaving_back.borrow().is_some();
        let has_front = self.inner.leaving_front.borrow().is_some();
        has_back && has_front
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
                let back = build_blob_rasters(texture, BACK_BLUR_EDGE, &BACK_BLOBS);
                let front = build_blob_rasters(texture, FRONT_BLUR_EDGE, &FRONT_BLOBS);
                if let Some((back, front)) = complete_raster_pair(back, front) {
                    self.begin_fade(Some(back), Some(front));
                    self.inner.generation.set(Some(generation));
                }
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
    fn begin_fade(&self, back: Option<BlobRasters>, front: Option<BlobRasters>) {
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
                self.inner
                    .arrived_at_us
                    .set(self.inner.drift_clock.get().elapsed_us());
            }
            FadeStep::Restart => self
                .inner
                .arrived_at_us
                .set(self.inner.drift_clock.get().elapsed_us()),
            FadeStep::Idle => {}
        }
    }

    pub(super) fn set_frame_time(&self, frame_time_us: i64) {
        if self.inner.pinned.get() {
            return;
        }
        // A setting that has switched animation off freezes the clock rather
        // than the clouds: the composition stays, the motion stops.
        let mut clock = self.inner.drift_clock.get();
        if frame_time_us <= 0 || !crate::ui::motion::animations_enabled() {
            if clock.hold() {
                self.inner.drift_clock.set(clock);
                self.area.queue_draw();
            }
            return;
        }
        let changed = clock.advance(frame_time_us);
        self.inner.drift_clock.set(clock);
        if changed {
            self.drop_faded_cover(clock.elapsed_us());
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
            self.inner.drift_clock.set(DriftClock::default());
            self.inner.arrived_at_us.set(0);
            *self.inner.leaving_back.borrow_mut() = None;
            *self.inner.leaving_front.borrow_mut() = None;
        }
        self.area.queue_draw();
    }

    #[cfg(test)]
    pub(super) fn draw_for_test(&self, cr: &cairo::Context, width: i32, height: i32) {
        draw(cr, width, height, &self.inner);
    }

    #[cfg(test)]
    pub(super) fn paint_layers_only_for_test(&self, cr: &cairo::Context, width: i32, height: i32) {
        let band = f64::from(tokens::NOW_PLAYING_ARTWORK_BAND).min(f64::from(height));
        let width = f64::from(width);
        if width <= 0.0 || band <= 0.0 {
            return;
        }
        cr.save().ok();
        cr.rectangle(0.0, 0.0, width, band);
        cr.clip();
        paint_layers_only(cr, width, &self.inner, crate::ui::style::accent::is_dark());
        cr.restore().ok();
    }
}

fn complete_raster_pair(
    back: Option<BlobRasters>,
    front: Option<BlobRasters>,
) -> Option<(BlobRasters, BlobRasters)> {
    Some((back?, front?))
}

fn draw(cr: &cairo::Context, width: i32, height: i32, inner: &Inner) {
    let band = f64::from(tokens::NOW_PLAYING_ARTWORK_BAND).min(f64::from(height));
    let width = f64::from(width);
    if width <= 0.0 || band <= 0.0 {
        return;
    }
    cr.save().ok();
    cr.rectangle(0.0, 0.0, width, band);
    cr.clip();

    // Read once per frame rather than once per layer: both the operator and the
    // scrim colour come from the same answer.
    let dark = crate::ui::style::accent::is_dark();
    let painted = paint_layers_only(cr, width, inner, dark);
    if painted {
        cover_scrim::paint(cr, &inner.scrim, dark, width, band);
    }
    cr.restore().ok();
}

fn paint_layers_only(cr: &cairo::Context, width: f64, inner: &Inner, dark: bool) -> bool {
    let cover = f64::from(tokens::NOW_PLAYING_COVER_SIZE);
    let bounds = field(width, cover);
    let clock = inner.drift_clock.get();
    let elapsed_s = clock.elapsed_s();
    let operator = blend_operator(dark);
    let back_poses = std::array::from_fn(|index| drift_at(elapsed_s, BACK_BLOBS[index].drift));
    let front_poses = std::array::from_fn(|index| drift_at(elapsed_s, FRONT_BLOBS[index].drift));
    inner.last_drawn_pose.set(Some((back_poses, front_poses)));

    // With the clock standing still — paused, or animation switched off — no
    // frame will ever advance a fade, so the change counts as already done
    // rather than leaving the incoming cover stuck at nothing.
    let elapsed_us = clock.elapsed_us();
    let arrived = if elapsed_us > 0 {
        cover_fade((elapsed_us.saturating_sub(inner.arrived_at_us.get())) as f64 / 1_000_000.0)
    } else {
        1.0
    };
    let back = inner.back.borrow();
    let front = inner.front.borrow();
    let leaving_back = inner.leaving_back.borrow();
    let leaving_front = inner.leaving_front.borrow();
    let layer_scratch = inner.layer_scratch.as_ref();

    // A cover that arrived with nothing to replace is simply up; only a cover
    // that displaced one has to fade in over it.
    let leaving = leaving_back.is_some() || leaving_front.is_some();
    let incoming_alpha = if leaving { arrived } else { 1.0 };

    // The outgoing cover first and underneath: both pairs drift on the same
    // clock, so what crosses over is the colour and not the movement.
    paint_crossfade_layers(
        cr,
        &[
            (leaving_back.as_ref(), &BACK_BLOBS, &back_poses),
            (leaving_front.as_ref(), &FRONT_BLOBS, &front_poses),
        ],
        &[
            (back.as_ref(), &BACK_BLOBS, &back_poses),
            (front.as_ref(), &FRONT_BLOBS, &front_poses),
        ],
        arrived,
        incoming_alpha,
        LayerComposite {
            bounds,
            operator,
            scratch: layer_scratch,
        },
    )
}

fn blend_operator(dark: bool) -> cairo::Operator {
    if dark {
        cairo::Operator::Screen
    } else {
        cairo::Operator::Multiply
    }
}

type LayerPaint<'a> = (
    Option<&'a BlobRasters>,
    &'a [Blob; BLOBS_PER_LAYER],
    &'a [Drift; BLOBS_PER_LAYER],
);

#[derive(Clone, Copy)]
struct LayerComposite<'a> {
    bounds: (f64, f64, f64, f64),
    operator: cairo::Operator,
    scratch: Option<&'a LayerScratch>,
}

fn paint_crossfade_layers(
    cr: &cairo::Context,
    outgoing: &[LayerPaint<'_>],
    incoming: &[LayerPaint<'_>],
    arrived: f64,
    incoming_alpha: f64,
    composite: LayerComposite<'_>,
) -> bool {
    let mut painted = false;
    for (surfaces, blobs, poses) in outgoing {
        if let Some(surfaces) = surfaces {
            painted |= paint_cloud_layer(cr, surfaces, blobs, poses, 1.0 - arrived, composite);
        }
    }
    for (surfaces, blobs, poses) in incoming {
        if let Some(surfaces) = surfaces {
            painted |= paint_cloud_layer(cr, surfaces, blobs, poses, incoming_alpha, composite);
        }
    }
    painted
}

fn paint_cloud_layer(
    cr: &cairo::Context,
    surfaces: &BlobRasters,
    blobs: &[Blob; BLOBS_PER_LAYER],
    poses: &[Drift; BLOBS_PER_LAYER],
    alpha: f64,
    composite: LayerComposite<'_>,
) -> bool {
    if alpha <= 0.0 {
        return false;
    }
    if composite.operator != cairo::Operator::Multiply {
        for index in 0..BLOBS_PER_LAYER {
            paint_layer(
                cr,
                &surfaces[index],
                (blobs[index].x, blobs[index].y),
                poses[index],
                composite.bounds,
                alpha,
                composite.operator,
            );
        }
        return true;
    }

    let Some(scratch) = composite.scratch else {
        return false;
    };
    let scratch_cr = &scratch.context;
    scratch_cr.set_operator(cairo::Operator::Clear);
    scratch_cr.paint().ok();
    scratch_cr.set_operator(cairo::Operator::Over);
    let edge = f64::from(FIELD_RASTER_EDGE);
    for index in 0..BLOBS_PER_LAYER {
        paint_layer(
            scratch_cr,
            &surfaces[index],
            (blobs[index].x, blobs[index].y),
            poses[index],
            (0.0, 0.0, edge, edge),
            1.0,
            cairo::Operator::Over,
        );
    }
    // A persistent context otherwise retains the last raster as its source and
    // could keep a faded cover alive after its cache entry has been dropped.
    scratch_cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    paint_surface_across_field(
        cr,
        &scratch.surface,
        composite.bounds,
        alpha,
        composite.operator,
    );
    true
}

fn paint_surface_across_field(
    cr: &cairo::Context,
    surface: &cairo::ImageSurface,
    (left, top, width, height): (f64, f64, f64, f64),
    alpha: f64,
    operator: cairo::Operator,
) {
    cr.save().ok();
    cr.translate(left, top);
    cr.scale(
        width / f64::from(FIELD_RASTER_EDGE),
        height / f64::from(FIELD_RASTER_EDGE),
    );
    cr.set_operator(operator);
    if cr.set_source_surface(surface, 0.0, 0.0).is_ok() {
        cr.source().set_filter(cairo::Filter::Bilinear);
        cr.paint_with_alpha(alpha.clamp(0.0, 1.0)).ok();
    }
    cr.restore().ok();
}

/// One drop, moved to where the clock says it is.
///
/// Scaling is centred on the drop's own anchor, so breathing never moves its
/// centre beyond the separately bounded translation. The cover is not in this
/// path at all: it is a sibling
/// above this widget and cannot be reached from here, which is what guarantees
/// the rule that it never turns and never grows.
fn paint_layer(
    cr: &cairo::Context,
    surface: &cairo::ImageSurface,
    anchor: (f64, f64),
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
    let (anchor_x, anchor_y) = anchor;
    cr.save().ok();
    cr.translate(
        left + (anchor_x + drift.x) * field_width,
        top + (anchor_y + drift.y) * field_height,
    );
    cr.scale(
        drift.scale * field_width / f64::from(FIELD_RASTER_EDGE),
        drift.scale * field_height / f64::from(FIELD_RASTER_EDGE),
    );
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
    let edge = f64::from(FIELD_RASTER_EDGE);
    if cr
        .set_source_surface(surface, -anchor_x * edge, -anchor_y * edge)
        .is_ok()
    {
        cr.source().set_filter(cairo::Filter::Bilinear);
        cr.paint_with_alpha(alpha).ok();
    }
    cr.restore().ok();
}

#[cfg(test)]
#[path = "cover_cloud_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cover_cloud_drift_tests.rs"]
mod drift_tests;
