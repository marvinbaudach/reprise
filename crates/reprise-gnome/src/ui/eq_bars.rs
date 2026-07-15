//! A tiny three-bar "now playing" indicator (like a mini equalizer). Purely
//! decorative: drawn on a `DrawingArea`, animated only while active, and static
//! when the desktop disables animations. Later tasks use this to mark the
//! now-playing artist/track in the Artists list and top-tracks rows.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

const BARS: usize = 3;
const SIZE: i32 = 14;

pub(in crate::ui) struct EqBars {
    area: gtk4::DrawingArea,
    phase: Rc<Cell<f64>>,
    active: Cell<bool>,
    tick_id: RefCell<Option<gtk4::TickCallbackId>>,
}

impl EqBars {
    pub(in crate::ui) fn new() -> Self {
        let area = gtk4::DrawingArea::builder()
            .content_width(SIZE)
            .content_height(SIZE)
            .valign(gtk4::Align::Center)
            .build();
        area.add_css_class("eq-bars");
        area.set_visible(false);

        let phase = Rc::new(Cell::new(0.0));

        area.set_draw_func({
            let phase = phase.clone();
            move |area, cr, w, h| {
                // Current foreground colour from CSS, so the bars recolor
                // with the active theme (same approach as waveform_seek.rs).
                let color = area.color();
                cr.set_source_rgba(
                    f64::from(color.red()),
                    f64::from(color.green()),
                    f64::from(color.blue()),
                    0.9,
                );
                let bar_w = (w as f64) / (BARS as f64 * 1.8);
                for i in 0..BARS {
                    let t = phase.get() + i as f64 * 0.7;
                    let frac = 0.35 + 0.55 * (0.5 + 0.5 * t.sin());
                    let bh = (h as f64) * frac;
                    let x = (i as f64) * bar_w * 1.8 + bar_w * 0.4;
                    cr.rectangle(x, h as f64 - bh, bar_w, bh);
                }
                let _ = cr.fill();
            }
        });

        Self {
            area,
            phase,
            active: Cell::new(false),
            tick_id: RefCell::new(None),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::DrawingArea {
        &self.area
    }

    /// Shows and starts animating (if `gtk-enable-animations` is on), or
    /// hides and stops animating. Idempotent — calling with the same value
    /// twice is a no-op, so no double tick-callback registration.
    pub(in crate::ui) fn set_active(&self, active: bool) {
        if self.active.get() == active {
            return;
        }
        self.active.set(active);
        self.area.set_visible(active);

        // Visibility must never depend on animation state — only the tick
        // callback (and thus the animated redraw) is gated on it.
        if !active {
            if let Some(id) = self.tick_id.borrow_mut().take() {
                id.remove();
            }
            self.area.queue_draw();
            return;
        }

        let animations = self.area.settings().is_gtk_enable_animations();
        if animations {
            if self.tick_id.borrow().is_some() {
                return;
            }
            let phase = self.phase.clone();
            let id = self.area.add_tick_callback(move |area, clock| {
                let dt = clock.frame_time() as f64 / 1_000_000.0;
                phase.set(dt * 4.0);
                area.queue_draw();
                glib::ControlFlow::Continue
            });
            *self.tick_id.borrow_mut() = Some(id);
        } else {
            self.area.queue_draw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn eq_bars_widget_builds_and_toggles() {
        if gtk4::init().is_err() {
            return;
        }
        let bars = EqBars::new();
        bars.set_active(true);
        assert!(bars.widget().is_visible());
        bars.set_active(false);
        assert!(!bars.widget().is_visible());
    }
}
