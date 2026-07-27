use super::column_layout::{self, ColumnId, ColumnLayout};
use super::TrackList;

pub(in crate::ui) const STACK_PAGE_EMPTY: &str = "empty";
pub(in crate::ui) const STACK_PAGE_LIST: &str = "list";
/// Stage 3 Task 8: the ImportErrors source's dedicated path/reason/time panel
/// (`ui::import_errors_view::ImportErrorsView`) — a third `gtk::Stack` page,
/// shown instead of `STACK_PAGE_LIST` only while `ViewSource::ImportErrors`
/// is selected and has rows (see `apply_empty_state`'s `List` arm).
pub(in crate::ui) const STACK_PAGE_IMPORT_ERRORS: &str = "import_errors";
pub(in crate::ui) const STACK_PAGE_MISSING: &str = "missing";

/// Keeps the track-content viewport filling the window from its first layout.
/// The initially selected empty page and the later list page have different
/// natural heights, so relying on child-derived expansion can leave the stack
/// at a single-row height until a source switch queues another allocation.
pub(in crate::ui) fn build_track_content_stack() -> gtk4::Stack {
    // Size to the visible page, not the widest: a homogeneous stack would make
    // the list page inherit the widest of the empty/import-errors/missing
    // pages' minimum width (and vice versa), inflating the content's minimum
    // (QA #3/#4).
    gtk4::Stack::builder()
        .vexpand(true)
        .hhomogeneous(false)
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .transition_duration(crate::ui::motion::STANDARD_MS)
        .build()
}

impl TrackList {
    pub(in crate::ui) fn column_view_widget(&self) -> &gtk4::ColumnView {
        &self.shared.column_view
    }

    pub(in crate::ui) fn apply_column_layout(
        &self,
        layout: &ColumnLayout,
    ) -> Result<(), rusqlite::Error> {
        let serialized = column_layout::serialize_layout(layout);
        reprise_core::library::settings::set_setting(
            &self.shared.conn.borrow(),
            reprise_core::library::settings::COLUMN_LAYOUT_KEY,
            &serialized,
        )?;
        self.column_registry.apply(layout);
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
            super::track_list_sort::sort_by_column(&self.shared.column_view, column, order);
        }
        Ok(())
    }

    pub(in crate::ui) fn current_column_layout(&self) -> ColumnLayout {
        column_layout::load_layout(&self.shared.conn.borrow())
    }

    /// Restores every column to its built-in default width; the wired
    /// `fixed-width` listeners persist the change.
    pub(in crate::ui) fn reset_column_widths(&self) {
        self.column_registry.reset_widths();
    }
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn track_content_stack_expands_from_initial_layout() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }
        let stack = super::build_track_content_stack();
        assert_eq!(
            stack.transition_type(),
            gtk4::StackTransitionType::Crossfade
        );
        assert_eq!(stack.transition_duration(), crate::ui::motion::STANDARD_MS);
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
