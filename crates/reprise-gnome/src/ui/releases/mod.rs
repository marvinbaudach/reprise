//! Releases full-view composition boundary.

use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::db::Db;

pub(super) mod css;
mod releases_cell_surface;
mod releases_column_layout;
mod releases_columns;
mod releases_cover_column;
mod releases_empty_state;
mod releases_failure_ui;
mod releases_filter_bar;
mod releases_model;
pub(in crate::ui) mod releases_presentation;
mod releases_selection;
mod releases_view;

pub(in crate::ui) use releases_view::ReleasesView;

#[allow(dead_code)]
pub(in crate::ui) fn install(conn: Rc<Db>, database_path: PathBuf) -> ReleasesView {
    ReleasesView::new(conn, database_path)
}
