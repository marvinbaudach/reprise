//! Density-change adapter retained for the existing TrackList entry point.

use std::cell::Cell;

/// Marks the per-view geometry cache so `ListGeometry` discards the persisted
/// value for the density that becomes active immediately after this call.
pub(in crate::ui) fn forget_row_height(last_row_height: &Cell<f64>) {
    crate::ui::list_geometry::invalidate_row_height(last_row_height);
}
