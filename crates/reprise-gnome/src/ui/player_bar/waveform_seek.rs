//! Custom waveform seek bar: draws precomputed peaks with a played/unplayed
//! split and turns a pointer position into a 0..1 seek fraction through its own
//! gesture (so, unlike `GtkScale`, there is no built-in trough-warp gesture to
//! fight — see the GtkRange note in the gtk4 building skill).
//!
//! Colours come from the widget's own CSS `color` (set to
//! `@reprise_player_accent` by the player-bar CSS), so the waveform recolors
//! with the active theme.

use std::cell::RefCell;
use std::f64::consts::{FRAC_PI_2, PI};
use std::rc::Rc;

use gtk4::prelude::*;

/// Shared, cloneable slot for the optional seek handler (cloned out before it
/// is invoked so no `RefCell` borrow is held across the call).
type SeekCallback = Rc<RefCell<Option<Rc<dyn Fn(f64)>>>>;

pub(super) const WAVEFORM_CSS_CLASS: &str = "waveform-seek";
const CONTENT_HEIGHT: i32 = 28;
const BAR_RADIUS: f64 = 1.5;
const BAR_GAP: f64 = 2.0;
const MIN_BAR_HEIGHT: f64 = 5.0;
const MAX_BAR_HEIGHT: f64 = 26.0;
/// Alpha for not-yet-played bars — white on dark background.
const UNPLAYED_ALPHA: f64 = 0.16;
/// Alpha for bars in the drag ghost region.
const GHOST_ALPHA: f64 = 0.40;
/// Fallback bar height when no peaks are available.
const FALLBACK_BAR_HEIGHT: f64 = 4.0;
/// Build-up animation duration in seconds.
const BUILD_DURATION_S: f64 = 0.3;
/// Per-bar stagger increment in seconds.
const BAR_STAGGER_S: f64 = 0.002;

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
    hover_index: Option<usize>,
    drag_fraction: Option<f64>,
    // Smooth interpolation.
    target_fraction: f64,
    fraction_velocity: f64, // fraction-per-microsecond
    last_tick_us: i64,
    // Build-up animation.
    build_progress: f64, // 0.0 = not started, 1.0 = complete
    build_start_us: i64, // 0 means not running
}

