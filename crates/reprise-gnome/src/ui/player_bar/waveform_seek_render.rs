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

    // Playhead: a 1 px line at the exact fraction, drawn over the bars —
    // replaces the old partially-filled boundary bar (the played/unplayed
    // switch is a hard per-bucket cut instead).
    let playhead_x = (state.fraction * w).clamp(0.5, (w - 0.5).max(0.5));
    if glow_is_active(
        state.fill_bars,
        state.drag_fraction,
        state.build_progress,
        state.crossfade_progress,
    ) {
        let radius = playhead_glow_radius(h, state.bass_kick);
        let alpha = playhead_glow_alpha(state.bass_kick, state.bass_pressure);
        let center_y = h / 2.0;
        let glow = gtk4::cairo::RadialGradient::new(
            playhead_x, center_y, 0.0, playhead_x, center_y, radius,
        );
        glow.add_color_stop_rgba(0.0, r, g, b, alpha);
        glow.add_color_stop_rgba(1.0, r, g, b, 0.0);
        cr.save().ok();
        cr.rectangle(0.0, 0.0, w, h);
        cr.clip();
        cr.set_operator(gtk4::cairo::Operator::Add);
        if cr.set_source(&glow).is_ok() {
            cr.arc(playhead_x, center_y, radius, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
        cr.restore().ok();
    }
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

const GLOW_RADIUS_REST: f64 = 0.22;
const GLOW_RADIUS_PER_KICK: f64 = 1.15;
const GLOW_ALPHA_REST: f64 = 0.30;
const GLOW_ALPHA_PER_PRESSURE: f64 = 0.12;
const GLOW_ALPHA_PER_KICK: f64 = 0.40;
const PLAYED_MIN_ALPHA: f64 = 0.55;

pub(super) fn playhead_glow_radius(height: f64, kick: f64) -> f64 {
    height * (GLOW_RADIUS_REST + GLOW_RADIUS_PER_KICK * kick.clamp(0.0, 1.0))
}

pub(super) fn playhead_glow_alpha(kick: f64, pressure: f64) -> f64 {
    GLOW_ALPHA_REST
        + GLOW_ALPHA_PER_PRESSURE * pressure.clamp(0.0, 1.0)
        + GLOW_ALPHA_PER_KICK * kick.clamp(0.0, 1.0)
}

/// The glow is out of the way while the user is aiming, and while another
/// animation owns the bar.
pub(super) fn glow_is_active(
    fill_bars: bool,
    drag_fraction: Option<f64>,
    build_progress: f64,
    crossfade_progress: f64,
) -> bool {
    !fill_bars && drag_fraction.is_none() && build_progress >= 1.0 && crossfade_progress >= 1.0
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
                let magnitude = f64::from(level).clamp(0.0, 1.0);
                bar_height(magnitude, state.min_bar_height, state.max_bar_height) * stagger
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
                style.opacity * played_alpha(index, count, state.fraction),
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
pub(super) fn bar_height_for_test_with_kick(
    level: u8,
    min_bar_height: f64,
    max_bar_height: f64,
    _kick: f64,
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
