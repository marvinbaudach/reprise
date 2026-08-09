//! Small geometry helpers shared by list reload and tag-save anchoring.

use std::cell::Cell;

use gtk4::prelude::AdjustmentExt;

/// Approximates the uniform row height from the adjustment's total content
/// height over the row count — the same technique `current_track_selection::
/// centered_scroll_value` uses for the "jump to now playing" center (NAV-9b):
/// `GtkColumnView` rows are uniform height by design, and there is no
/// per-row height API to query instead.
pub(in crate::ui) fn row_height(column_view: &gtk4::ColumnView, n_rows: u32) -> Option<f64> {
    if n_rows == 0 {
        return None;
    }
    let adjustment = gtk4::prelude::ScrollableExt::vadjustment(column_view)?;
    let upper = adjustment.upper();
    (upper > 0.0).then(|| upper / f64::from(n_rows))
}

/// Remembers a row height only while there is enough geometry to measure it.
pub(in crate::ui) fn remember_row_height(
    column_view: &gtk4::ColumnView,
    n_rows: u32,
    last_row_height: &Cell<f64>,
) {
    if let Some(height) = row_height(column_view, n_rows) {
        let cached = last_row_height.get();
        if cached <= 0.0
            || restore_geometry_is_ready(height * f64::from(n_rows), n_rows as usize, cached)
        {
            last_row_height.set(height);
        }
    }
}

/// Uses the last consistent measurement across a model swap, falling back to
/// the live geometry during the first allocation.
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
        upper / n_rows as f64
    })
}

/// The adjustment is ready only once it describes the same rows as the model.
pub(in crate::ui) fn restore_geometry_is_ready(upper: f64, n_rows: usize, height: f64) -> bool {
    (upper - n_rows as f64 * height).abs() <= height
}
