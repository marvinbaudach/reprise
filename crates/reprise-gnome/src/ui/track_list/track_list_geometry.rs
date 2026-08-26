//! TrackList adapters for the view-neutral list geometry service.

use std::rc::Rc;

use gtk4::gio::prelude::ListModelExt;
use gtk4::prelude::{AdjustmentExt, ScrollableExt};

use crate::ui::list_geometry::{ListGeometry, RowHeight};
use crate::ui::list_geometry_layout::{LayoutValidation, ListLayout};

use super::Shared;

/// Builds the complete row/header layout, using the live adjustment only when
/// the layout contains section headers. Section starts are copied before any
/// GTK or database call so no `RefCell` borrow crosses a re-entrant boundary.
pub(in crate::ui) fn layout(
    shared: &Shared,
    captured_row_height: Option<RowHeight>,
    n_rows: usize,
) -> Option<ListLayout> {
    let section_starts = shared
        .queue_sections
        .borrow()
        .iter()
        .map(|section| section.start)
        .collect::<Vec<_>>();
    let n_sections = section_starts.len();
    let adjustment = shared.column_view.vadjustment()?;
    let geometry = ListGeometry::for_view(&shared.column_view);
    let row_height = captured_row_height.or_else(|| {
        geometry.observed_row_height(
            &shared.conn,
            &shared.list_geometry_cache,
            n_rows,
            n_sections,
        )
    })?;
    let layout = geometry.layout(
        &shared.conn,
        &shared.list_geometry_cache,
        row_height,
        section_starts,
        adjustment.upper(),
        n_rows,
    );
    Some(layout_for_live_allocation(
        layout,
        n_rows,
        adjustment.upper(),
    ))
}

/// Keeps useful anchor geometry for every validation outcome. An allocation
/// that cannot yet judge the header guess keeps the complete layout; a proven
/// disagreement falls back to the rows-only arithmetic used before section
/// geometry existed, so callers never lose the anchor entirely.
fn layout_for_live_allocation(layout: ListLayout, n_rows: usize, upper: f64) -> ListLayout {
    if !layout.has_sections() {
        return layout;
    }
    match layout.validate(n_rows, upper) {
        LayoutValidation::Accepted | LayoutValidation::NoOpinion => layout,
        LayoutValidation::Rejected => ListLayout::rows_only(layout.row_height()),
    }
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
    shared.diagnostic_trail.record(
        super::diagnostic_trail::Event::GeometryMeasurementScheduled {
            generation: shared.model.generation(),
            rows: shared.model.n_items(),
            sections: shared.queue_sections.borrow().len(),
        },
    );
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

#[cfg(test)]
mod tests {
    use crate::ui::list_geometry::RowHeight;
    use crate::ui::list_geometry_layout::ListLayout;

    use super::layout_for_live_allocation;

    fn height(pixels: f64) -> RowHeight {
        RowHeight::new(pixels).unwrap()
    }

    #[test]
    fn rejected_section_geometry_falls_back_but_no_opinion_keeps_the_anchor_model() {
        let sectioned = ListLayout::sectioned(height(34.0), height(36.0), vec![0, 1]);

        let rejected = layout_for_live_allocation(sectioned.clone(), 2_276, 77_464.0);
        assert_eq!(rejected, ListLayout::rows_only(height(34.0)));

        let unsettled = layout_for_live_allocation(sectioned.clone(), 2_276, 77_438.0);
        assert_eq!(unsettled, sectioned);

        let unsectioned = ListLayout::rows_only(height(34.0));
        assert_eq!(
            layout_for_live_allocation(unsectioned.clone(), 2_276, 748.0),
            unsectioned,
            "an unsectioned layout never consults allocation validation"
        );
    }
}
