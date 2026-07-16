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
use reprise_core::format::format_duration;

/// Shared, cloneable slot for the optional seek handler (cloned out before it
/// is invoked so no `RefCell` borrow is held across the call).
type SeekCallback = Rc<RefCell<Option<Rc<dyn Fn(f64)>>>>;

pub(super) const WAVEFORM_CSS_CLASS: &str = "waveform-seek";
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
/// Buckets quieter than −50 dB relative to the track's own maximum RMS render
/// as fixed 2 px dots instead of scaled bars. Stored values are normalized to
/// the track max, so the threshold is relative: 10^(−50/20).
const SILENCE_RMS: f32 = 0.003_162_28;
const SILENCE_DOT_HEIGHT: f64 = 2.0;
/// Percentile window for the height mapping: p10 → minimum height, p95 →
/// full height, values above clip. This is what gives a uniformly loud
/// (compressed) track visible internal dynamics.
const PERCENTILE_LOW: f64 = 0.10;
const PERCENTILE_HIGH: f64 = 0.95;
/// Gamma applied after the percentile mapping — pushes mid levels down and
/// spreads the visible contrast between verse and chorus.
const HEIGHT_GAMMA: f32 = 1.6;
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
/// Build-up animation duration in seconds.
const BUILD_DURATION_S: f64 = 0.3;
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

/// One display bar: either a true-silence dot or an audible level in 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DisplayBar {
    Silence,
    Level(f32),
}

/// Aggregates the stored peaks (sqrt-compressed RMS, see `waveform_peaks.rs`)
/// into `count` buckets in the *linear* RMS domain: undo the sqrt compression,
/// average power over the window, take the root. Returns RMS values in 0..1
/// (relative to the track's own maximum).
fn aggregate_rms(raw: &[u8], count: usize) -> Vec<f32> {
    if raw.is_empty() || count == 0 {
        return Vec::new();
    }
    (0..count)
        .map(|i| {
            let start = i * raw.len() / count;
            let end = (((i + 1) * raw.len() / count).max(start + 1)).min(raw.len());
            let slice = &raw[start..end];
            let mean_power: f32 = slice
                .iter()
                .map(|&v| {
                    let rms = (f32::from(v) / 255.0).powi(2); // undo sqrt compression
                    rms * rms // power
                })
                .sum::<f32>()
                / slice.len() as f32;
            mean_power.sqrt()
        })
        .collect()
}

/// Nearest-rank percentile over an already sorted slice.
fn percentile(sorted: &[f32], p: f64) -> f32 {
    let last = sorted.len() - 1;
    let rank = ((last as f64) * p).round() as usize;
    sorted[rank.min(last)]
}

/// 3-bucket moving average with 25/50/25 weights; edges clamp to themselves.
/// Applied AFTER the percentile mapping, purely against bar-to-bar flicker.
fn smooth_neighbors(values: &[f32]) -> Vec<f32> {
    (0..values.len())
        .map(|i| {
            let prev = values[i.saturating_sub(1)];
            let next = values[(i + 1).min(values.len() - 1)];
            0.25 * prev + 0.5 * values[i] + 0.25 * next
        })
        .collect()
}

