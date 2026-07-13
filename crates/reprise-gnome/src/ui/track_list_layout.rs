use super::column_layout::{self, ColumnId, ColumnLayout};
use super::track_list::TrackList;

impl TrackList {
    pub(super) fn apply_column_layout(&self, layout: &ColumnLayout) -> Result<(), rusqlite::Error> {
        let serialized = column_layout::serialize_layout(layout);
        reprise_core::library::settings::set_setting(
            &self.shared.conn.borrow(),
            reprise_core::library::settings::COLUMN_LAYOUT_KEY,
            &serialized,
        )?;
        self.column_registry.apply(layout);
        super::column_header_menu::sync(self, layout);
        let sort = self.shared.sort.borrow().clone();
        let current_id = ColumnId::from_sort_field(&sort.field);
        let (column, order) = if current_id.is_some_and(|id| self.column_registry.is_visible(id)) {
            let column = current_id.and_then(|id| self.column_registry.column(id));
            let order = if sort.dir == "desc" {
                gtk4::SortType::Descending
            } else {
                gtk4::SortType::Ascending
            };
            (column, order)
        } else {
            (
                self.column_registry.column(ColumnId::Title),
                gtk4::SortType::Ascending,
            )
        };
        if let Some(column) = column {
            self.shared.column_view.sort_by_column(Some(column), order);
        }
        Ok(())
    }

    pub(super) fn current_column_layout(&self) -> ColumnLayout {
        column_layout::load_layout(&self.shared.conn.borrow())
    }
}
