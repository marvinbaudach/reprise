//! Small geometry helpers shared by list reload and tag-save anchoring.

use std::cell::Cell;

use crate::ui::style::tokens::{ROW_MIN_HEIGHT_COMFORTABLE, ROW_MIN_HEIGHT_COMPACT};

const MAX_READY_DRIFT_IN_ROWS: f64 = 0.5;

/// Total drift, expressed in rows, that a row count running ahead of the
/// allocation explains on its own: `old_upper / new_count` is off by exactly
/// the count delta, spread over the whole list. Two rows covers the one-row
/// case `MAX_READY_DRIFT_IN_ROWS` already rejects, plus floating-point slack.
/// A density change moves a 200-row list by ~59 rows' worth, and even the
/// smallest step needs only seven rows to clear this — shorter lists fit
/// their viewport and never reach a scroll restore at all.
const MAX_STALE_DRIFT_IN_ROWS: f64 = 2.0;

/// Drops the cached height, so the next measurement is believed unconditionally.
/// The row height is a property of the display density, and a density change is
/// the one event that invalidates it without any geometry looking wrong.
pub(in crate::ui) fn forget_row_height(last_row_height: &Cell<f64>) {
    crate::ui::list_geometry::invalidate_row_height(last_row_height);
}

/// Decides whether a freshly measured row height replaces the cached one.
///
/// Both candidates for a difference look the same in absolute pixels, so the
/// judgement has to be *relative and per row*:
/// - A stale adjustment is the previous list's `upper` over the new row count,
///   so its error is the count delta spread over the list: being one row
///   behind moves the derived height by `cached / n_rows`, and the longer the
///   list the smaller that is per row.
/// - A density change moves every row by the same fraction, whatever the list
///   length.
///
/// An absolute budget cannot separate those: the same 10 px per row is a
/// rounding artefact in a three-row list and a whole density step in a
/// 200-row one. Judging total drift in rows does separate them.
#[allow(dead_code)] // Removed with its superseded heuristic in G5.
pub(in crate::ui) fn should_replace_cached_height(
    measured: f64,
    cached: f64,
    n_rows: usize,
) -> bool {
    if cached <= 0.0 {
        return measured > 0.0;
    }
    if measured <= 0.0 || n_rows == 0 {
        return false;
    }
    // A measurement further from the cache than one density change could ever
    // move a row is stale geometry, not a new density. The CSS minima bound
    // that span, and the constant cell chrome added to both only pulls the
    // real ratio closer to 1, so this stays on the generous side.
    let max_density_ratio =
        f64::from(ROW_MIN_HEIGHT_COMFORTABLE) / f64::from(ROW_MIN_HEIGHT_COMPACT);
    let ratio = measured / cached;
    if ratio > max_density_ratio || ratio < 1.0 / max_density_ratio {
        return false;
    }
    (measured - cached).abs() / cached * n_rows as f64 > MAX_STALE_DRIFT_IN_ROWS
}

/// Uses the last consistent measurement across a model swap, falling back to
/// the live geometry during the first allocation.
#[allow(dead_code)] // Removed with its superseded fallback in G5.
pub(in crate::ui) fn row_height_for_restore(
    last_row_height: &Cell<f64>,
    upper: f64,
    n_rows: usize,
) -> Option<f64> {
    if n_rows == 0 || upper <= 0.0 {
        return None;
    }
    let cached = last_row_height.get();
    Some(if cached > 0.0 {
        cached
    } else {
        crate::ui::list_geometry::adjustment_row_height(upper, n_rows)?.pixels()
    })
}

/// The adjustment is ready only once it describes the same rows as the model.
/// Sub-row drift allows for fractional allocation rounding, but a full row is
/// stale geometry: a one-row deletion otherwise looks consistent and poisons
/// the cached height with `old_upper / new_count`.
pub(in crate::ui) fn restore_geometry_is_ready(upper: f64, n_rows: usize, height: f64) -> bool {
    (upper - n_rows as f64 * height).abs() < height * MAX_READY_DRIFT_IN_ROWS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_one_row_count_change_is_not_ready_geometry() {
        let row_height = 36.0;
        let old_upper = 200.0 * row_height;

        assert!(!restore_geometry_is_ready(old_upper, 199, row_height));
    }

    #[test]
    fn sub_row_rounding_drift_is_ready_geometry() {
        let row_height = 36.0;
        let rounded_upper = 199.0 * row_height + row_height * 0.49;

        assert!(restore_geometry_is_ready(rounded_upper, 199, row_height));
    }

    #[test]
    fn a_changed_display_density_replaces_the_cached_height() {
        // Standard 34px → compact 24px over a 200-row list. The absolute
        // difference (2000px in total) dwarfs any per-row tolerance, which is
        // precisely why the old whole-height comparison rejected it forever.
        assert!(should_replace_cached_height(24.0, 34.0, 200));
        assert!(should_replace_cached_height(34.0, 24.0, 200));
    }

    #[test]
    fn a_one_row_stale_measurement_does_not_replace_the_cached_height() {
        let cached = 34.0;
        let n_rows = 200;
        let stale = cached * (n_rows + 1) as f64 / n_rows as f64;

        assert!(!should_replace_cached_height(stale, cached, n_rows));
    }

    #[test]
    fn a_measurement_from_a_wholly_different_list_is_rejected() {
        // The measured journey: the 22-row artist view's `upper` divided by
        // the 2276-row library it is going back to.
        let cached = 34.0;

        assert!(!should_replace_cached_height(
            22.0 * cached / 2_276.0,
            cached,
            2_276
        ));
        assert!(!should_replace_cached_height(
            2_276.0 * cached / 22.0,
            cached,
            22
        ));
    }

    #[test]
    fn the_first_measurement_is_always_believed() {
        assert!(should_replace_cached_height(34.0, 0.0, 200));
        assert!(!should_replace_cached_height(0.0, 0.0, 200));
        assert!(!should_replace_cached_height(-1.0, 34.0, 200));
        assert!(!should_replace_cached_height(24.0, 34.0, 0));
    }

    #[test]
    fn a_forgotten_height_is_measured_again_from_scratch() {
        let cached = Cell::new(34.0);

        forget_row_height(&cached);

        assert_eq!(
            cached.get(),
            crate::ui::list_geometry::INVALIDATED_ROW_HEIGHT
        );
        assert!(should_replace_cached_height(24.0, cached.get(), 200));
    }

    #[test]
    fn restore_height_prefers_the_cache_over_the_live_geometry() {
        let cached = Cell::new(34.0);

        assert_eq!(row_height_for_restore(&cached, 748.0, 2_276), Some(34.0));
    }

    #[test]
    fn restore_height_falls_back_to_the_live_geometry_without_a_cache() {
        let cached = Cell::new(0.0);

        assert_eq!(row_height_for_restore(&cached, 6_800.0, 200), Some(34.0));
    }

    #[test]
    fn restore_height_needs_both_rows_and_a_measurable_upper() {
        let cached = Cell::new(34.0);

        assert_eq!(row_height_for_restore(&cached, 6_800.0, 0), None);
        assert_eq!(row_height_for_restore(&cached, 0.0, 200), None);
        assert_eq!(row_height_for_restore(&cached, -1.0, 200), None);
    }
}