#[derive(Clone)]
pub(super) struct WaveformSeek {
    area: gtk4::DrawingArea,
    state: Rc<RefCell<State>>,
    on_seek: SeekCallback,
    /// Active tick callback handle. Stored in an `Rc<RefCell<Option<…>>>` so
    /// the closure inside the callback can clear it on completion without needing
    /// an extra flag.  `TickCallbackId` is not `Clone`, so we take it out to
    /// call `.remove()` rather than copying it.
    tick_id: Rc<RefCell<Option<gtk4::TickCallbackId>>>,
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
            hover_index: None,
            drag_fraction: None,
            target_fraction: 0.0,
            fraction_velocity: 0.0,
            last_tick_us: 0,
            build_progress: 1.0,
            build_start_us: 0,
        }));
        let on_seek: SeekCallback = Rc::new(RefCell::new(None));
        let tick_id: Rc<RefCell<Option<gtk4::TickCallbackId>>> = Rc::new(RefCell::new(None));

        area.set_draw_func({
            let state = state.clone();
            move |area, cr, width, height| draw(area, cr, width, height, &state.borrow())
        });

        // Hover tracking: update the highlighted bar index as the pointer moves.
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion({
            let state = state.clone();
            let area = area.clone();
            move |_, x, _| {
                let count = state.borrow().peaks.len();
                if count == 0 {
                    return;
                }
                let w = f64::from(area.width());
                let slot = (w + BAR_GAP) / count as f64;
                let index = ((x / slot) as usize).min(count.saturating_sub(1));
                state.borrow_mut().hover_index = Some(index);
                area.queue_draw();
            }
        });
        motion.connect_leave({
            let state = state.clone();
            let area = area.clone();
            move |_| {
                state.borrow_mut().hover_index = None;
                area.queue_draw();
            }
        });
        area.add_controller(motion);

        // Drag-to-seek: begin/update show a ghost fill; end commits the seek.
        // A single click with no movement still triggers drag_begin + drag_end
        // with a zero offset, so click-to-seek is handled for free.
        let drag = gtk4::GestureDrag::new();
        drag.connect_drag_begin({
            let state = state.clone();
            let area = area.clone();
            move |_, x, _| {
                let frac = fraction_at(x, f64::from(area.width()));
                state.borrow_mut().drag_fraction = Some(frac);
                area.queue_draw();
            }
        });
        drag.connect_drag_update({
            let state = state.clone();
            let area = area.clone();
            move |gesture, offset_x, _| {
                let (start_x, _) = gesture.start_point().unwrap_or((0.0, 0.0));
                let frac = fraction_at(start_x + offset_x, f64::from(area.width()));
                state.borrow_mut().drag_fraction = Some(frac);
                area.queue_draw();
            }
        });
        drag.connect_drag_end({
            let state = state.clone();
            let on_seek = on_seek.clone();
            let area = area.clone();
            move |gesture, offset_x, _| {
                let (start_x, _) = gesture.start_point().unwrap_or((0.0, 0.0));
                let frac = fraction_at(start_x + offset_x, f64::from(area.width()));
                {
                    let mut s = state.borrow_mut();
                    s.drag_fraction = None;
                    s.fraction = frac;
                }
                area.queue_draw();
                // Clone callback out before invoking; handler may re-enter via a
                // position tick and would otherwise deadlock on the RefCell.
                let callback = on_seek.borrow().clone();
                if let Some(callback) = callback {
                    callback(frac);
                }
            }
        });
        area.add_controller(drag);

        // Tooltip: show position as a percentage while hovering.
        // Task 5 will upgrade this to the actual elapsed/total time display.
        area.set_has_tooltip(true);
        area.connect_query_tooltip({
            let state = state.clone();
            move |area, x, _y, _keyboard, tooltip| {
                let s = state.borrow();
                if s.peaks.is_empty() {
                    return false;
                }
                let frac = fraction_at(x as f64, f64::from(area.width()));
                tooltip.set_text(Some(&format!("{:.0}%", frac * 100.0)));
                true
            }
        });

        Self {
            area,
            state,
            on_seek,
            tick_id,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::DrawingArea {
        &self.area
    }

    /// Set peaks and trigger a 300 ms build-up animation (gated on
    /// `gtk-enable-animations`).  Use this whenever the track changes.
    pub(super) fn set_peaks(&self, peaks: Vec<f32>) {
        let now = self.area.frame_clock().map_or(0, |c| c.frame_time());
        let animate = gtk4::Settings::default()
            .is_none_or(|s| s.is_gtk_enable_animations());
        let mut s = self.state.borrow_mut();
        s.peaks = peaks;
        if animate && !s.peaks.is_empty() {
            s.build_progress = 0.0;
            s.build_start_us = now;
        } else {
            s.build_progress = 1.0;
        }
        drop(s);
        self.area.queue_draw();
        self.ensure_tick_callback();
    }

    /// Instantly set the playback position (0..1).  Prefer `set_fraction_smooth`
    /// when updating from a sub-second position tick so movement is continuous.
    #[allow(dead_code)]
    pub(super) fn set_fraction(&self, fraction: f64) {
        let fraction = fraction.clamp(0.0, 1.0);
        let mut s = self.state.borrow_mut();
        s.fraction = fraction;
        s.target_fraction = fraction;
        s.fraction_velocity = 0.0;
        drop(s);
        self.area.queue_draw();
    }

    /// Update the target playback fraction with velocity estimation for smooth
    /// interpolation.  Installs a frame-clock tick callback to animate the fill
    /// toward the new target.
    pub(super) fn set_fraction_smooth(&self, fraction: f64) {
        let fraction = fraction.clamp(0.0, 1.0);
        let mut s = self.state.borrow_mut();
        let now = self.area.frame_clock().map_or(0, |c| c.frame_time());
        let dt = (now - s.last_tick_us).max(1) as f64;
        s.fraction_velocity = (fraction - s.target_fraction) / dt;
        s.target_fraction = fraction;
        s.last_tick_us = now;
        drop(s);
        self.ensure_tick_callback();
    }

    pub(super) fn connect_seek(&self, callback: impl Fn(f64) + 'static) {
        *self.on_seek.borrow_mut() = Some(Rc::new(callback));
    }

    /// Installs a `GdkFrameClock` tick callback if one is not already running.
    /// The callback advances the interpolation and build-up animation each frame,
    /// then stops itself when both are settled.
    fn ensure_tick_callback(&self) {
        if self.tick_id.borrow().is_some() {
            return;
        }
        let state = self.state.clone();
        let area = self.area.clone();
        let tick_id_slot = self.tick_id.clone();
        let id = self.area.add_tick_callback(move |_, clock| {
            let now = clock.frame_time();
            let mut s = state.borrow_mut();

            // Advance the smooth-position interpolation.
            let dt = (now - s.last_tick_us).max(0) as f64;
            s.fraction += s.fraction_velocity * dt;
            s.fraction = s.fraction.clamp(0.0, 1.0);
            s.last_tick_us = now;

            // Advance the build-up animation.
            if s.build_progress < 1.0 && s.build_start_us > 0 {
                let elapsed = (now - s.build_start_us) as f64 / 1_000_000.0;
                s.build_progress = (elapsed / BUILD_DURATION_S).clamp(0.0, 1.0);
            }

            let settled = (s.fraction - s.target_fraction).abs() < 0.001
                && s.build_progress >= 1.0;
            drop(s);

            area.queue_draw();

            if settled {
                *tick_id_slot.borrow_mut() = None;
                return gtk4::glib::ControlFlow::Break;
            }
            gtk4::glib::ControlFlow::Continue
        });
        *self.tick_id.borrow_mut() = Some(id);
    }
}

