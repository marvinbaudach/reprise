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
use libadwaita::prelude::AnimationExt;

#[cfg(test)]
use super::waveform_shape::{aggregate_rms, smooth_neighbors};
use super::waveform_shape::{shape_display_peaks, DisplayBar, SILENCE_DOT_HEIGHT};
use crate::ui::motion;
use crate::ui::style::cover_accent::scale_chroma;
use reprise_core::format::format_duration;

/// Shared, cloneable slot for the optional seek handler (cloned out before it
/// is invoked so no `RefCell` borrow is held across the call).
type SeekCallback = Rc<RefCell<Option<Rc<dyn Fn(f64)>>>>;

pub(in crate::ui) const WAVEFORM_CSS_CLASS: &str = "waveform-seek";
const CONTENT_HEIGHT: i32 = 28;
/// Fixed bar width; the count varies with the widget width instead.
const BAR_WIDTH: f64 = 3.0;
/// Rounded caps: radius = half the bar width.
const BAR_RADIUS: f64 = BAR_WIDTH / 2.0;
const BAR_GAP: f64 = 2.0;
/// Hard cap on displayed bars — beyond this the waveform reads as noise.
const MAX_BAR_COUNT: usize = 160;
/// Audible bars span 15%..100% of the max bar height.
const MIN_BAR_HEIGHT: f64 = MAX_BAR_HEIGHT * 0.15;
const MAX_BAR_HEIGHT: f64 = 26.0;
/// Alpha for not-yet-played bars — white on dark background, deliberately
/// receding so the played (accent) part dominates.
const UNPLAYED_ALPHA: f64 = 0.18;
/// Alpha for unplayed bars between the playhead and the hovered position —
/// the seek preview.
const HOVER_PREVIEW_ALPHA: f64 = 0.30;
/// Alpha of the 1 px playhead line drawn over the bars.
const PLAYHEAD_ALPHA: f64 = 0.70;
/// Alpha for bars in the drag ghost region.
const GHOST_ALPHA: f64 = 0.40;
/// Ambient build-up animation duration in seconds.
const BUILD_DURATION_S: f64 = motion::AMBIENT_MS as f64 / 1_000.0;
/// Track-change alpha crossfade duration in seconds.
const CROSSFADE_DURATION_S: f64 = motion::AMBIENT_MS as f64 / 1_000.0;
/// Per-bar stagger increment in seconds.
const BAR_STAGGER_S: f64 = 0.002;

const FALLBACK_BAR_HEIGHT: f64 = 4.0;

const MINI_CONTENT_HEIGHT: i32 = 16;
const MINI_MAX_BAR_HEIGHT: f64 = 13.0;
const MINI_MIN_BAR_HEIGHT: f64 = 2.0;
const MINI_FALLBACK_BAR_HEIGHT: f64 = 3.0;

/// Advances the smooth-fill interpolation by one frame: `fraction` moves by
/// `velocity * dt_us` but never past `target` — the interpolation chases the
/// most recent position tick, so overshooting it is always wrong. This bound
/// is what makes a mis-measured `dt` (and thus an exploded velocity)
/// harmless: the worst case degrades to snapping straight to the target
/// instead of pinning the fill at 100% for the rest of the song. A fraction
/// that is already past the target (a stale stuck state) snaps back to it
/// for the same reason. Result stays in 0..1.
fn interpolation_step(fraction: f64, velocity: f64, dt_us: f64, target: f64) -> f64 {
    let advanced = velocity.mul_add(dt_us, fraction);
    let bounded = if velocity >= 0.0 {
        advanced.min(target)
    } else {
        advanced.max(target)
    };
    bounded.clamp(0.0, 1.0)
}

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

/// Number of display bars for `width` pixels: fixed 3 px bars + 2 px gaps,
/// hard-capped at [`MAX_BAR_COUNT`] (when capped, the slots widen instead).
fn compute_bar_count(width: i32) -> usize {
    ((f64::from(width) / (BAR_WIDTH + BAR_GAP)).floor() as usize).clamp(1, MAX_BAR_COUNT)
}

