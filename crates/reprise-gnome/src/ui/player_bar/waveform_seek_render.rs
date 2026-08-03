//! Cairo rendering for the waveform seek bar — the played/unplayed bar split,
//! the hover-preview / playhead overlays, and the flat-line fallback. Split out
//! of `waveform_seek.rs` to keep it under the 800-line file cap; the draw
//! callback in `super::WaveformSeek` calls `draw`.

use gtk4::prelude::*;

use super::*;

pub(super) fn draw(
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

    // Playhead: a 1 px line, as narrow as a position marker gets. Outside the
    // mini player it carries a slim glow beside it — see `draw_playhead`.
    let playhead_x = (state.fraction * w).clamp(0.5, (w - 0.5).max(0.5));
    draw_playhead(cr, w, h, playhead_x, (r, g, b), state);
}

#[derive(Clone, Copy)]
struct BarDrawStyle {
    color: (f64, f64, f64),
    build_progress: f64,
    opacity: f64,
}

const PLAYED_MIN_ALPHA: f64 = 0.55;

/// The colour term of a played bar: a floor plus what the music adds.
///
/// The floor is not tuning. The played/unplayed boundary is the seek bar's
/// primary information, and a boundary that dims with a quiet passage makes
/// the position unreadable exactly when the listener looks for it.
const PLAYED_LIGHT_FLOOR: f64 = 0.74;
const PLAYED_LIGHT_PER_PRESSURE: f64 = 0.16;
const PLAYED_LIGHT_PER_SWELL: f64 = 0.10;
/// Half-width of the playhead's glow, in device-independent pixels beside the
/// 1 px line. Deliberately small: this sits where the user aims.
const PLAYHEAD_GLOW_REST_PX: f64 = 2.0;
const PLAYHEAD_GLOW_PER_PRESSURE_PX: f64 = 4.0;
const PLAYHEAD_GLOW_ALPHA_REST: f64 = 0.22;
const PLAYHEAD_GLOW_ALPHA_PER_PRESSURE: f64 = 0.26;

pub(super) fn played_light(pressure: f64, swell: f64) -> f64 {
    PLAYED_LIGHT_FLOOR
        + PLAYED_LIGHT_PER_PRESSURE * pressure.clamp(0.0, 1.0)
        + PLAYED_LIGHT_PER_SWELL * swell.clamp(0.0, 1.0)
}

/// Half-width of the glow beside the playhead line.
///
/// It follows `pressure`, not `kick`. Four attempts drove something here from
/// the raw beat — a lens twice, a radial glow, then a pulsing dot — and all
/// four were rejected on sight for the same reason: at five to seven kicks a
/// second this is the one surface the eye is aiming at, and anything answering
/// per beat reads as flicker rather than as life. `pressure` moves over
/// seconds, so it cannot flicker at all, and the line stays a line.
pub(super) fn playhead_glow_half_width(pressure: f64) -> f64 {
    PLAYHEAD_GLOW_REST_PX + PLAYHEAD_GLOW_PER_PRESSURE_PX * pressure.clamp(0.0, 1.0)
}

pub(super) fn playhead_glow_alpha(pressure: f64) -> f64 {
    PLAYHEAD_GLOW_ALPHA_REST + PLAYHEAD_GLOW_ALPHA_PER_PRESSURE * pressure.clamp(0.0, 1.0)
}

pub(super) fn reactive_light_is_active(
    fill_bars: bool,
    drag_fraction: Option<f64>,
    build_progress: f64,
    crossfade_progress: f64,
) -> bool {
    !fill_bars && drag_fraction.is_none() && build_progress >= 1.0 && crossfade_progress >= 1.0
}

