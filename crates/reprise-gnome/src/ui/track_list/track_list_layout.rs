use super::column_layout::{self, ColumnId, ColumnLayout};
use super::track_list::TrackList;

/// Keeps the track-content viewport filling the window from its first layout.
/// The initially selected empty page and the later list page have different
/// natural heights, so relying on child-derived expansion can leave the stack
/// at a single-row height until a source switch queues another allocation.
pub(super) fn build_track_content_stack() -> gtk4::Stack {
    gtk4::Stack::builder().vexpand(true).build()
}

impl TrackList {
    pub(super) fn column_view_widget(&self) -> &gtk4::ColumnView {
        &self.shared.column_view
    }

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

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn track_content_stack_expands_from_initial_layout() {
        if gtk4::init().is_err() {
            return;
        }
        let stack = super::build_track_content_stack();
        stack.add_named(&gtk4::Label::new(Some("first track")), Some("list"));
        stack.set_visible_child_name("list");

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&gtk4::Label::new(Some("filter bar")));
        root.append(&stack);
        let window = gtk4::Window::builder()
            .default_width(600)
            .default_height(400)
            .child(&root)
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(stack.vexpands());
        assert!(stack.height() > 300, "stack height was {}", stack.height());
        window.close();
    }
}
