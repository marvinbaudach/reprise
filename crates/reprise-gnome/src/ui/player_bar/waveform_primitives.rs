//! Stateless geometry, input, and accessibility helpers for the waveform.

use std::f64::consts::{FRAC_PI_2, PI};

use gtk4::prelude::*;
use reprise_core::format::format_duration;

pub(super) const BAR_WIDTH: f64 = 3.0;
pub(super) const BAR_RADIUS: f64 = BAR_WIDTH / 2.0;
pub(super) const BAR_GAP: f64 = 2.0;
const MAX_BAR_COUNT: usize = 160;

/// Mini-player waveform (redesign frame 1e / MINI-1): a fixed, reduced bar
/// count whose bars fill the width (equal-width) instead of the dense
/// fixed-3px-bar look of the full waveform.
pub(super) const MINI_BAR_COUNT: usize = 46;
pub(super) const MINI_BAR_GAP: f64 = 1.5;
pub(super) const MINI_BAR_RADIUS: f64 = 1.0;

/// Advances the smooth-fill interpolation by one frame: `fraction` moves by
/// `velocity * dt_us` but never past `target` — the interpolation chases the
/// most recent position tick, so overshooting it is always wrong. This bound
/// is what makes a mis-measured `dt` (and thus an exploded velocity)
/// harmless: the worst case degrades to snapping straight to the target
/// instead of pinning the fill at 100% for the rest of the song. A fraction
/// that is already past the target (a stale stuck state) snaps back to it
/// for the same reason. Result stays in 0..1.
pub(super) fn interpolation_step(fraction: f64, velocity: f64, dt_us: f64, target: f64) -> f64 {
    let advanced = velocity.mul_add(dt_us, fraction);
    let bounded = if velocity >= 0.0 {
        advanced.min(target)
    } else {
        advanced.max(target)
    };
    bounded.clamp(0.0, 1.0)
}

/// Maps a pointer `x` within `width` to a 0..1 seek fraction.
pub(super) fn fraction_at(x: f64, width: f64) -> f64 {
    if width <= 0.0 {
        return 0.0;
    }
    (x / width).clamp(0.0, 1.0)
}

pub(super) fn keyboard_seek_target(
    key: gtk4::gdk::Key,
    current: f64,
    duration_ms: i64,
) -> Option<f64> {
    const ARROW_STEP_MS: f64 = 5_000.0;
    const PAGE_STEP_MS: f64 = 30_000.0;
    let duration = duration_ms.max(0) as f64;
    let arrow = if duration > 0.0 {
        ARROW_STEP_MS / duration
    } else {
        0.05
    };
    let page = if duration > 0.0 {
        PAGE_STEP_MS / duration
    } else {
        0.10
    };
    let target = match key {
        gtk4::gdk::Key::Left | gtk4::gdk::Key::Down => current - arrow,
        gtk4::gdk::Key::Right | gtk4::gdk::Key::Up => current + arrow,
        gtk4::gdk::Key::Page_Down => current - page,
        gtk4::gdk::Key::Page_Up => current + page,
        gtk4::gdk::Key::Home => 0.0,
        gtk4::gdk::Key::End => 1.0,
        _ => return None,
    };
    Some(target.clamp(0.0, 1.0))
}

/// Whether bar `index` of `count` falls within the played `fraction` (using the
/// bar's centre so the split lands mid-bar rather than on an edge).
pub(super) fn bar_played(index: usize, count: usize, fraction: f64) -> bool {
    if count == 0 {
        return false;
    }
    ((index as f64 + 0.5) / count as f64) <= fraction
}

/// Number of display bars for `width` pixels: fixed 3 px bars + 2 px gaps,
/// hard-capped at 160 bars (when capped, the slots widen instead).
pub(super) fn compute_bar_count(width: i32) -> usize {
    ((f64::from(width) / (BAR_WIDTH + BAR_GAP)).floor() as usize).clamp(1, MAX_BAR_COUNT)
}

/// Resolves the number of display bars: a fixed `override_count` (mini player)
/// wins over the width-derived dynamic count (full waveform). Never zero.
pub(super) fn resolve_bar_count(override_count: Option<usize>, width: i32) -> usize {
    match override_count {
        Some(count) => count.max(1),
        None => compute_bar_count(width),
    }
}

/// Width of a single bar within its `slot`. In `fill` mode (mini player) the
/// bar spans the slot minus `gap`, so equal-width bars tile the whole width;
/// otherwise a fixed `BAR_WIDTH` bar sits centred in the slot.
pub(super) fn bar_slot_width(slot: f64, fill: bool, gap: f64) -> f64 {
    if fill {
        (slot - gap).max(1.0)
    } else {
        BAR_WIDTH.min(slot.max(1.0))
    }
}

pub(super) fn update_accessible_value(area: &gtk4::DrawingArea, fraction: f64, duration_ms: i64) {
    let fraction = fraction.clamp(0.0, 1.0);
    let value_text = if duration_ms > 0 {
        format_duration((fraction * duration_ms as f64).round() as i64)
    } else {
        format!("{:.0}%", fraction * 100.0)
    };
    area.update_property(&[
        gtk4::accessible::Property::ValueMin(0.0),
        gtk4::accessible::Property::ValueMax(100.0),
        gtk4::accessible::Property::ValueNow(fraction * 100.0),
        gtk4::accessible::Property::ValueText(&value_text),
    ]);
}

pub(super) fn rounded_bar(
    cr: &gtk4::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    cr.new_sub_path();
    cr.arc(x + width - radius, y + radius, radius, -FRAC_PI_2, 0.0);
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        FRAC_PI_2,
    );
    cr.arc(x + radius, y + height - radius, radius, FRAC_PI_2, PI);
    cr.arc(x + radius, y + radius, radius, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_bar_count_prefers_override() {
        // Mini player: a fixed 46-bar count regardless of width (frame 1e).
        assert_eq!(resolve_bar_count(Some(MINI_BAR_COUNT), 300), 46);
        assert_eq!(resolve_bar_count(Some(MINI_BAR_COUNT), 800), 46);
        // Never collapses to zero even with a degenerate override.
        assert_eq!(resolve_bar_count(Some(0), 300), 1);
        // Full waveform: falls back to the width-derived dynamic count.
        assert_eq!(resolve_bar_count(None, 300), compute_bar_count(300));
    }

    #[test]
    fn fill_bars_span_the_slot_minus_gap() {
        let slot = 6.4;
        assert!((bar_slot_width(slot, true, MINI_BAR_GAP) - (slot - MINI_BAR_GAP)).abs() < 1e-9);
        // A slot narrower than the gap still yields a visible (>=1px) bar.
        assert_eq!(bar_slot_width(1.0, true, MINI_BAR_GAP), 1.0);
        // Non-fill (full waveform) keeps the fixed bar width.
        assert_eq!(bar_slot_width(slot, false, 0.0), BAR_WIDTH);
    }
}
