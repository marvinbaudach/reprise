//! Concerts full-view composition boundary.

use std::rc::Rc;

use reprise_core::db::Db;

mod concerts_columns;
mod concerts_empty_state;
mod concerts_filter_bar;
mod concerts_model;
mod concerts_presentation;
mod concerts_view;
mod concerts_worker;
pub(super) mod css;

pub(in crate::ui) use concerts_view::ConcertsView;
pub(in crate::ui) use concerts_worker::{ConcertsRequest, ConcertsRuntime};

#[allow(dead_code)]
pub(in crate::ui) fn install(conn: Rc<Db>, runtime: &Rc<ConcertsRuntime>) -> ConcertsView {
    ConcertsView::new(conn, runtime)
}