fn draw_playhead(
    cr: &gtk4::cairo::Context,
    width: f64,
    height: f64,
    x: f64,
    color: (f64, f64, f64),
    state: &State,
) {
    let (r, g, b) = color;
    let top = (height - state.max_bar_height) / 2.0;

    // The mini player is 46 bars wide; even a slim glow would wash it out.
    let glow_is_live = reactive_light_is_active(
        state.fill_bars,
        state.drag_fraction,
        state.build_progress,
        state.crossfade_progress,
    );
    if glow_is_live {
        let half = playhead_glow_half_width(state.bass_pressure);
        let alpha = playhead_glow_alpha(state.bass_pressure);
        cr.save().ok();
        cr.rectangle(0.0, 0.0, width, height);
        cr.clip();
        cr.set_operator(gtk4::cairo::Operator::Add);
        let gradient = gtk4::cairo::LinearGradient::new(x - half, 0.0, x + half, 0.0);
        gradient.add_color_stop_rgba(0.0, r, g, b, 0.0);
        gradient.add_color_stop_rgba(0.5, r, g, b, alpha);
        gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
        if cr.set_source(&gradient).is_ok() {
            cr.rectangle(x - half, top, 2.0 * half, state.max_bar_height);
            let _ = cr.fill();
        }
        cr.restore().ok();
    }

    cr.set_source_rgba(r, g, b, PLAYHEAD_ALPHA);
    cr.rectangle(x - 0.5, top, 1.0, state.max_bar_height);
    let _ = cr.fill();
}

/// Brightness of an already-played bar: dim at the start of the track, full at
/// the playhead. Purely positional, so it holds still within a frame.
pub(super) fn played_alpha(index: usize, count: usize, fraction: f64) -> f64 {
    if count == 0 {
        return 1.0;
    }
    let head = fraction * count as f64;
    let distance = ((head - index as f64) / count as f64).clamp(0.0, 1.0);
    PLAYED_MIN_ALPHA + (1.0 - PLAYED_MIN_ALPHA) * (1.0 - distance)
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
    // Slots fill the full width so the seek mapping stays linear. Full
    // waveform: fixed 3px bars, gaps widen when the count caps. Mini player
    // (fill mode): equal-width bars minus a small gap, tiling the width.
    let slot = w / count as f64;
    let bar_w = bar_slot_width(slot, state.fill_bars, MINI_BAR_GAP);
    let bar_radius = if state.fill_bars {
        MINI_BAR_RADIUS
    } else {
        BAR_RADIUS
    };

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
                bar_height(f64::from(level), state.min_bar_height, state.max_bar_height) * stagger
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
            cr.set_source_rgba(
                r,
                g,
                b,
                style.opacity
                    * played_light(state.bass_pressure, state.bass_swell)
                    * played_alpha(index, count, state.fraction),
            );
        } else if is_hover_preview {
            cr.set_source_rgba(1.0, 1.0, 1.0, HOVER_PREVIEW_ALPHA * style.opacity);
        } else {
            cr.set_source_rgba(1.0, 1.0, 1.0, UNPLAYED_ALPHA * style.opacity);
        }
        rounded_bar(cr, x, y, bar_w, bar_h, bar_radius);
        let _ = cr.fill();
    }
}

fn bar_height(magnitude: f64, min_bar_height: f64, max_bar_height: f64) -> f64 {
    min_bar_height + magnitude.clamp(0.0, 1.0) * (max_bar_height - min_bar_height)
}

#[cfg(test)]
pub(super) fn bar_height_for_test(level: u8, min_bar_height: f64, max_bar_height: f64) -> f64 {
    bar_height(
        f64::from(level) / f64::from(u8::MAX),
        min_bar_height,
        max_bar_height,
    )
}

#[cfg(test)]
pub(super) fn bar_height_for_test_with_light(
    level: u8,
    min_bar_height: f64,
    max_bar_height: f64,
    _kick: f64,
    _pressure: f64,
    _swell: f64,
) -> f64 {
    bar_height_for_test(level, min_bar_height, max_bar_height)
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
    let count = resolve_bar_count(state.bar_count_override, w as i32);
    if count == 0 {
        return;
    }
    let slot = w / count as f64;
    let bar_w = bar_slot_width(slot, state.fill_bars, MINI_BAR_GAP);
    let bar_radius = if state.fill_bars {
        MINI_BAR_RADIUS
    } else {
        BAR_RADIUS
    };

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
        rounded_bar(cr, x, y, bar_w, bar_h, bar_radius);
        let _ = cr.fill();
    }
}
