//! Theme-independent play and pause geometry for the main transport button.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::{cairo, prelude::*};

const PLAY_WIDTH: f64 = 0.50;
const PLAY_TOP: f64 = 0.24;
const PLAY_BOTTOM: f64 = 0.76;
const PAUSE_BAR_CENTRE_OFFSET: f64 = 0.14;
const PAUSE_BAR_WIDTH: f64 = 0.12;
const PAUSE_TOP: f64 = 0.30;
const PAUSE_BOTTOM: f64 = 0.70;
const GLYPH_SIZE: i32 = 24;

/// Horizontal extent of the play triangle, placed so its **centroid** — not
/// its bounding box — sits on the button's centre.
///
/// The triangle has its base on the left and its apex on the right, so two
/// thirds of its area lie in the left half and its centroid falls one third of
/// the way across. A bounding-box-centred triangle therefore reads as sitting
/// too far left, and the correction moves it right.
///
/// This shipped inverted: the old comment had the premise right ("mass sits
/// left of its geometric centre") and the conclusion backwards ("reads as
/// right-heavy"), and the code subtracted its shift — pushing the triangle
/// further left, which is what made the play button look off centre inside a
/// button the layout had centred to the pixel. Written as the centroid rule
/// there is no sign left to get wrong.
pub(super) fn play_ink_bounds(size: f64) -> (f64, f64) {
    let width = PLAY_WIDTH * size;
    let x0 = size / 2.0 - width / 3.0;
    (x0, x0 + width)
}

pub(super) fn pause_ink_bounds(size: f64) -> (f64, f64) {
    let half_width = PAUSE_BAR_WIDTH * size / 2.0;
    (
        size * (0.5 - PAUSE_BAR_CENTRE_OFFSET) - half_width,
        size * (0.5 + PAUSE_BAR_CENTRE_OFFSET) + half_width,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Glyph {
    Play,
    Pause,
}

#[derive(Clone)]
pub(super) struct TransportGlyph {
    area: gtk4::DrawingArea,
    glyph: Rc<Cell<Glyph>>,
}

impl TransportGlyph {
    pub(super) fn new(glyph: Glyph) -> Self {
        let area = gtk4::DrawingArea::builder()
            .content_width(GLYPH_SIZE)
            .content_height(GLYPH_SIZE)
            .can_target(false)
            .can_focus(false)
            .build();
        area.set_accessible_role(gtk4::AccessibleRole::Presentation);
        let state = Rc::new(Cell::new(glyph));
        area.set_draw_func({
            let state = state.clone();
            move |area, cr, width, height| draw(area, cr, width, height, state.get())
        });
        Self { area, glyph: state }
    }

    pub(super) fn widget(&self) -> &gtk4::DrawingArea {
        &self.area
    }

    pub(super) fn set_glyph(&self, glyph: Glyph) {
        if self.glyph.replace(glyph) != glyph {
            self.area.queue_draw();
        }
    }

    #[cfg(test)]
    pub(super) fn glyph(&self) -> Glyph {
        self.glyph.get()
    }
}

fn draw(area: &gtk4::DrawingArea, cr: &cairo::Context, width: i32, height: i32, glyph: Glyph) {
    let size = f64::from(width.min(height));
    if size <= 0.0 {
        return;
    }
    let x_origin = (f64::from(width) - size) / 2.0;
    let y_origin = (f64::from(height) - size) / 2.0;
    let colour = area.color();
    cr.set_source_rgba(
        f64::from(colour.red()),
        f64::from(colour.green()),
        f64::from(colour.blue()),
        f64::from(colour.alpha()),
    );
    match glyph {
        Glyph::Play => draw_play(cr, x_origin, y_origin, size),
        Glyph::Pause => draw_pause(cr, x_origin, y_origin, size),
    }
}

fn draw_play(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    let (x0, x1) = play_ink_bounds(size);
    cr.move_to(x + x0, y + PLAY_TOP * size);
    cr.line_to(x + x1, y + size / 2.0);
    cr.line_to(x + x0, y + PLAY_BOTTOM * size);
    cr.close_path();
    cr.fill().ok();
}

fn draw_pause(cr: &cairo::Context, x: f64, y: f64, size: f64) {
    let line_width = PAUSE_BAR_WIDTH * size;
    let (x0, x1) = pause_ink_bounds(size);
    cr.set_line_width(line_width);
    cr.set_line_cap(cairo::LineCap::Round);
    for bar_x in [x + x0 + line_width / 2.0, x + x1 - line_width / 2.0] {
        cr.move_to(bar_x, y + PAUSE_TOP * size);
        cr.line_to(bar_x, y + PAUSE_BOTTOM * size);
        cr.stroke().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_24_the_pause_glyph_is_centred_and_the_play_glyph_is_optically_offset() {
        // Pause is two vertical bars and symmetric: dead centre.
        let (x0, x1) = pause_ink_bounds(24.0);
        assert!(((x0 + x1) / 2.0 - 12.0).abs() < 1e-9, "pause is off centre");

        // The play triangle is centred on its centroid, which sits one third
        // of the way from base to apex — so its bounding box lands right of
        // centre by a sixth of its width, not left of it.
        let (x0, x1) = play_ink_bounds(24.0);
        let width = x1 - x0;
        let centroid = x0 + width / 3.0;
        assert!(
            (centroid - 12.0).abs() < 1e-9,
            "play is not optically centred: centroid {centroid}, width {width}"
        );
        assert!(
            x0 > (24.0 - width) / 2.0,
            "the correction moved the triangle left again: x0 {x0}"
        );
    }
}
