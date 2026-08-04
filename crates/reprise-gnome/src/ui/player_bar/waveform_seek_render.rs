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
    state: &mut State,
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
    let widget_colour = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );
    let chroma_factor = 1.0 - 0.55 * state.desaturation_progress;
    let (r, g, b) = scale_chroma(
        widget_colour.0,
        widget_colour.1,
        widget_colour.2,
        chroma_factor,
    );

    let bar_head_x = (state.fraction * w).clamp(0.0, w);
    let playhead_x = bar_head_x.clamp(1.5, (w - 1.5).max(1.5));
    let cache_live = waveform_surface::cache_is_live(state)
        && waveform_surface::ensure_cache(state, width, height, area.scale_factor(), widget_colour);
    if cache_live {
        waveform_surface::draw_cached_bars(cr, state, w, h, bar_head_x, (r, g, b));
    } else if state.crossfade_progress < 1.0 && !state.previous_bars.is_empty() {
        draw_bars(
            cr,
            w,
            h,
            &state.previous_bars,
            state,
            BarDrawStyle {
                color: (r, g, b),
                centroid: &state.previous_centroid,
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
                centroid: &state.shaped_centroid,
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
                centroid: &state.shaped_centroid,
                build_progress: state.build_progress,
                opacity: 1.0,
            },
        );
    }

    let head_colour = if state.raw_centroid.is_empty() {
        (r, g, b)
    } else {
        state
            .head_colour
            .unwrap_or_else(|| spectral_colour(centroid_at(&state.raw_centroid, state.fraction)))
    };
    let top = (h - state.max_bar_height) / 2.0;
    let decorations_are_live =
        !state.fill_bars && state.build_progress >= 1.0 && state.crossfade_progress >= 1.0;
    if decorations_are_live {
        crate::ui::player_bar::waveform_playhead::draw_afterglow(
            cr,
            playhead_x,
            top,
            state.max_bar_height,
            head_colour,
        );
    }
    crate::ui::player_bar::waveform_playhead::draw_playhead(
        cr,
        playhead_x,
        top,
        state.max_bar_height,
        head_colour,
        decorations_are_live,
        PLAYHEAD_ALPHA,
    );
    state.last_drawn_head_x = Some(playhead_x);
    state.last_drawn_colour = Some(head_colour);
    state.last_drawn_hover_fraction = state.hover_fraction;
    state.last_drawn_drag_fraction = state.drag_fraction;
    state.last_drawn_pressure = state.bass_pressure;
    state.last_drawn_swell = state.bass_swell;
}

#[derive(Clone, Copy)]
struct BarDrawStyle<'a> {
    color: (f64, f64, f64),
    centroid: &'a [f32],
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

pub(super) fn played_light(pressure: f64, swell: f64) -> f64 {
    PLAYED_LIGHT_FLOOR
        + PLAYED_LIGHT_PER_PRESSURE * pressure.clamp(0.0, 1.0)
        + PLAYED_LIGHT_PER_SWELL * swell.clamp(0.0, 1.0)
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
    style: BarDrawStyle<'_>,
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

        let accent = style.color;
        let spectral = style
            .centroid
            .get(index)
            .map(|value| spectral_colour(f64::from(*value)));
        let (r, g, b) = spectral
            .map(|colour| {
                let chroma_factor = 1.0 - 0.55 * state.desaturation_progress;
                scale_chroma(colour.0, colour.1, colour.2, chroma_factor)
            })
            .unwrap_or(accent);
        if is_ghost {
            cr.set_source_rgba(accent.0, accent.1, accent.2, GHOST_ALPHA * style.opacity);
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

pub(super) fn bar_height(magnitude: f64, min_bar_height: f64, max_bar_height: f64) -> f64 {
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
