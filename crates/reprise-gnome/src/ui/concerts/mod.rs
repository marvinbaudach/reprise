//! Concerts full-view composition boundary.

use std::cell::RefCell;
use std::rc::Rc;

use rusqlite::Connection;

mod concerts_columns;
mod concerts_empty_state;
mod concerts_filter_bar;
mod concerts_model;
mod concerts_presentation;
mod concerts_view;
pub(super) mod css;

pub(in crate::ui) use concerts_view::ConcertsView;

#[allow(dead_code)]
pub(in crate::ui) fn install(conn: Rc<RefCell<Connection>>) -> ConcertsView {
    ConcertsView::new(conn)
}
