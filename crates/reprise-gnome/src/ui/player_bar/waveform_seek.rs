//! Custom waveform seek bar: draws precomputed peaks with a played/unplayed
//! split and turns a pointer position into a 0..1 seek fraction through its own
//! gesture (so, unlike `GtkScale`, there is no built-in trough-warp gesture to
//! fight — see the GtkRange note in the gtk4 building skill).
//!
//! Colours come from the widget's own CSS `color` (set to
//! `@reprise_player_accent` by the player-bar CSS), so the waveform recolors
//! with the active theme.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita::prelude::AnimationExt;

#[cfg(test)]
use super::waveform_primitives::BAR_GAP;
use super::waveform_primitives::{
    bar_played, bar_slot_width, fraction_at, frame_clock_stalled, keyboard_seek_target,
    position_step, resolve_bar_count, rounded_bar, should_redraw, update_accessible_value,
    velocity_between, RedrawSnapshot, BAR_RADIUS, MINI_BAR_COUNT, MINI_BAR_GAP, MINI_BAR_RADIUS,
};
use super::waveform_shape::{shape_display_peaks, DisplayBar, SILENCE_DOT_HEIGHT};
use crate::ui::motion;
use crate::ui::style::color_math::scale_chroma;
use reprise_core::format::format_duration;
use reprise_core::library::settings::SeekColouring;
use reprise_view::spectral_colour::{
    centroid_at, section_boundaries, shape_centroid, smooth_centroid_over_seconds, smooth_towards,
    spectral_colour, CENTROID_WINDOW_S,
};

/// Shared, cloneable slot for the optional seek handler (cloned out before it
/// is invoked so no `RefCell` borrow is held across the call).
type SeekCallback = Rc<RefCell<Option<Rc<dyn Fn(f64)>>>>;

fn colour_near(a: (f64, f64, f64), b: (f64, f64, f64), threshold: f64) -> bool {
    (a.0 - b.0).abs() < threshold && (a.1 - b.1).abs() < threshold && (a.2 - b.2).abs() < threshold
}

pub(in crate::ui) const WAVEFORM_CSS_CLASS: &str = "waveform-seek";
const CONTENT_HEIGHT: i32 = 28;
/// Audible bars span 15%..100% of the max bar height.
const MIN_BAR_HEIGHT: f64 = MAX_BAR_HEIGHT * 0.15;
const MAX_BAR_HEIGHT: f64 = 26.0;
/// Alpha for not-yet-played bars, which carry the same spectral colour as the
/// played ones: progress is an opacity step, not a change of colour.
///
/// Measured, not chosen: below this the deep-blue stretches of a bass intro
/// disappear against the bar's own background, and above it the played/unplayed
/// boundary stops being readable at a glance.
const UNPLAYED_ALPHA: f64 = 0.34;
/// Alpha for unplayed bars between the playhead and the hovered position —
/// the seek preview. Between the two sides, so the preview reads as "this much
/// would be played" rather than as a third state.
const HOVER_PREVIEW_ALPHA: f64 = 0.62;
/// The coming side of the single-colour bar.
const SOLID_UNPLAYED: (f64, f64, f64) = (
    0x3C as f64 / 255.0,
    0x3F as f64 / 255.0,
    0x44 as f64 / 255.0,
);
/// Its seek preview: the same grey, one step lighter. A dimmed grey would read
/// as further away rather than as nearer.
const SOLID_HOVER_PREVIEW: (f64, f64, f64) = (
    0x5C as f64 / 255.0,
    0x60 as f64 / 255.0,
    0x68 as f64 / 255.0,
);
/// Hairlines at detected section boundaries — the single-colour bar's only
/// remaining hint at where the music changes.
const SECTION_MARK_ALPHA: f64 = 0.30;
const SECTION_MARK_WIDTH: f64 = 1.0;
/// Buffered-but-unplayed remote media, between the coming side and the played
/// one.
///
/// Re-derived, not carried over: the 0.24 this arrived with was picked against
/// an unplayed side of 0.12, and against 0.34 it would sit *below* the very
/// thing it is supposed to be ahead of. It keeps its meaning — visibly more
/// than not-yet-loaded, visibly less than played — on the new scale.
const BUFFERED_ALPHA: f64 = 0.48;
/// Alpha of the rounded playhead drawn over the bars.
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
/// Mini bars span 3px..15px inside the 16px row (frame 1e), vertically centred.
const MINI_MAX_BAR_HEIGHT: f64 = 15.0;
const MINI_MIN_BAR_HEIGHT: f64 = 3.0;
const MINI_FALLBACK_BAR_HEIGHT: f64 = 3.0;