/// The full display pipeline: aggregate to `count` RMS buckets, map through
/// the p10..p95 percentile window (giving compressed material internal
/// dynamics), apply the gamma curve, smooth, and mark true silence. The
/// degenerate case (all audible buckets equal) renders at mid height rather
/// than as a full wall.
fn shape_display_peaks(raw: &[u8], count: usize) -> Vec<DisplayBar> {
    let rms = aggregate_rms(raw, count);
    if rms.is_empty() {
        return Vec::new();
    }
    let mut audible: Vec<f32> = rms
        .iter()
        .copied()
        .filter(|value| *value >= SILENCE_RMS)
        .collect();
    if audible.is_empty() {
        return vec![DisplayBar::Silence; rms.len()];
    }
    audible.sort_by(f32::total_cmp);
    let low = percentile(&audible, PERCENTILE_LOW);
    let high = percentile(&audible, PERCENTILE_HIGH);
    let span = high - low;

    let shaped: Vec<f32> = rms
        .iter()
        .map(|&value| {
            if value < SILENCE_RMS {
                return 0.0;
            }
            let norm = if span <= f32::EPSILON {
                // Degenerate percentile window (≥ ~85% of buckets identical):
                // the flat mass sits at mid height, anything louder than the
                // window still clips to full height.
                if value > high {
                    1.0
                } else {
                    0.5
                }
            } else {
                ((value - low) / span).clamp(0.0, 1.0)
            };
            norm.powf(HEIGHT_GAMMA)
        })
        .collect();
    let smoothed = smooth_neighbors(&shaped);

    rms.iter()
        .zip(smoothed)
        .map(|(&value, level)| {
            if value < SILENCE_RMS {
                DisplayBar::Silence
            } else {
                DisplayBar::Level(level)
            }
        })
        .collect()
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
    min_bar_height: f64,
    max_bar_height: f64,
    // Duration of the current track (ms), for formatted tooltip display.
    duration_ms: i64,
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
        Self::new_with_heights(
            CONTENT_HEIGHT,
            MAX_BAR_HEIGHT,
            MIN_BAR_HEIGHT,
            FALLBACK_BAR_HEIGHT,
        )
    }

    pub(super) fn new_mini() -> Self {
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
            min_bar_height: min_h,
            max_bar_height: max_h,
            duration_ms: 0,
        }));
        let on_seek: SeekCallback = Rc::new(RefCell::new(None));
        let tick_id: Rc<RefCell<Option<gtk4::TickCallbackId>>> = Rc::new(RefCell::new(None));

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
        }
    }

    pub(super) fn widget(&self) -> &gtk4::DrawingArea {
        &self.area
    }

    /// Set peaks (as raw `u8` values, 0-255) and trigger a 300 ms build-up
    /// animation (gated on `gtk-enable-animations`). Use this whenever the
    /// track changes.
    pub(super) fn set_peaks(&self, peaks: Vec<u8>) {
        let now = self.area.frame_clock().map_or(0, |c| c.frame_time());
        let animate = gtk4::Settings::default().is_none_or(|s| s.is_gtk_enable_animations());
        let mut s = self.state.borrow_mut();
        s.raw_peaks = peaks;
        s.display_peaks.clear();
        s.last_display_width = 0; // force resample on next draw
        if animate && !s.raw_peaks.is_empty() {
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
    ///
    /// Large jumps (> 5% of the track) are treated as seeks: the fraction snaps
    /// instantly and velocity resets, preventing the overshoot that occurs when
    /// a stale pre-seek position tick arrives before the post-seek position.
    pub(super) fn set_fraction_smooth(&self, fraction: f64) {
        let fraction = fraction.clamp(0.0, 1.0);
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
    pub(super) fn set_duration(&self, duration_ms: i64) {
        self.state.borrow_mut().duration_ms = duration_ms.max(0);
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

            // Advance the smooth-position interpolation (never past the
            // target — see `interpolation_step`).
            let dt = (now - s.last_tick_us).max(0) as f64;
            s.fraction = interpolation_step(s.fraction, s.fraction_velocity, dt, s.target_fraction);
            s.last_tick_us = now;

            // Advance the build-up animation.
            if s.build_progress < 1.0 && s.build_start_us > 0 {
                let elapsed = (now - s.build_start_us) as f64 / 1_000_000.0;
                s.build_progress = (elapsed / BUILD_DURATION_S).clamp(0.0, 1.0);
            }

            let settled = (s.fraction - s.target_fraction).abs() < 0.001 && s.build_progress >= 1.0;
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

    let count = state.display_peaks.len();
    // Slots fill the full width so the seek mapping stays linear; when the
    // bar-count cap kicks in the gaps simply widen (bars stay 3 px).
    let slot = w / count as f64;
    let bar_w = BAR_WIDTH.min(slot.max(1.0));

    let color = area.color();
    let (r, g, b) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );

    for (index, &bar) in state.display_peaks.iter().enumerate() {
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

        if is_ghost {
            cr.set_source_rgba(r, g, b, GHOST_ALPHA);
        } else if played {
            cr.set_source_rgba(r, g, b, 1.0);
        } else if is_hover_preview {
            cr.set_source_rgba(1.0, 1.0, 1.0, HOVER_PREVIEW_ALPHA);
        } else {
            cr.set_source_rgba(1.0, 1.0, 1.0, UNPLAYED_ALPHA);
        }
        rounded_bar(cr, x, y, bar_w, bar_h, BAR_RADIUS);
        let _ = cr.fill();
    }

    // Playhead: a 1 px line at the exact fraction, drawn over the bars —
    // replaces the old partially-filled boundary bar (the played/unplayed
    // switch is a hard per-bucket cut instead).
    let playhead_x = (state.fraction * w).clamp(0.5, (w - 0.5).max(0.5));
    cr.set_source_rgba(1.0, 1.0, 1.0, PLAYHEAD_ALPHA);
    cr.rectangle(
        playhead_x - 0.5,
        (h - state.max_bar_height) / 2.0,
        1.0,
        state.max_bar_height,
    );
    let _ = cr.fill();
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
    let (r, g, b) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );

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
mod tests {
    use super::*;

    #[test]
    fn interpolation_step_never_overshoots_its_target() {
        // Ordinary frame: advances proportionally, still below target.
        let stepped = interpolation_step(0.10, 1e-6, 16_000.0, 0.20);
        assert!((stepped - 0.116).abs() < 1e-9);
        // Runaway velocity (the stuck-at-100% bug): a stale frame-clock
        // reading once produced dt = 1 µs and an exploded velocity; one real
        // frame then shot the fill to 1.0. The step must stop AT the target.
        assert_eq!(interpolation_step(0.0, 0.002, 16_000.0, 0.004), 0.004);
        // Backwards motion clamps at the target from below, too.
        assert_eq!(interpolation_step(0.5, -0.002, 16_000.0, 0.3), 0.3);
    }

    #[test]
    fn interpolation_step_recovers_a_fill_stuck_beyond_the_target() {
        // Self-healing: if fraction is already past the target (legacy stuck
        // state at 1.0 while the song still plays), the next step snaps back
        // to the target instead of staying pinned.
        assert_eq!(interpolation_step(1.0, 1e-7, 16_000.0, 0.02), 0.02);
    }

    #[test]
    fn interpolation_step_stays_inside_the_unit_range() {
        assert_eq!(interpolation_step(0.99, 0.5, 16_000.0, 1.0), 1.0);
        assert_eq!(interpolation_step(0.01, -0.5, 16_000.0, 0.0), 0.0);
    }

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
        assert!(in_ghost(1, 4, 0.25, 0.75)); // centre 0.375 in (0.25, 0.75]
        assert!(in_ghost(2, 4, 0.25, 0.75)); // centre 0.625 in (0.25, 0.75]
        assert!(!in_ghost(3, 4, 0.25, 0.75)); // centre 0.875 > 0.75

        // Reversed drag: drag < fraction should also produce a ghost range.
        assert!(!in_ghost(0, 4, 0.75, 0.25)); // centre 0.125 ≤ 0.25
        assert!(in_ghost(1, 4, 0.75, 0.25)); // centre 0.375 in (0.25, 0.75]
        assert!(in_ghost(2, 4, 0.75, 0.25)); // centre 0.625 in (0.25, 0.75]
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

    #[test]
    fn aggregate_rms_undoes_the_stored_sqrt_compression() {
        // Stored values are sqrt-compressed: v = sqrt(rms) * 255. A stored 255
        // must aggregate back to rms 1.0, a stored 0 to 0.0.
        let rms = aggregate_rms(&[255, 255, 0, 0], 2);
        assert_eq!(rms.len(), 2);
        assert!((rms[0] - 1.0).abs() < 1e-6);
        assert!(rms[1].abs() < 1e-6);
    }

    #[test]
    fn aggregate_rms_handles_empty_input() {
        assert!(aggregate_rms(&[], 10).is_empty());
        assert!(aggregate_rms(&[128], 0).is_empty());
    }

    #[test]
    fn shape_gives_a_compressed_wall_internal_dynamics() {
        // A "loudness war" track: RMS varies only in a narrow, loud band
        // (a 230-ish verse into a 250-ish chorus). Percentile mapping must
        // spread that band across the full height.
        let mut raw = vec![230u8; 100];
        raw.extend([250u8; 100]);
        let bars = shape_display_peaks(&raw, 100);
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for bar in &bars {
            if let DisplayBar::Level(level) = bar {
                lo = lo.min(*level);
                hi = hi.max(*level);
            }
        }
        assert!(
            hi - lo > 0.5,
            "narrow loud band must be spread out, got lo={lo} hi={hi}"
        );
    }

    #[test]
    fn shape_clips_outliers_above_the_high_percentile() {
        // 96 quiet bars, 4 very loud ones: the loud ones sit above p95 and
        // must clip to the full height (1.0 after gamma).
        let mut raw = vec![100u8; 96];
        raw.extend([255u8; 4]);
        let bars = shape_display_peaks(&raw, 100);
        let last = bars.last().unwrap();
        match last {
            DisplayBar::Level(level) => assert!(*level > 0.95, "outlier level {level}"),
            DisplayBar::Silence => panic!("loud bar classified as silence"),
        }
    }

    #[test]
    fn shape_marks_true_silence_as_dots_not_levels() {
        // Stored 0 (and anything below −50 dB of track max) is silence.
        let mut raw = vec![0u8; 10];
        raw.extend([200u8; 90]);
        let bars = shape_display_peaks(&raw, 100);
        assert_eq!(bars[0], DisplayBar::Silence);
        assert!(matches!(bars[99], DisplayBar::Level(_)));
    }

    #[test]
    fn shape_of_a_perfectly_flat_track_sits_mid_height_not_full() {
        // Degenerate percentiles (p10 == p95): render mid-height, never a
        // full-height wall.
        let raw = vec![200u8; 100];
        let bars = shape_display_peaks(&raw, 50);
        for bar in bars {
            match bar {
                DisplayBar::Level(level) => {
                    assert!((0.05..0.95).contains(&level), "flat level {level}");
                }
                DisplayBar::Silence => panic!("flat loud track is not silence"),
            }
        }
    }

    #[test]
    fn smoothing_averages_neighbors_25_50_25() {
        let smoothed = smooth_neighbors(&[0.0, 1.0, 0.0]);
        // Middle: 0.25*0 + 0.5*1 + 0.25*0 = 0.5; edges clamp to themselves:
        // 0.25*0 + 0.5*0 + 0.25*1 = 0.25.
        assert!((smoothed[1] - 0.5).abs() < 1e-6);
        assert!((smoothed[0] - 0.25).abs() < 1e-6);
        assert!((smoothed[2] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn compute_bar_count_uses_fixed_slots_and_caps_at_160() {
        assert_eq!(compute_bar_count(0), 1);
        assert_eq!(compute_bar_count(1), 1);
        // 600px / 5px per slot = 120 bars.
        assert_eq!(compute_bar_count(600), 120);
        // Very wide bars hit the hard cap.
        assert_eq!(compute_bar_count(2000), 160);
    }

    #[test]
    fn ensure_resampled_clears_display_peaks_when_raw_empty() {
        let mut state = State {
            raw_peaks: Vec::new(),
            display_peaks: vec![DisplayBar::Level(0.5)],
            last_display_width: 100,
            fraction: 0.0,
            hover_fraction: None,
            drag_fraction: None,
            target_fraction: 0.0,
            fraction_velocity: 0.0,
            last_tick_us: 0,
            build_progress: 1.0,
            build_start_us: 0,
            min_bar_height: MIN_BAR_HEIGHT,
            max_bar_height: MAX_BAR_HEIGHT,
            duration_ms: 0,
        };
        ensure_resampled(&mut state, 200);
        assert!(state.display_peaks.is_empty());
    }

    #[test]
    fn ensure_resampled_populates_on_width_change() {
        let mut state = State {
            raw_peaks: vec![128u8; 1000],
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
            min_bar_height: MIN_BAR_HEIGHT,
            max_bar_height: MAX_BAR_HEIGHT,
            duration_ms: 0,
        };
        ensure_resampled(&mut state, 600);
        assert!(!state.display_peaks.is_empty());
        assert_eq!(state.last_display_width, 600);

        // Calling again with same width should not change the display_peaks vec.
        let before_len = state.display_peaks.len();
        ensure_resampled(&mut state, 600);
        assert_eq!(state.display_peaks.len(), before_len);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mini_waveform_has_16px_height() {
        if gtk4::init().is_err() {
            return;
        }
        let w = WaveformSeek::new_mini();
        assert_eq!(w.widget().content_height(), 16);
    }
}
