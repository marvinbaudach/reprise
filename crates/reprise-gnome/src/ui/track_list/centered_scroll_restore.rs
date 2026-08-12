//! Deferred centering for playing-track reveals.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::{AdjustmentExt, ScrollableExt};

use super::Shared;
use crate::ui::list_geometry::ListGeometry;

pub(super) fn schedule(shared: &Rc<Shared>, track_id: Option<i64>, current_ids: Vec<i64>) {
    let anchor = track_id.map(|track_id| (track_id, 0.0));
    let Some(position) = super::reload_restore::prepaint_position(anchor, &current_ids) else {
        return;
    };
    if apply(shared, track_id, &current_ids) {
        return;
    }
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return;
    };
    let generation = shared.model.generation();
    let applied = Rc::new(Cell::new(false));

    let weak_shared = Rc::downgrade(shared);
    let changed_ids = current_ids.clone();
    let changed_applied = applied.clone();
    crate::ui::list_geometry_changed::after_changed_once(&adjustment, move || {
        if changed_applied.get() {
            return;
        }
        let Some(shared) = weak_shared.upgrade() else {
            return;
        };
        if shared.model.generation() == generation && apply(&shared, track_id, &changed_ids) {
            changed_applied.set(true);
        }
    });

    // A stable adjustment range emits no `changed`. Its row allocation still
    // settles in the pending redraw, so refine once after that frame as well.
    let weak_shared = Rc::downgrade(shared);
    gtk4::glib::idle_add_local_once(move || {
        if applied.get() {
            return;
        }
        let Some(shared) = weak_shared.upgrade() else {
            return;
        };
        if shared.model.generation() == generation && apply(&shared, track_id, &current_ids) {
            applied.set(true);
        }
    });

    let scroll = gtk4::ScrollInfo::new();
    scroll.set_enable_vertical(true);
    shared
        .column_view
        .scroll_to(position, None, gtk4::ListScrollFlags::NONE, Some(scroll));
}

fn apply(shared: &Shared, track_id: Option<i64>, current_ids: &[i64]) -> bool {
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return false;
    };
    let page = adjustment.page_size();
    if adjustment.upper() <= page {
        return true;
    }
    let Some(height) = ListGeometry::for_view(&shared.column_view)
        .live_row_height(current_ids.len())
        .map(crate::ui::list_geometry::RowHeight::pixels)
    else {
        return false;
    };
    let Some(value) =
        super::reload_restore::centered_track_scroll_target(track_id, current_ids, height, page)
    else {
        return false;
    };
    crate::ui::scroll_probe::probe("centered_refinement", &adjustment, value);
    debug_assert!(
        !crate::ui::list_geometry_changed::in_changed_emission(),
        "centered scroll written from inside a changed emission"
    );
    adjustment.set_value(value);
    true
}