/// Ensure `state.display_peaks` is up to date for the given `width`.
/// Re-aggregates from the cached `raw_peaks` (never re-decodes) when the
/// width changed or the cache is empty.
fn ensure_resampled(state: &mut State, width: i32) {
    if state.last_display_width != 0
        && state.last_display_width != width
        && state.crossfade_progress < 1.0
    {
        state.previous_bars.clear();
        state.crossfade_progress = 1.0;
        state.crossfade_start_us = 0;
    }
    if state.raw_peaks.is_empty() {
        state.display_peaks.clear();
        return;
    }
    if state.last_display_width != width || state.display_peaks.is_empty() {
        let count = compute_bar_count(width);
        state.display_peaks = shape_display_peaks(&state.raw_peaks, count);
        state.last_display_width = width;
    }
}

struct State {
    raw_peaks: Vec<u8>,             // stored peaks from DB (1000 values, 0-255)
    display_peaks: Vec<DisplayBar>, // shaped to current bar count
    last_display_width: i32,        // width used for last resample
    fraction: f64,
    /// Pointer position as a 0..1 fraction while hovering — drives the
    /// seek-preview tint on unplayed bars up to the cursor.
    hover_fraction: Option<f64>,
    drag_fraction: Option<f64>,
    // Smooth interpolation.
    target_fraction: f64,
    fraction_velocity: f64, // fraction-per-microsecond
    last_tick_us: i64,
    // Build-up animation.
    build_progress: f64, // 0.0 = not started, 1.0 = complete
    build_start_us: i64, // 0 means not running
    // Track-change alpha crossfade.
    previous_bars: Vec<DisplayBar>,
    crossfade_progress: f64, // 1.0 means no crossfade is running
    crossfade_start_us: i64,
    // Pause desaturation animation.
    desaturation_progress: f64, // 0.0 = full chroma, 1.0 = paused chroma
    #[allow(dead_code)] // Consumed by the PlayerBar/Compact wiring in MOT-5 Phase B.
    desaturation_target: f64,
    min_bar_height: f64,
    max_bar_height: f64,
    // Duration of the current track (ms), for formatted tooltip display.
    duration_ms: i64,
}

#[derive(Clone)]
pub(in crate::ui) struct WaveformSeek {
    area: gtk4::DrawingArea,
    state: Rc<RefCell<State>>,
    on_seek: SeekCallback,
    /// Active tick callback handle. Stored in an `Rc<RefCell<Option<…>>>` so
    /// the closure inside the callback can clear it on completion without needing
    /// an extra flag.  `TickCallbackId` is not `Clone`, so we take it out to
    /// call `.remove()` rather than copying it.
    tick_id: Rc<RefCell<Option<gtk4::TickCallbackId>>>,
    /// Active pause-desaturation animation. Replacements skip the previous
    /// visual state before starting from its settled endpoint.
    #[allow(dead_code)] // Consumed by the PlayerBar/Compact wiring in MOT-5 Phase B.
    desaturation_animation: Rc<RefCell<Option<libadwaita::TimedAnimation>>>,
}

impl WaveformSeek {
    pub(in crate::ui) fn new() -> Self {
        Self::new_with_heights(
            CONTENT_HEIGHT,
            MAX_BAR_HEIGHT,
            MIN_BAR_HEIGHT,
            FALLBACK_BAR_HEIGHT,
        )
    }

    pub(in crate::ui) fn new_mini() -> Self {
        Self::new_with_heights(
            MINI_CONTENT_HEIGHT,
            MINI_MAX_BAR_HEIGHT,
            MINI_MIN_BAR_HEIGHT,
            MINI_FALLBACK_BAR_HEIGHT,
        )
    }

