//! Small geometry helpers shared by list reload and tag-save anchoring.

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