fn commit_seek(
    area: &gtk4::DrawingArea,
    state: &Rc<RefCell<State>>,
    on_seek: &SeekCallback,
    fraction: f64,
) {
    let fraction = fraction.clamp(0.0, 1.0);
    let duration_ms = {
        let mut state = state.borrow_mut();
        state.drag_fraction = None;
        state.fraction = fraction;
        state.target_fraction = fraction;
        state.fraction_velocity = 0.0;
        state.duration_ms
    };
    update_accessible_value(area, fraction, duration_ms);
    area.queue_draw();
    let callback = on_seek.borrow().clone();
    if let Some(callback) = callback {
        callback(fraction);
    }
}

/// Fires on the first press anywhere in the bar, before any seek is committed.
/// The colour-scale legend uses it to get out of the way the moment the user
/// shows they are aiming at the bar rather than reading it.
type PressCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

#[derive(Clone)]
pub(in crate::ui) struct WaveformSeek {
    area: gtk4::DrawingArea,
    state: Rc<RefCell<State>>,
    on_seek: SeekCallback,
    on_press: PressCallback,
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
            None,
            false,
        )
    }

    pub(in crate::ui) fn new_mini() -> Self {
        Self::new_with_heights(
            MINI_CONTENT_HEIGHT,
            MINI_MAX_BAR_HEIGHT,
            MINI_MIN_BAR_HEIGHT,
            MINI_FALLBACK_BAR_HEIGHT,
            Some(MINI_BAR_COUNT),
            true,
        )
    }

    fn new_with_heights(
        content_height: i32,
        max_h: f64,
        min_h: f64,
        _fallback_h: f64,
        bar_count_override: Option<usize>,
        fill_bars: bool,
    ) -> Self {
        let area = gtk4::DrawingArea::new();
        area.add_css_class(WAVEFORM_CSS_CLASS);
        area.set_hexpand(true);
        area.set_content_height(content_height);
        area.set_valign(gtk4::Align::Center);
        // a11y-semantics: role=slider name=playback-position state=value action=range-keys
        area.set_focusable(true);
        area.set_accessible_role(gtk4::AccessibleRole::Slider);
        area.update_property(&[
            gtk4::accessible::Property::Label(&crate::ui::strings::text(
                crate::ui::strings::PLAYBACK_POSITION,
            )),
            gtk4::accessible::Property::KeyShortcuts(
                "ArrowLeft ArrowRight ArrowUp ArrowDown PageUp PageDown Home End",
            ),
        ]);
        update_accessible_value(&area, 0.0, 0);

        let state = Rc::new(RefCell::new(State {
            raw_peaks: Vec::new(),
            raw_centroid: Vec::new(),
            colour_curve: Vec::new(),
            section_marks: Vec::new(),
            colouring: SeekColouring::DEFAULT,
            display_peaks: Vec::new(),
            shaped_centroid: Vec::new(),
            last_display_width: 0,
            fraction: 0.0,
            buffered_fraction: None,
            hover_fraction: None,
            drag_fraction: None,
            target_fraction: 0.0,
            fraction_velocity: 0.0,
            last_position_us: 0,
            last_frame_us: 0,
            build_progress: 1.0,
            build_start_us: 0,
            previous_bars: Vec::new(),
            previous_centroid: Vec::new(),
            crossfade_progress: 1.0,
            crossfade_start_us: 0,
            head_colour_target: None,
            head_colour: None,
            mask_surface: None,
            colour_surface: None,
            surface_key: None,
            last_drawn_head_x: None,
            last_drawn_colour: None,
            last_drawn_hover_fraction: None,
            last_drawn_drag_fraction: None,
            desaturation_progress: 0.0,
            desaturation_target: 0.0,
            min_bar_height: min_h,
            max_bar_height: max_h,
            bar_count_override,
            fill_bars,
            duration_ms: 0,
        }));
        let on_seek: SeekCallback = Rc::new(RefCell::new(None));
        let on_press: PressCallback = Rc::new(RefCell::new(None));
        let tick_id: Rc<RefCell<Option<gtk4::TickCallbackId>>> = Rc::new(RefCell::new(None));
        let desaturation_animation = Rc::new(RefCell::new(None));

        area.set_draw_func({
            let state = state.clone();
            move |area, cr, width, height| {
                let mut s = state.borrow_mut();
                ensure_resampled(&mut s, width);
                render::draw(area, cr, width, height, &mut s);
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
        // input-parity: ACC-8 keyboard=range-keys
        let drag = gtk4::GestureDrag::new();
        drag.connect_drag_begin({
            let state = state.clone();
            let area = area.clone();
            let on_press = on_press.clone();
            move |gesture, x, _| {
                // Cloned out before the call, so no borrow is held across it.
                let pressed = on_press.borrow().clone();
                if let Some(pressed) = pressed {
                    pressed();
                }
                // Claim the sequence on press. In the mini player this widget
                // sits inside the card's GtkWindowHandle, whose own drag
                // gesture claims the sequence once the pointer passes the drag
                // threshold and starts a window move — cancelling this gesture
                // mid-scrub, so the seek committed at the *start* point while
                // the window slid across the desktop. Claiming first keeps the
                // scrub here; the rest of the card stays a drag surface, which
                // is exactly what MINI-2 asks for.
                gesture.set_state(gtk4::EventSequenceState::Claimed);
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
                commit_seek(&area, &state, &on_seek, frac);
            }
        });
        area.add_controller(drag);

        let keys = gtk4::EventControllerKey::new();
        {
            let state = state.clone();
            let on_seek = on_seek.clone();
            let area = area.clone();
            keys.connect_key_pressed(move |_, key, _, _| {
                let (current, duration_ms) = {
                    let state = state.borrow();
                    (state.target_fraction, state.duration_ms)
                };
                let Some(target) = keyboard_seek_target(key, current, duration_ms) else {
                    return gtk4::glib::Propagation::Proceed;
                };
                commit_seek(&area, &state, &on_seek, target);
                gtk4::glib::Propagation::Stop
            });
        }
        area.add_controller(keys);

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
            on_press,
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
        self.set_analysis(peaks, None);
    }

    /// Peaks plus the spectral colour curve derived from the track's stored
    /// spectrogram. A curve whose length does not match the peaks is dropped
    /// rather than trusted — the bar then draws in the plain accent, as it did
    /// before there was a spectral axis at all.
    pub(in crate::ui) fn set_analysis(&self, peaks: Vec<u8>, centroid: Option<Vec<u8>>) {
        // Peaks can arrive before the player bar is mapped, when the drawing
        // area has no frame clock yet. Monotonic time uses the same timescale
        // as `GdkFrameClock::frame_time`, so the first mapped frame can always
        // advance the build instead of leaving every bar at zero height.
        let build_start_us = gtk4::glib::monotonic_time();
        let crossfade_start_us = build_start_us;
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
                s.previous_centroid = std::mem::take(&mut s.shaped_centroid);
            }
            s.crossfade_progress = 0.0;
            s.crossfade_start_us = crossfade_start_us;
            s.build_progress = 1.0;
            s.build_start_us = 0;
        } else {
            s.previous_bars.clear();
            s.previous_centroid.clear();
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
        s.raw_centroid = centroid
            .filter(|curve| curve.len() == peaks.len())
            .unwrap_or_default();
        s.raw_peaks = peaks;
        // Once per track, not once per frame: the averaged curve is what the
        // bars are painted from and it never changes while the track plays.
        // `rebuild_colour_curve` clears the shaped caches and the surfaces,
        // which is what the old inline `display_peaks.clear()` did here.
        rebuild_colour_curve(&mut s);
        if !animate {
            s.fraction = s.target_fraction;
            s.fraction_velocity = 0.0;
        }
        let should_tick = s.build_progress < 1.0
            || s.crossfade_progress < 1.0
            || (!s.colour_curve.is_empty() && motion::animations_enabled());
        drop(s);
        self.area.queue_draw();
        if should_tick {
            self.ensure_tick_callback();
        }
    }

    /// Animates the local waveform fill toward the paused or playing chroma.
    /// This never mutates the application-wide effective accent.
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

    /// Switches between the two colourings and rebuilds everything derived
    /// from the curve. Cheap enough to call on every preference change: it
    /// touches one cached curve per bar, not a frame.
    pub(in crate::ui) fn set_colouring(&self, colouring: SeekColouring) {
        {
            let mut state = self.state.borrow_mut();
            if state.colouring == colouring {
                return;
            }
            state.colouring = colouring;
            rebuild_colour_curve(&mut state);
            // The playhead colour is derived from the curve, so a stale one
            // would survive the switch until the next tick moved it.
            state.head_colour = None;
            state.head_colour_target = None;
        }
        self.area.queue_draw();
    }

    /// Fires on the first press anywhere in the bar, before the drag gesture
    /// resolves into a seek.
    pub(in crate::ui) fn connect_pressed(&self, callback: impl Fn() + 'static) {
        *self.on_press.borrow_mut() = Some(Rc::new(callback));
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
        update_accessible_value(&self.area, fraction, self.state.borrow().duration_ms);
        self.area.queue_draw();
    }

    pub(in crate::ui) fn set_buffered_fraction(&self, fraction: Option<f64>) {
        let fraction = fraction.map(|value| value.clamp(0.0, 1.0));
        let changed = {
            let mut state = self.state.borrow_mut();
            if state.buffered_fraction == fraction {
                false
            } else {
                state.buffered_fraction = fraction;
                true
            }
        };
        if changed {
            self.area.queue_draw();
        }
    }

    #[cfg(test)]
    pub(in crate::ui) fn buffered_fraction_for_test(&self) -> Option<f64> {
        self.state.borrow().buffered_fraction
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
        // position ticks arriving between frames used to read the same stale
        // timestamp and explode the velocity. `frame_time` shares
        // `g_get_monotonic_time`'s timescale, but the timestamps stay separate:
        // position gaps estimate velocity, while frame gaps drive interpolation
        // and reveal a stopped frame clock.
        let now = gtk4::glib::monotonic_time();
        let delta = (fraction - s.target_fraction).abs();
        if delta > 0.05 || s.last_position_us == 0 {
            // Large discontinuity, seek, or no valid time reference yet — snap.
            s.fraction = fraction;
            s.target_fraction = fraction;
            s.fraction_velocity = 0.0;
        } else {
            let dt = (now - s.last_position_us).max(1) as f64;
            s.fraction_velocity = velocity_between(s.target_fraction, fraction, dt);
            s.target_fraction = fraction;
        }
        s.last_position_us = now;
        drop(s);
        update_accessible_value(&self.area, fraction, self.state.borrow().duration_ms);
        self.ensure_tick_callback();
    }

    /// Set the track duration so the hover tooltip can show formatted time
    /// instead of a raw percentage.
    ///
    /// The duration is also the colour curve's timescale. It arrives on every
    /// position tick and independently of the curve itself, so a *changed*
    /// duration rebuilds the averaged curve — and an unchanged one, which is
    /// the case a few times a second, does nothing at all.
    pub(in crate::ui) fn set_duration(&self, duration_ms: i64) {
        let (duration_ms, fraction, rebuilt) = {
            let mut state = self.state.borrow_mut();
            let duration_ms = duration_ms.max(0);
            let changed = state.duration_ms != duration_ms;
            state.duration_ms = duration_ms;
            if changed && !state.raw_centroid.is_empty() {
                rebuild_colour_curve(&mut state);
            }
            (duration_ms, state.target_fraction, changed)
        };
        update_accessible_value(&self.area, fraction, duration_ms);
        if rebuilt {
            self.area.queue_draw();
        }
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
            let stalled = frame_clock_stalled(now, s.last_frame_us);
            let dt = if stalled {
                0.0
            } else {
                (now - s.last_frame_us).max(0) as f64
            };

            if motion::animations_enabled() {
                // Advance the smooth-position interpolation (never past the
                // target), or snap after a stopped frame clock.
                (s.fraction, s.fraction_velocity) = position_step(
                    s.fraction,
                    s.fraction_velocity,
                    dt,
                    s.target_fraction,
                    stalled,
                );

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
                        s.previous_centroid.clear();
                        s.crossfade_start_us = 0;
                    }
                }
            } else {
                s.fraction = s.target_fraction;
                s.fraction_velocity = 0.0;
                s.build_progress = 1.0;
                s.build_start_us = 0;
                s.previous_bars.clear();
                s.previous_centroid.clear();
                s.crossfade_progress = 1.0;
                s.crossfade_start_us = 0;
            }
            s.last_frame_us = now;

            let accent = area.color();
            let accent = (
                f64::from(accent.red()),
                f64::from(accent.green()),
                f64::from(accent.blue()),
            );
            let target = if s.colour_curve.is_empty() {
                accent
            } else {
                spectral_colour(centroid_at(&s.colour_curve, s.fraction))
            };
            s.head_colour_target = Some(target);
            s.head_colour = Some(match s.head_colour {
                Some(current) if motion::animations_enabled() && !stalled => {
                    smooth_towards(current, target, dt / 1_000_000.0, 0.120)
                }
                _ => target,
            });

            let animation_running = s.build_progress < 1.0
                || s.crossfade_progress < 1.0
                || (s.desaturation_progress - s.desaturation_target).abs() > f64::EPSILON;
            let current_draw = RedrawSnapshot {
                head_x: (s.fraction * f64::from(area.width()))
                    .clamp(1.5, (f64::from(area.width()) - 1.5).max(1.5)),
                colour: s.head_colour.unwrap_or(target),
                hover_fraction: s.hover_fraction,
                drag_fraction: s.drag_fraction,
            };
            let last_drawn =
                s.last_drawn_head_x
                    .zip(s.last_drawn_colour)
                    .map(|(head_x, colour)| RedrawSnapshot {
                        head_x,
                        colour,
                        hover_fraction: s.last_drawn_hover_fraction,
                        drag_fraction: s.last_drawn_drag_fraction,
                    });
            let redraw = should_redraw(last_drawn, current_draw, animation_running);
            let settled = (s.fraction - s.target_fraction).abs() < 0.001
                && s.build_progress >= 1.0
                && s.crossfade_progress >= 1.0
                && s.head_colour.is_some_and(|colour| {
                    colour_near(colour, s.head_colour_target.unwrap_or(colour), 1.0 / 512.0)
                });
            drop(s);

            if redraw {
                area.queue_draw();
            }

            if settled {
                *tick_id_slot.borrow_mut() = None;
                return gtk4::glib::ControlFlow::Break;
            }
            gtk4::glib::ControlFlow::Continue
        });
        *self.tick_id.borrow_mut() = Some(id);
    }
}

#[path = "waveform_seek_state.rs"]
mod seek_state;
pub(in crate::ui::player_bar::waveform_seek) use seek_state::{
    ensure_resampled, rebuild_colour_curve, State,
};

#[path = "waveform_seek_render.rs"]
mod render;

#[path = "waveform_surface.rs"]
mod waveform_surface;

#[cfg(test)]
#[path = "waveform_seek_tests.rs"]
mod tests;