    fn new_with_heights(content_height: i32, max_h: f64, min_h: f64, _fallback_h: f64) -> Self {
        let area = gtk4::DrawingArea::new();
        area.add_css_class(WAVEFORM_CSS_CLASS);
        area.set_hexpand(true);
        area.set_content_height(content_height);
        area.set_valign(gtk4::Align::Center);

        let state = Rc::new(RefCell::new(State {
            raw_peaks: Vec::new(),
            display_peaks: Vec::new(),
            last_display_width: 0,
            fraction: 0.0,
            hover_fraction: None,
            drag_fraction: None,
            target_fraction: 0.0,
            fraction_velocity: 0.0,
            last_tick_us: 0,
            build_progress: 1.0,
            build_start_us: 0,
            previous_bars: Vec::new(),
            crossfade_progress: 1.0,
            crossfade_start_us: 0,
            desaturation_progress: 0.0,
            desaturation_target: 0.0,
            min_bar_height: min_h,
            max_bar_height: max_h,
            duration_ms: 0,
        }));
        let on_seek: SeekCallback = Rc::new(RefCell::new(None));
        let tick_id: Rc<RefCell<Option<gtk4::TickCallbackId>>> = Rc::new(RefCell::new(None));
        let desaturation_animation = Rc::new(RefCell::new(None));

        area.set_draw_func({
            let state = state.clone();
            move |area, cr, width, height| {
                let mut s = state.borrow_mut();
                ensure_resampled(&mut s, width);
                draw(area, cr, width, height, &s);
            }
        });

        // Hover tracking: remember the pointer position as a fraction so the
        // draw pass can tint unplayed bars up to it (seek preview).
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion({
            let state = state.clone();
            let area = area.clone();
            move |_, x, _| {
                if state.borrow().display_peaks.is_empty() {
                    return;
                }
                let frac = fraction_at(x, f64::from(area.width()));
                state.borrow_mut().hover_fraction = Some(frac);
                area.queue_draw();
            }
        });
        motion.connect_leave({
            let state = state.clone();
            let area = area.clone();
            move |_| {
                state.borrow_mut().hover_fraction = None;
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
                    s.target_fraction = frac;
                    s.fraction_velocity = 0.0;
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

        // Tooltip: show the formatted time at the hovered position.
        area.set_has_tooltip(true);
        area.connect_query_tooltip({
            let state = state.clone();
            move |area, x, _y, _keyboard, tooltip| {
                let s = state.borrow();
                if s.raw_peaks.is_empty() {
                    return false;
                }
                let frac = fraction_at(x as f64, f64::from(area.width()));
                let text = if s.duration_ms > 0 {
                    let position_ms = (frac * s.duration_ms as f64).round() as i64;
                    format_duration(position_ms)
                } else {
                    format!("{:.0}%", frac * 100.0)
                };
                tooltip.set_text(Some(&text));
                true
            }
        });

        Self {
            area,
            state,
            on_seek,
            tick_id,
            desaturation_animation,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::DrawingArea {
        &self.area
    }

    /// Set peaks (as raw `u8` values, 0-255) and trigger an Ambient build-up
    /// animation (gated on `gtk-enable-animations`). Use this whenever the
    /// track changes.
    pub(in crate::ui) fn set_peaks(&self, peaks: Vec<u8>) {
        let build_start_us = self.area.frame_clock().map_or(0, |c| c.frame_time());
        let crossfade_start_us = gtk4::glib::monotonic_time();
        let animate = motion::animations_enabled();
        if !animate {
            let existing_tick = self.tick_id.borrow_mut().take();
            if let Some(existing_tick) = existing_tick {
                existing_tick.remove();
            }
        }
        let mut s = self.state.borrow_mut();
        // Bars currently on screen to fade *from*: the resolved bars, or — if a
        // crossfade is still mid-flight because no draw has resampled yet — the
        // ones it is already fading from (two `set_peaks` with no draw between
        // must not lose that source and rebuild). Empty incoming peaks arm no
        // crossfade: draw() takes the fallback, so its tick would show nothing.
        let crossfade_in_flight = s.crossfade_progress < 1.0 && !s.previous_bars.is_empty();
        let has_visible_bars = !s.display_peaks.is_empty() || crossfade_in_flight;
        if animate && has_visible_bars && !peaks.is_empty() {
            // When display_peaks is empty, keep the in-flight previous_bars.
            if !s.display_peaks.is_empty() {
                s.previous_bars = std::mem::take(&mut s.display_peaks);
            }
            s.crossfade_progress = 0.0;
            s.crossfade_start_us = crossfade_start_us;
            s.build_progress = 1.0;
            s.build_start_us = 0;
        } else {
            s.previous_bars.clear();
            s.crossfade_progress = 1.0;
            s.crossfade_start_us = 0;
            if animate && !peaks.is_empty() {
                s.build_progress = 0.0;
                s.build_start_us = build_start_us;
            } else {
                s.build_progress = 1.0;
                s.build_start_us = 0;
            }
        }
        s.raw_peaks = peaks;
        s.display_peaks.clear();
        if !animate {
            s.fraction = s.target_fraction;
            s.fraction_velocity = 0.0;
        }
        let should_tick = s.build_progress < 1.0 || s.crossfade_progress < 1.0;
        drop(s);
        self.area.queue_draw();
        if should_tick {
            self.ensure_tick_callback();
        }
    }

    /// Animates the local waveform fill toward the paused or playing chroma.
    /// This never mutates the application-wide cover-accent provider.
    #[allow(dead_code)] // Wired from PlayerBar and Compact only after the Phase-B gate opens.
    pub(in crate::ui) fn set_paused(&self, paused: bool) {
        let target = if paused { 1.0 } else { 0.0 };
        if self.state.borrow().desaturation_target == target {
            return;
        }

        if !motion::animations_enabled() {
            let previous = self.desaturation_animation.borrow_mut().take();
            if let Some(previous) = previous {
                previous.skip();
            }
            let mut state = self.state.borrow_mut();
            state.desaturation_progress = target;
            state.desaturation_target = target;
            drop(state);
            self.area.queue_draw();
            return;
        }

        // Start from the current interpolated value (read before the skip
        // below overwrites it), so a fast Pause→Play reversal glides from
        // mid-flight instead of snapping to the old target and flashing grey.
        let from = self.state.borrow().desaturation_progress;
        self.state.borrow_mut().desaturation_target = target;
        let state = self.state.clone();
        let area = self.area.clone();
        let animation_target = libadwaita::CallbackAnimationTarget::new(move |value| {
            state.borrow_mut().desaturation_progress = value;
            area.queue_draw();
        });
        let animation = motion::timed(&self.area, from, target, motion::STANDARD, animation_target);
        motion::replace_animation(&self.desaturation_animation, animation.clone());
        animation.play();
    }

    /// Instantly set the playback position (0..1).  Prefer `set_fraction_smooth`
    /// when updating from a sub-second position tick so movement is continuous.
    #[allow(dead_code)]
    pub(in crate::ui) fn set_fraction(&self, fraction: f64) {
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
    ///
    /// Large jumps (> 5% of the track) are treated as seeks: the fraction snaps
    /// instantly and velocity resets, preventing the overshoot that occurs when
    /// a stale pre-seek position tick arrives before the post-seek position.
    pub(in crate::ui) fn set_fraction_smooth(&self, fraction: f64) {
        let fraction = fraction.clamp(0.0, 1.0);
        if !motion::animations_enabled() {
            let existing_tick = self.tick_id.borrow_mut().take();
            if let Some(existing_tick) = existing_tick {
                existing_tick.remove();
            }
            // Complete any in-progress build-up: the tick that would have
            // advanced it was just removed, so without this the waveform would
            // freeze half-built if animations are disabled mid-build. Mirrors
            // the disabled branch of `set_peaks`.
            {
                let mut state = self.state.borrow_mut();
                state.build_progress = 1.0;
                state.build_start_us = 0;
                state.previous_bars.clear();
                state.crossfade_progress = 1.0;
                state.crossfade_start_us = 0;
            }
            self.set_fraction(fraction);
            return;
        }
        let mut s = self.state.borrow_mut();
        // Real monotonic time, NOT `frame_clock().frame_time()`: the frame
        // clock only advances while frames are being produced, so two
        // position ticks arriving between frames used to read the same
        // stale timestamp — `dt` collapsed to 1 µs, the velocity exploded,
        // and the next real frame pinned the fill at 100% (the stuck-full
        // bar bug). `frame_time` shares `g_get_monotonic_time`'s timescale,
        // so mixing the two sources in `last_tick_us` is safe.
        let now = gtk4::glib::monotonic_time();
        let delta = (fraction - s.target_fraction).abs();
        if delta > 0.05 || s.last_tick_us == 0 {
            // Large discontinuity, seek, or no valid time reference yet — snap.
            s.fraction = fraction;
            s.target_fraction = fraction;
            s.fraction_velocity = 0.0;
            s.last_tick_us = now;
        } else {
            let dt = (now - s.last_tick_us).max(1) as f64;
            s.fraction_velocity = (fraction - s.target_fraction) / dt;
            s.target_fraction = fraction;
            s.last_tick_us = now;
        }
        drop(s);
        self.ensure_tick_callback();
    }

    /// Set the track duration so the hover tooltip can show formatted time
    /// instead of a raw percentage.
    pub(in crate::ui) fn set_duration(&self, duration_ms: i64) {
        self.state.borrow_mut().duration_ms = duration_ms.max(0);
    }

    pub(in crate::ui) fn connect_seek(&self, callback: impl Fn(f64) + 'static) {
        *self.on_seek.borrow_mut() = Some(Rc::new(callback));
    }

    /// Installs a `GdkFrameClock` tick callback if one is not already running.
    /// The callback advances interpolation, build-up, and track crossfade each
    /// frame, then stops itself when all three are settled.
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

            if motion::animations_enabled() {
                // Advance the smooth-position interpolation (never past the
                // target — see `interpolation_step`).
                let dt = (now - s.last_tick_us).max(0) as f64;
                s.fraction =
                    interpolation_step(s.fraction, s.fraction_velocity, dt, s.target_fraction);
                s.last_tick_us = now;

                // Advance the build-up animation.
                if s.build_progress < 1.0 && s.build_start_us > 0 {
                    let elapsed = (now - s.build_start_us) as f64 / 1_000_000.0;
                    s.build_progress = (elapsed / BUILD_DURATION_S).clamp(0.0, 1.0);
                }

                if s.crossfade_progress < 1.0 && s.crossfade_start_us > 0 {
                    let elapsed = (now - s.crossfade_start_us) as f64 / 1_000_000.0;
                    s.crossfade_progress = (elapsed / CROSSFADE_DURATION_S).clamp(0.0, 1.0);
                    if s.crossfade_progress >= 1.0 {
                        s.previous_bars.clear();
                        s.crossfade_start_us = 0;
                    }
                }
            } else {
                s.fraction = s.target_fraction;
                s.fraction_velocity = 0.0;
                s.build_progress = 1.0;
                s.build_start_us = 0;
                s.previous_bars.clear();
                s.crossfade_progress = 1.0;
                s.crossfade_start_us = 0;
            }

            let settled = (s.fraction - s.target_fraction).abs() < 0.001
                && s.build_progress >= 1.0
                && s.crossfade_progress >= 1.0;
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

    if state.display_peaks.is_empty() {
        draw_fallback(area, cr, w, h, state);
        return;
    }

    let color = area.color();
    let color = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );
    let chroma_factor = 1.0 - 0.55 * state.desaturation_progress;
    let (r, g, b) = scale_chroma(color.0, color.1, color.2, chroma_factor);

    if state.crossfade_progress < 1.0 && !state.previous_bars.is_empty() {
        draw_bars(
            cr,
            w,
            h,
            &state.previous_bars,
            state,
            BarDrawStyle {
                color: (r, g, b),
                build_progress: 1.0,
                opacity: 1.0 - state.crossfade_progress,
            },
        );
        draw_bars(
            cr,
            w,
            h,
            &state.display_peaks,
            state,
            BarDrawStyle {
                color: (r, g, b),
                build_progress: 1.0,
                opacity: state.crossfade_progress,
            },
        );
    } else {
        draw_bars(
            cr,
            w,
            h,
            &state.display_peaks,
            state,
            BarDrawStyle {
                color: (r, g, b),
                build_progress: state.build_progress,
                opacity: 1.0,
            },
        );
    }

    // Playhead: a 1 px line at the exact fraction, drawn over the bars —
    // replaces the old partially-filled boundary bar (the played/unplayed
    // switch is a hard per-bucket cut instead).
    let playhead_x = (state.fraction * w).clamp(0.5, (w - 0.5).max(0.5));
    cr.set_source_rgba(r, g, b, PLAYHEAD_ALPHA);
    cr.rectangle(
        playhead_x - 0.5,
        (h - state.max_bar_height) / 2.0,
        1.0,
        state.max_bar_height,
    );
    let _ = cr.fill();
}

#[derive(Clone, Copy)]
struct BarDrawStyle {
    color: (f64, f64, f64),
    build_progress: f64,
    opacity: f64,
}

fn draw_bars(
    cr: &gtk4::cairo::Context,
    w: f64,
    h: f64,
    bars: &[DisplayBar],
    state: &State,
    style: BarDrawStyle,
) {
    let count = bars.len();
    if count == 0 {
        return;
    }
    // Slots fill the full width so the seek mapping stays linear; when the
    // bar-count cap kicks in the gaps simply widen (bars stay 3 px).
    let slot = w / count as f64;
    let bar_w = BAR_WIDTH.min(slot.max(1.0));

    for (index, &bar) in bars.iter().enumerate() {
        // Staggered build-up: each bar has a small time offset so they rise
        // one after another from left to right over the Ambient window.
        let stagger = if style.build_progress < 1.0 {
            let bar_delay = index as f64 * BAR_STAGGER_S;
            let bar_delay_normalized = bar_delay / BUILD_DURATION_S;
            let adjusted = (style.build_progress - bar_delay_normalized).max(0.0)
                / (1.0 - bar_delay_normalized).max(0.01);
            adjusted.clamp(0.0, 1.0)
        } else {
            1.0
        };

        let bar_h = match bar {
            // True silence: a fixed dot, unaffected by the height mapping.
            DisplayBar::Silence => SILENCE_DOT_HEIGHT * stagger,
            DisplayBar::Level(level) => {
                let magnitude = f64::from(level).clamp(0.0, 1.0);
                (state.min_bar_height + magnitude * (state.max_bar_height - state.min_bar_height))
                    * stagger
            }
        };
        // Guard against zero-height bars during early animation frames.
        if bar_h < 0.5 {
            continue;
        }

        let x = index as f64 * slot + (slot - bar_w) / 2.0;
        let y = (h - bar_h) / 2.0;

        let bar_center = (index as f64 + 0.5) / count as f64;
        let played = bar_played(index, count, state.fraction);
        let is_ghost = state.drag_fraction.is_some_and(|drag_frac| {
            let (lo, hi) = if drag_frac > state.fraction {
                (state.fraction, drag_frac)
            } else {
                (drag_frac, state.fraction)
            };
            bar_center > lo && bar_center <= hi
        });
        // Seek preview: unplayed bars between the playhead and the cursor.
        let is_hover_preview = !played
            && state
                .hover_fraction
                .is_some_and(|hover| bar_center <= hover);

        let (r, g, b) = style.color;
        if is_ghost {
            cr.set_source_rgba(r, g, b, GHOST_ALPHA * style.opacity);
        } else if played {
            cr.set_source_rgba(r, g, b, style.opacity);
        } else if is_hover_preview {
            cr.set_source_rgba(1.0, 1.0, 1.0, HOVER_PREVIEW_ALPHA * style.opacity);
        } else {
            cr.set_source_rgba(1.0, 1.0, 1.0, UNPLAYED_ALPHA * style.opacity);
        }
        rounded_bar(cr, x, y, bar_w, bar_h, BAR_RADIUS);
        let _ = cr.fill();
    }
}

/// Skeleton waveform: deterministic pseudo-random bar heights that look like
/// a plausible waveform while the real peaks are still being computed.
fn draw_fallback(
    area: &gtk4::DrawingArea,
    cr: &gtk4::cairo::Context,
    w: f64,
    h: f64,
    state: &State,
) {
    let count = compute_bar_count(w as i32);
    if count == 0 {
        return;
    }
    let slot = w / count as f64;
    let bar_w = BAR_WIDTH.min(slot.max(1.0));

    let color = area.color();
    let color = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );
    let chroma_factor = 1.0 - 0.55 * state.desaturation_progress;
    let (r, g, b) = scale_chroma(color.0, color.1, color.2, chroma_factor);

    for index in 0..count {
        // Deterministic pseudo-random height using a simple hash.
        let seed = (index as u32).wrapping_mul(2654435761); // Knuth multiplicative hash
        let magnitude = (seed % 200) as f64 / 400.0 + 0.15; // range ~0.15..0.65
        let bar_h =
            state.min_bar_height + magnitude * (state.max_bar_height - state.min_bar_height);
        let x = index as f64 * slot + (slot - bar_w) / 2.0;
        let y = (h - bar_h) / 2.0;

        if bar_played(index, count, state.fraction) {
            cr.set_source_rgba(r, g, b, 0.5);
        } else {
            cr.set_source_rgba(1.0, 1.0, 1.0, UNPLAYED_ALPHA * 0.6);
        }
        rounded_bar(cr, x, y, bar_w, bar_h, BAR_RADIUS);
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
#[path = "waveform_seek_tests.rs"]
mod tests;
