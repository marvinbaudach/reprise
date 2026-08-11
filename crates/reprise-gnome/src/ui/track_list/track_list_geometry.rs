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

/// How often the measurement may re-arm itself before giving up.
///
/// The first `changed` is spent on the pre-seed's own `configure()`, the
/// second is the allocation that realizes the headers. The remaining two
/// absorb an extra range change (a resize landing between the two) without
/// letting a Queue that never settles reconnect on every future `changed` for
/// the widget's lifetime. Giving up is safe: the pre-seed's assumed-token
/// branch carries the restore without a measured header height — it is what
/// the accepted measurement actually ran on.
const SECTION_MEASUREMENT_MAX_ATTEMPTS: u8 = 4;

pub(in crate::ui) fn schedule_section_measurement(shared: &Rc<Shared>) {
    schedule_section_measurement_attempt(shared, SECTION_MEASUREMENT_MAX_ATTEMPTS);
}

fn schedule_section_measurement_attempt(shared: &Rc<Shared>, attempts_left: u8) {
    let Some(attempts_left) = attempts_left.checked_sub(1) else {
        return;
    };
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return;
    };
    let weak_shared = Rc::downgrade(shared);
    // A pending subscription outlives the view it measured: switching source
    // while it waits would otherwise let it persist this Queue's measurement
    // against whatever list is showing now. The model's generation changes on
    // every reload, so an older arming simply stops here.
    let generation = shared.model.generation();
    crate::ui::list_geometry_changed::on_changed_once(&adjustment, move |_| {
        let Some(shared) = weak_shared.upgrade() else {
            return;
        };
        if shared.model.generation() != generation {
            return;
        }
        // Deliberately synchronous: `remember_after_layout` only writes the
        // geometry caches and settings, never the adjustment, so it is safe
        // inside a `changed` emission. Anything that writes the adjustment
        // must defer — see `crate::ui::list_geometry_changed::in_changed_emission`.
        if !remember_after_layout(&shared, shared.model.n_items() as usize) {
            // `configure()` itself emits `changed` in the pre-layout window.
            // Re-arm after that early signal so the later allocation change,
            // where bound headers have non-zero heights, remains observable.
            schedule_section_measurement_attempt(&shared, attempts_left);
        }
    });
}
