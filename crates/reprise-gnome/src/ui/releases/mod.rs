//! Releases full-view composition boundary.

use std::cell::RefCell;
use std::rc::Rc;

use rusqlite::Connection;

pub(in crate::ui) type OnShowAlbum = Rc<dyn Fn(&str, &str)>;
pub(super) mod css;
mod releases_columns;
mod releases_empty_state;
mod releases_model;
mod releases_presentation;
mod releases_view;

pub(in crate::ui) use releases_view::ReleasesView;

#[allow(dead_code)]
pub(in crate::ui) fn install(
    conn: Rc<RefCell<Connection>>,
    on_show_album: OnShowAlbum,
) -> ReleasesView {
    ReleasesView::new(conn, on_show_album)
}