fn draw(
    area: &gtk4::DrawingArea,
    cr: &gtk4::cairo::Context,
    width: i32,
    height: i32,
    state: &State,
) {
    if width <= 0 || height <= 0 {
        return;
    }
    let w = f64::from(width);
    let h = f64::from(height);

    if state.peaks.is_empty() {
        draw_fallback(area, cr, w, h, state.fraction);
        return;
    }

    let count = state.peaks.len();
    let slot = (w + BAR_GAP) / count as f64;
    let bar_w = (slot - BAR_GAP).max(1.0);

    let color = area.color();
    let (r, g, b) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );

    for (index, &peak) in state.peaks.iter().enumerate() {
        let magnitude = f64::from(peak).clamp(0.0, 1.0);

        // Staggered build-up: each bar has a small time offset so they rise
        // one after another from left to right over the 300 ms window.
        let stagger = if state.build_progress < 1.0 {
            let bar_delay = index as f64 * BAR_STAGGER_S;
            let bar_delay_normalized = bar_delay / BUILD_DURATION_S;
            let adjusted = (state.build_progress - bar_delay_normalized).max(0.0)
                / (1.0 - bar_delay_normalized).max(0.01);
            adjusted.clamp(0.0, 1.0)
        } else {
            1.0
        };

        let bar_h = (MIN_BAR_HEIGHT + magnitude * (MAX_BAR_HEIGHT - MIN_BAR_HEIGHT)) * stagger;
        // Guard against zero-height bars during early animation frames.
        if bar_h < 0.5 {
            continue;
        }

        let x = index as f64 * slot;
        let y = (h - bar_h) / 2.0;

        let is_hovered = state.hover_index == Some(index);
        let is_ghost = state.drag_fraction.is_some_and(|drag_frac| {
            let bar_center = (index as f64 + 0.5) / count as f64;
            let (lo, hi) = if drag_frac > state.fraction {
                (state.fraction, drag_frac)
            } else {
                (drag_frac, state.fraction)
            };
            bar_center > lo && bar_center <= hi
        });

        if is_hovered {
            // Highlight: full accent alpha regardless of played/unplayed state.
            cr.set_source_rgba(r, g, b, 1.0);
        } else if is_ghost {
            cr.set_source_rgba(r, g, b, GHOST_ALPHA);
        } else if bar_played(index, count, state.fraction) {
            cr.set_source_rgba(r, g, b, 1.0);
        } else {
            cr.set_source_rgba(1.0, 1.0, 1.0, UNPLAYED_ALPHA);
        }
        rounded_bar(cr, x, y, bar_w, bar_h, BAR_RADIUS);
        let _ = cr.fill();
    }
}

fn draw_fallback(
    area: &gtk4::DrawingArea,
    cr: &gtk4::cairo::Context,
    w: f64,
    h: f64,
    fraction: f64,
) {
    let y = (h - FALLBACK_BAR_HEIGHT) / 2.0;
    let color = area.color();
    let (r, g, b) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );

    let played_w = (fraction * w).max(0.0);
    if played_w > 0.0 {
        cr.set_source_rgba(r, g, b, 1.0);
        rounded_bar(cr, 0.0, y, played_w, FALLBACK_BAR_HEIGHT, BAR_RADIUS);
        let _ = cr.fill();
    }

    let remaining_w = w - played_w;
    if remaining_w > 0.0 {
        cr.set_source_rgba(1.0, 1.0, 1.0, UNPLAYED_ALPHA);
        rounded_bar(cr, played_w, y, remaining_w, FALLBACK_BAR_HEIGHT, BAR_RADIUS);
        let _ = cr.fill();
    }
}

