//! Main-window binding for the music table's shared header interactions.

use std::rc::Rc;

use crate::ui::track_list::TrackList;

pub(super) fn install(track_list: &Rc<TrackList>) {
    let model = crate::ui::column_layout::model(track_list);
    crate::ui::table_columns::header_popover::install_header_popover(
        track_list.column_view_widget(),
        &model,
    );
    crate::ui::table_columns::header_dnd::install_header_drag(
        track_list.column_view_widget(),
        &model,
    );
}
