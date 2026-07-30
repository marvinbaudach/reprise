//! Releases full-view composition boundary.

use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::db::Db;

pub(super) mod css;
mod releases_columns;
mod releases_empty_state;
mod releases_filter_bar;
mod releases_model;
mod releases_presentation;
mod releases_view;

pub(in crate::ui) use releases_view::ReleasesView;

#[allow(dead_code)]
pub(in crate::ui) fn install(conn: Rc<Db>, database_path: PathBuf) -> ReleasesView {
    ReleasesView::new(conn, database_path)
}
