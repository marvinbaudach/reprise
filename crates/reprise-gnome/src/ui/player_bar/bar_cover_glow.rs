//! Cover-derived light behind the player-bar thumbnail.
//!
//! The decoded texture arrives through the bar's existing cover-loader path.
//! This layer only caches the shared 32 px blurred surface and redraws alpha
//! and scale as the live bass pair changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::cairo;
use gtk4::prelude::*;

use crate::ui::cover_glow;

use super::player_bar_layout::{bar_glow_opacity, bar_glow_scale};

const READING_EPSILON: f64 = 0.01;

struct Inner {
    surface: RefCell<Option<cairo::ImageSurface>>,
    generation: Cell<Option<u64>>,
    kick: Cell<f64>,
    pressure: Cell<f64>,
    pinned: Cell<bool>,
}

#[derive(Clone)]
pub(in crate::ui) struct BarCoverGlow {
    area: gtk4::DrawingArea,
    inner: Rc<Inner>,
}

impl BarCoverGlow {
    pub(super) fn new(width: i32, height: i32) -> Self {
        let area = gtk4::DrawingArea::new();
        area.set_content_width(width);
        area.set_content_height(height);
        area.set_can_target(false);
        area.set_can_focus(false);
        let inner = Rc::new(Inner {
            surface: RefCell::new(None),
            generation: Cell::new(None),
            kick: Cell::new(0.0),
            pressure: Cell::new(0.0),
            pinned: Cell::new(false),
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

    pub(in crate::ui) fn set_cover(&self, texture: Option<&gtk4::gdk::Texture>, generation: u64) {
        match texture {
            Some(texture) if self.inner.generation.get() != Some(generation) => {
                *self.inner.surface.borrow_mut() = cover_glow::blurred_surface(texture);
                self.inner.generation.set(Some(generation));
            }
            Some(_) => return,
            None => {
                *self.inner.surface.borrow_mut() = None;
                self.inner.generation.set(None);
            }
        }
        self.area.queue_draw();
    }

    pub(super) fn set_bass(&self, kick: f64, pressure: f64) {
        if self.inner.pinned.get()
            || ((self.inner.kick.get() - kick).abs() < READING_EPSILON
                && (self.inner.pressure.get() - pressure).abs() < READING_EPSILON)
        {
            return;
        }
        self.inner.kick.set(kick.clamp(0.0, 1.0));
        self.inner.pressure.set(pressure.clamp(0.0, 1.0));
        self.area.queue_draw();
    }

    pub(super) fn set_pinned(&self, pinned: bool) {
        if self.inner.pinned.replace(pinned) == pinned {
            return;
        }
        if pinned {
            self.inner.kick.set(0.0);
            self.inner.pressure.set(0.0);
        }
        self.area.queue_draw();
    }
}

fn draw(cr: &cairo::Context, width: i32, height: i32, inner: &Inner) {
    if width <= 0 || height <= 0 {
        return;
    }
    let surface = inner.surface.borrow();
    let Some(surface) = surface.as_ref() else {
        return;
    };
    let (kick, pressure) = if inner.pinned.get() {
        (0.0, 0.0)
    } else {
        (inner.kick.get(), inner.pressure.get())
    };
    let opacity = bar_glow_opacity(kick, pressure);
    let scale = bar_glow_scale(kick);
    let diameter = f64::from(width) * scale;
    let radius = diameter / 2.0;
    let center_x = f64::from(width) / 2.0;
    let center_y = f64::from(height) / 2.0;

    cr.save().ok();
    // The bar is shorter than the glow's diameter. The widget boundary clips
    // the vertical overflow, while the circle prevents a rectangular seam at
    // the bar's top and bottom edges.
    cr.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
    cr.clip();
    cr.arc(center_x, center_y, radius, 0.0, std::f64::consts::TAU);
    cr.clip();
    cr.translate(center_x - radius, center_y - radius);
    cr.scale(
        diameter / f64::from(cover_glow::BLUR_EDGE),
        diameter / f64::from(cover_glow::BLUR_EDGE),
    );
    if cr.set_source_surface(surface, 0.0, 0.0).is_ok() {
        cr.source().set_filter(cairo::Filter::Bilinear);
        cr.source().set_extend(cairo::Extend::Pad);
        cr.paint_with_alpha(opacity).ok();
    }
    cr.restore().ok();
}
