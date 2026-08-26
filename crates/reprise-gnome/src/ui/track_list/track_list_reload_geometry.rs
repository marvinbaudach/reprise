use gtk4::prelude::*;

use crate::ui::list_geometry::RowHeight;

use super::super::Shared;
use super::observed_row_height;

/// Before the model swap, unlike during restore, the adjustment range and row
/// count describe the same list. Capture that exact quotient so a later stale
/// widget allocation or an assumed CSS floor cannot reinterpret the anchor.
pub(super) fn capture_row_height(shared: &Shared, old_total: u32) -> Option<RowHeight> {
    if old_total == 0 {
        return None;
    }
    if shared.queue_sections.borrow().is_empty() {
        let adjustment = shared.column_view.vadjustment()?;
        if !crate::ui::list_geometry::gtk_authored(
            &shared.list_geometry_cache,
            adjustment.upper(),
            old_total as usize,
        ) {
            return None;
        }
        return RowHeight::new(adjustment.upper() / f64::from(old_total));
    }
    observed_row_height(shared, old_total).and_then(RowHeight::new)
}
