//! Precise, generation-guarded viewport restoration after a model reload.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::{AdjustmentExt, ScrollableExt};

use super::{reload_restore, Shared};
use crate::ui::adjustment_hold::AdjustmentHold;
use crate::ui::list_geometry::{ListGeometry, RowHeight};

pub(super) fn schedule(
    shared: &Rc<Shared>,
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &[i64],
    hold: Option<&AdjustmentHold>,
) {
    let Some(anchor_position) = reload_restore::prepaint_position(anchor, current_ids) else {
        return;
    };
    let applied = apply(shared, anchor, captured_row_height, current_ids, hold);
    if !applied {
        arm_refinement(shared, anchor, captured_row_height, current_ids, hold);
    }

    let guard_position = if applied {
        let page = shared
            .column_view
            .vadjustment()
            .map(|value| value.page_size());
        captured_row_height
            .zip(page)
            .and_then(|(height, page)| {
                reload_restore::prepaint_guard_position(anchor, current_ids, height.pixels(), page)
            })
            .unwrap_or(anchor_position)
    } else {
        anchor_position
    };
    let scroll = gtk4::ScrollInfo::new();
    scroll.set_enable_vertical(true);
    shared.column_view.scroll_to(
        guard_position,
        None,
        gtk4::ListScrollFlags::NONE,
        Some(scroll),
    );
}

fn arm_refinement(
    shared: &Rc<Shared>,
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &[i64],
    hold: Option<&AdjustmentHold>,
) {
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return;
    };
    let generation = shared.model.generation();
    let restored = Rc::new(Cell::new(false));

    let weak_shared = Rc::downgrade(shared);
    let changed_ids = current_ids.to_owned();
    let changed_hold = hold.cloned();
    let changed_restored = restored.clone();
    crate::ui::list_geometry_changed::after_changed_once(&adjustment, move || {
        if changed_restored.get() {
            return;
        }
        let Some(shared) = weak_shared.upgrade() else {
            return;
        };
        if shared.model.generation() != generation {
            return;
        }
        super::track_list_geometry::remember_after_layout(&shared, changed_ids.len());
        if apply(
            &shared,
            anchor,
            captured_row_height,
            &changed_ids,
            changed_hold.as_ref(),
        ) {
            changed_restored.set(true);
        }
    });

    // A stable range emits no `changed`; its rows still settle in the pending
    // redraw, so make one post-redraw attempt as the complementary path.
    let weak_shared = Rc::downgrade(shared);
    let idle_ids = current_ids.to_owned();
    let idle_hold = hold.cloned();
    gtk4::glib::idle_add_local_once(move || {
        if restored.get() {
            return;
        }
        let Some(shared) = weak_shared.upgrade() else {
            return;
        };
        if shared.model.generation() == generation
            && apply(
                &shared,
                anchor,
                captured_row_height,
                &idle_ids,
                idle_hold.as_ref(),
            )
        {
            restored.set(true);
        }
    });
}

fn apply(
    shared: &Shared,
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &[i64],
    hold: Option<&AdjustmentHold>,
) -> bool {
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return false;
    };
    crate::ui::scroll_probe::probe_rows("apply_scroll_anchor", &shared.column_view);
    if current_ids.is_empty() {
        return false;
    }
    let n_sections = shared.queue_sections.borrow().len();
    let geometry = ListGeometry::for_view(&shared.column_view);
    let Some(height) = captured_row_height
        .or_else(|| {
            geometry.observed_row_height(
                &shared.conn,
                &shared.list_geometry_cache,
                current_ids.len(),
                n_sections,
            )
        })
        .map(RowHeight::pixels)
    else {
        return false;
    };
    let Some(target) =
        reload_restore::scroll_target(anchor, current_ids, height, adjustment.page_size())
    else {
        return false;
    };
    if let Some(hold) = hold {
        hold.set_target(target);
    }
    if !crate::ui::scroll_probe::preseed_suppressed() {
        geometry.configure(
            &adjustment,
            target,
            &shared.conn,
            &shared.list_geometry_cache,
            current_ids.len(),
            n_sections,
        );
    }
    if !geometry.is_settled(adjustment.upper(), current_ids.len(), n_sections) {
        return false;
    }
    geometry.remember_if_settled(
        &shared.conn,
        &shared.list_geometry_cache,
        adjustment.upper(),
        current_ids.len(),
        n_sections,
    );
    crate::ui::scroll_probe::probe("anchor", &adjustment, target);
    debug_assert!(
        !crate::ui::list_geometry_changed::in_changed_emission(),
        "scroll anchor written from inside a changed emission"
    );
    adjustment.set_value(target);
    true
}
