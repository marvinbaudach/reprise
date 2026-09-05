//! Concerts full-view composition boundary.

use std::rc::Rc;

use reprise_core::db::Db;

mod concerts_activation;
mod concerts_artist_cover;
mod concerts_column_layout;
mod concerts_columns;
mod concerts_empty_state;
mod concerts_end_of_results;
mod concerts_failure_ui;
mod concerts_filter_bar;
mod concerts_location_banner;
mod concerts_location_columns;
mod concerts_model;
pub(in crate::ui) mod concerts_presentation;
mod concerts_search;
mod concerts_sorting;
mod concerts_status_cells;
mod concerts_view;
mod concerts_view_refresh;
mod concerts_view_render;
mod concerts_view_state;
mod concerts_worker;
pub(super) mod css;

pub(in crate::ui) use concerts_view::ConcertsView;
pub(in crate::ui) use concerts_worker::{ConcertsProgress, ConcertsRequest, ConcertsRuntime};

pub(in crate::ui) fn install(
    conn: Rc<Db>,
    runtime: &Rc<ConcertsRuntime>,
    location_broadcast: &Rc<crate::ui::location_broadcast::LocationBroadcast>,
) -> ConcertsView {
    ConcertsView::new(conn, runtime, location_broadcast)
}
