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
        rounded_bar(cr, x, y, bar_w, bar_h, bar_radius);
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
