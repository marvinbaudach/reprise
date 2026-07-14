//! Custom waveform seek bar: draws precomputed peaks with a played/unplayed
//! split and turns a pointer position into a 0..1 seek fraction through its own
//! gesture (so, unlike `GtkScale`, there is no built-in trough-warp gesture to
//! fight — see the GtkRange note in the gtk4 building skill).
//!
//! Colours come from the widget's own CSS `color` (set to
//! `@reprise_player_accent` by the player-bar CSS), so the waveform recolors
//! with the active theme. Wired into the player bar in the following slice —
//! hence the module-level dead-code allow.
#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

/// Shared, cloneable slot for the optional seek handler (cloned out before it
/// is invoked so no `RefCell` borrow is held across the call).
type SeekCallback = Rc<RefCell<Option<Rc<dyn Fn(f64)>>>>;

pub(super) const WAVEFORM_CSS_CLASS: &str = "waveform-seek";
const CONTENT_HEIGHT: i32 = 28;
/// Floor so a near-silent sample still draws a visible sliver.
const MIN_BAR_HEIGHT_FRACTION: f64 = 0.08;
/// Alpha applied to not-yet-played bars (played bars use full alpha).
const UNPLAYED_ALPHA: f64 = 0.28;
/// Gap between bars as a fraction of each bar's horizontal slot.
const BAR_GAP_FRACTION: f64 = 0.35;

/// Maps a pointer `x` within `width` to a 0..1 seek fraction.
fn fraction_at(x: f64, width: f64) -> f64 {
    if width <= 0.0 {
        return 0.0;
    }
    (x / width).clamp(0.0, 1.0)
}

/// Whether bar `index` of `count` falls within the played `fraction` (using the
/// bar's centre so the split lands mid-bar rather than on an edge).
fn bar_played(index: usize, count: usize, fraction: f64) -> bool {
    if count == 0 {
        return false;
    }
    ((index as f64 + 0.5) / count as f64) <= fraction
}

struct State {
    peaks: Vec<f32>,
    fraction: f64,
}

pub(super) struct WaveformSeek {
    area: gtk4::DrawingArea,
    state: Rc<RefCell<State>>,
    on_seek: SeekCallback,
}

impl WaveformSeek {
    pub(super) fn new() -> Self {
        let area = gtk4::DrawingArea::new();
        area.add_css_class(WAVEFORM_CSS_CLASS);
        area.set_hexpand(true);
        area.set_content_height(CONTENT_HEIGHT);
        area.set_valign(gtk4::Align::Center);

        let state = Rc::new(RefCell::new(State {
            peaks: Vec::new(),
            fraction: 0.0,
        }));
        let on_seek: SeekCallback = Rc::new(RefCell::new(None));

        area.set_draw_func({
            let state = state.clone();
            move |area, cr, width, height| draw(area, cr, width, height, &state.borrow())
        });

        // Click-to-seek only: a press jumps playback to that position. There is
        // no drag-scrub, so — unlike GtkScale — there is no tick-vs-drag guard
        // to maintain; position ticks always update the drawn fraction.
        let click = gtk4::GestureClick::new();
        click.connect_pressed({
            let state = state.clone();
            let on_seek = on_seek.clone();
            let area = area.clone();
            move |_, _, x, _| seek_to(&area, &state, &on_seek, x)
        });
        area.add_controller(click);

        Self {
            area,
            state,
            on_seek,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::DrawingArea {
        &self.area
    }

    pub(super) fn set_peaks(&self, peaks: Vec<f32>) {
        self.state.borrow_mut().peaks = peaks;
        self.area.queue_draw();
    }

    pub(super) fn set_fraction(&self, fraction: f64) {
        self.state.borrow_mut().fraction = fraction.clamp(0.0, 1.0);
        self.area.queue_draw();
    }

    pub(super) fn connect_seek(&self, callback: impl Fn(f64) + 'static) {
        *self.on_seek.borrow_mut() = Some(Rc::new(callback));
    }
}

fn seek_to(area: &gtk4::DrawingArea, state: &Rc<RefCell<State>>, on_seek: &SeekCallback, x: f64) {
    let fraction = fraction_at(x, f64::from(area.width()));
    state.borrow_mut().fraction = fraction; // optimistic until the tick confirms
    area.queue_draw();
    // Clone the callback out before invoking so the RefCell borrow is dropped
    // (the handler may re-enter through a position tick).
    let callback = on_seek.borrow().clone();
    if let Some(callback) = callback {
        callback(fraction);
    }
}

fn draw(
    area: &gtk4::DrawingArea,
    cr: &gtk4::cairo::Context,
    width: i32,
    height: i32,
    state: &State,
) {
    let count = state.peaks.len();
    if count == 0 || width <= 0 || height <= 0 {
        return;
    }
    let color = area.color();
    let (r, g, b, a) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()),
    );
    let w = f64::from(width);
    let h = f64::from(height);
    let slot = w / count as f64;
    let bar_w = slot * (1.0 - BAR_GAP_FRACTION);
    for (index, &peak) in state.peaks.iter().enumerate() {
        let magnitude = f64::from(peak).clamp(0.0, 1.0).max(MIN_BAR_HEIGHT_FRACTION);
        let bar_h = magnitude * h;
        let x = index as f64 * slot + (slot - bar_w) / 2.0;
        let y = (h - bar_h) / 2.0;
        let alpha = if bar_played(index, count, state.fraction) {
            a
        } else {
            a * UNPLAYED_ALPHA
        };
        cr.set_source_rgba(r, g, b, alpha);
        cr.rectangle(x, y, bar_w, bar_h);
        let _ = cr.fill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_maps_and_clamps_to_unit_range() {
        assert_eq!(fraction_at(0.0, 200.0), 0.0);
        assert_eq!(fraction_at(100.0, 200.0), 0.5);
        assert_eq!(fraction_at(200.0, 200.0), 1.0);
        assert_eq!(fraction_at(260.0, 200.0), 1.0);
        assert_eq!(fraction_at(50.0, 0.0), 0.0);
    }

    #[test]
    fn bars_split_played_from_unplayed_at_the_fraction() {
        // 4 bars, centres at 0.125/0.375/0.625/0.875; fraction 0.5 plays first 2.
        assert!(bar_played(0, 4, 0.5));
        assert!(bar_played(1, 4, 0.5));
        assert!(!bar_played(2, 4, 0.5));
        assert!(!bar_played(3, 4, 0.5));
        assert!(!bar_played(0, 0, 1.0));
    }
}
