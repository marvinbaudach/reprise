use reprise_core::library::settings::ListDensity;

use super::column_layout::{self, ColumnLayout};
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
        .vhomogeneous(false)
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .transition_duration(crate::ui::motion::STANDARD_MS)
        .build()
}

impl TrackList {
    pub(in crate::ui) fn column_view_widget(&self) -> &gtk4::ColumnView {
        &self.shared.column_view
    }

    /// Projects a display density onto the table. The one entry point for it,
    /// because the cached row height the scroll restore relies on describes
    /// the *old* density from here on: nothing about the geometry looks wrong
    /// afterwards, so only this event can invalidate it.
    pub(in crate::ui) fn apply_list_density(&self, density: ListDensity) {
        super::track_list_geometry::forget_row_height(&self.shared.list_geometry_cache);
        super::list_density::apply(&self.shared.column_view, density);
    }

    pub(in crate::ui) fn current_column_layout(&self) -> ColumnLayout {
        column_layout::load_layout(&self.shared.conn)
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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_5_hidden_tall_track_page_cannot_push_the_player_out_of_view() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        if gtk4::init().is_err() {
            return;
        }
        let stack = super::build_track_content_stack();
        let visible_page = gtk4::Label::new(Some("visible tracks"));
        let hidden_page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        hidden_page.set_height_request(1_200);
        stack.add_named(&visible_page, Some("list"));
        stack.add_named(&hidden_page, Some("hidden"));
        stack.set_visible_child_name("list");

        let player = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        player.set_height_request(86);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&stack);
        root.append(&player);
        let window = gtk4::Window::builder()
            .default_width(1_200)
            .default_height(420)
            .child(&root)
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(
            !stack.is_vhomogeneous(),
            "hidden track pages must not define the visible page height"
        );
        let player_bounds = player
            .compute_bounds(&window)
            .expect("player must share the window coordinate space");
        assert!(
            player_bounds.y() + player_bounds.height() <= window.height() as f32,
            "player ended at {} outside the {} px window",
            player_bounds.y() + player_bounds.height(),
            window.height()
        );
        window.close();
    }
}
