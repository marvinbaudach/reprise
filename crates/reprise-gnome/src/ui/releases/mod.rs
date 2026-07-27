//! Releases full-view composition boundary.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use rusqlite::Connection;

pub(super) mod css;
mod releases_columns;
mod releases_empty_state;
mod releases_filter_bar;
mod releases_model;
mod releases_presentation;
mod releases_view;

pub(in crate::ui) use releases_view::ReleasesView;

#[allow(dead_code)]
pub(in crate::ui) fn install(
    conn: Rc<RefCell<Connection>>,
    database_path: PathBuf,
) -> ReleasesView {
    ReleasesView::new(conn, database_path)
}
