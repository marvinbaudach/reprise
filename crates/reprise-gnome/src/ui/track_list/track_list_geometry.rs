//! TrackList adapters for the view-neutral list geometry service.

use std::rc::Rc;

use gtk4::gio::prelude::ListModelExt;
use gtk4::prelude::{AdjustmentExt, ScrollableExt};

use crate::ui::list_geometry::ListGeometry;

use super::Shared;

/// Marks the per-view geometry cache so `ListGeometry` discards the persisted
/// value for the density that becomes active immediately after this call.
pub(in crate::ui) fn forget_row_height(cache: &crate::ui::list_geometry::ListGeometryCache) {
    cache.invalidate();
}

/// Explicitly warms both geometry caches at the first post-swap allocation.
///
/// Sectioned lists cannot prove their header height in the pre-seed window:
/// their bound `ListHeader` widgets still report zero there. The adjustment's
/// next `changed` signal is the first existing restore seam after layout, so a
/// uniform, settled Queue records its row and header together before the
/// callback retries the anchor.
pub(in crate::ui) fn remember_after_layout(shared: &Shared, n_rows: usize) -> bool {
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return false;
    };
    let n_sections = shared.queue_sections.borrow().len();
    ListGeometry::for_view(&shared.column_view).remember_if_settled(
        &shared.conn,
        &shared.list_geometry_cache,
        adjustment.upper(),
        n_rows,
        n_sections,
    )
}

pub(in crate::ui) fn schedule_section_measurement(shared: &Rc<Shared>) {
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return;
    };
    let weak_shared = Rc::downgrade(shared);
    crate::ui::list_geometry::on_changed_once(&adjustment, move |_| {
        let Some(shared) = weak_shared.upgrade() else {
            return;
        };
        if !remember_after_layout(&shared, shared.model.n_items() as usize) {
            // `configure()` itself emits `changed` in the pre-layout window.
            // Re-arm after that early signal so the later allocation change,
            // where bound headers have non-zero heights, remains observable.
            schedule_section_measurement(&shared);
        }
    });
}