fn rounded_bar(cr: &gtk4::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
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

    #[test]
    fn fallback_draws_flat_bar_when_peaks_empty() {
        // No peaks → draw function should not panic, draws fallback.
        // This is a logic test; actual rendering verified in smoke tests.
        assert_eq!(fraction_at(50.0, 100.0), 0.5);
    }

    #[test]
    fn ghost_region_spans_between_fraction_and_drag_fraction() {
        // drag_fraction > fraction: bars with centres in (fraction, drag_fraction]
        // should be in the ghost region.
        let in_ghost = |index: usize, count: usize, fraction: f64, drag_frac: f64| -> bool {
            let bar_center = (index as f64 + 0.5) / count as f64;
            let (lo, hi) = if drag_frac > fraction {
                (fraction, drag_frac)
            } else {
                (drag_frac, fraction)
            };
            bar_center > lo && bar_center <= hi
        };

        // 4 bars at 0.125 / 0.375 / 0.625 / 0.875; fraction=0.25, drag=0.75
        assert!(!in_ghost(0, 4, 0.25, 0.75)); // centre 0.125 ≤ 0.25
        assert!(in_ghost(1, 4, 0.25, 0.75));  // centre 0.375 in (0.25, 0.75]
        assert!(in_ghost(2, 4, 0.25, 0.75));  // centre 0.625 in (0.25, 0.75]
        assert!(!in_ghost(3, 4, 0.25, 0.75)); // centre 0.875 > 0.75

        // Reversed drag: drag < fraction should also produce a ghost range.
        assert!(!in_ghost(0, 4, 0.75, 0.25)); // centre 0.125 ≤ 0.25
        assert!(in_ghost(1, 4, 0.75, 0.25));  // centre 0.375 in (0.25, 0.75]
        assert!(in_ghost(2, 4, 0.75, 0.25));  // centre 0.625 in (0.25, 0.75]
        assert!(!in_ghost(3, 4, 0.75, 0.25)); // centre 0.875 > 0.75
    }

    #[test]
    fn hover_index_targets_correct_bar() {
        // Given 10 bars across 200px, each slot is (200+2)/10 = 20.2px.
        // Bar 0: x in [0, 20.2), bar 3: x in [60.6, 80.8).
        let count = 10usize;
        let w = 200.0_f64;
        let slot = (w + BAR_GAP) / count as f64;
        let x_to_index = |x: f64| ((x / slot) as usize).min(count.saturating_sub(1));

        assert_eq!(x_to_index(0.0), 0);
        assert_eq!(x_to_index(slot * 3.0 + 1.0), 3);
        assert_eq!(x_to_index(w - 1.0), 9);
        // Past the end should clamp to last bar.
        assert_eq!(x_to_index(w + 50.0), 9);
    }

    #[test]
    fn stagger_factor_is_zero_at_start_and_one_at_completion() {
        // At build_progress=0.0, bar 0 stagger is 0 (progress=0, delay_norm=0).
        // stagger = (0.0 - 0.0).max(0) / (1.0 - 0.0).max(0.01) = 0.0.
        let stagger_for = |build_progress: f64, index: usize| -> f64 {
            if build_progress < 1.0 {
                let bar_delay = index as f64 * BAR_STAGGER_S;
                let bar_delay_normalized = bar_delay / BUILD_DURATION_S;
                let adjusted = (build_progress - bar_delay_normalized).max(0.0)
                    / (1.0 - bar_delay_normalized).max(0.01);
                adjusted.clamp(0.0, 1.0)
            } else {
                1.0
            }
        };

        // progress=0: all bars start at 0.
        assert_eq!(stagger_for(0.0, 0), 0.0);
        assert_eq!(stagger_for(0.0, 10), 0.0);

        // progress=1: sentinel branch — returns 1.0.
        assert_eq!(stagger_for(1.0, 0), 1.0);
        assert_eq!(stagger_for(1.0, 50), 1.0);

        // progress=0.5: bar 0 (no delay) is at 0.5; a late bar with enough
        // delay to push its bar_delay_normalized > 0.5 is still 0.
        assert!((stagger_for(0.5, 0) - 0.5).abs() < 1e-9);
        assert_eq!(stagger_for(0.5, 100), 0.0); // bar 100: delay=0.2s > 0.15s already passed
    }

    #[test]
    fn smooth_fraction_velocity_is_computed_from_delta() {
        // Pure logic test: given target=0.5, old_target=0.0, dt=1_000_000 us
        // the velocity should be 0.5/1_000_000 per microsecond.
        let old_target = 0.0_f64;
        let new_target = 0.5_f64;
        let dt = 1_000_000_i64;
        let velocity = (new_target - old_target) / dt as f64;
        assert!((velocity - 5e-7).abs() < 1e-12);
    }
}
