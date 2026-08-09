//! Music-table entry points for the shared column editor.

use std::rc::Rc;

use libadwaita as adw;

use crate::ui::table_columns;
use crate::ui::track_list::TrackList;

pub(in crate::ui) const SMOKE_ENV: &str = "REPRISE_SMOKE_COLUMN_LAYOUT_EDITOR";

pub(in crate::ui) fn css() -> String {
    table_columns::editor_dnd::css()
}

pub(in crate::ui) fn build_navigation_page(track_list: &Rc<TrackList>) -> adw::NavigationPage {
    table_columns::editor::build_navigation_page(&super::column_layout::model(track_list))
}
