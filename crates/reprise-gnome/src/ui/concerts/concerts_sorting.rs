//! Column sort wiring and row ordering for the Concerts view.

use std::rc::Rc;

use gtk4::prelude::*;

use super::concerts_presentation::{sort_key_for_id, sort_rows, SortDirection};
use super::concerts_view_state::Shared;

pub(super) fn wire_sorting(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let Some(sorter) = column_view
        .sorter()
        .and_downcast::<gtk4::ColumnViewSorter>()
    else {
        tracing::warn!("concerts table has no ColumnViewSorter");
        return;
    };
    {
        let shared = shared.clone();
        sorter.connect_primary_sort_column_notify(move |sorter| apply_sort(&shared, sorter));
    }
    {
        let shared = shared.clone();
        sorter.connect_primary_sort_order_notify(move |sorter| apply_sort(&shared, sorter));
    }
}

fn apply_sort(shared: &Shared, sorter: &gtk4::ColumnViewSorter) {
    let Some(column) = sorter.primary_sort_column() else {
        return;
    };
    let Some(key) = sort_key_for_id(column.id().as_deref()) else {
        return;
    };
    let direction = if sorter.primary_sort_order() == gtk4::SortType::Descending {
        SortDirection::Descending
    } else {
        SortDirection::Ascending
    };
    let mut rows = shared.rows.borrow().clone();
    sort_rows(&mut rows, key, direction);
    shared.model.replace(rows);
}
