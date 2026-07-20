//! Keyboard-focus routing for the track content stack.

use gtk4::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrackContentFocusTarget {
    ColumnView,
    VisiblePage,
}

fn target_for_page(visible_page: Option<&str>) -> TrackContentFocusTarget {
    match visible_page {
        Some(super::STACK_PAGE_LIST) => TrackContentFocusTarget::ColumnView,
        _ => TrackContentFocusTarget::VisiblePage,
    }
}

fn focus_visible_page(page: &gtk4::Widget) -> bool {
    if let Some(scrolled) = page.downcast_ref::<gtk4::ScrolledWindow>() {
        // GtkScrolledWindow's focus implementation prefers the scroll surface
        // itself, even when a focusable child is requested explicitly. Keep
        // that fallback available, but take it out of the focus chain while
        // entering the visible page's actual content.
        let scroller_was_focusable = scrolled.is_focusable();
        scrolled.set_focusable(false);
        let focused_content = scrolled
            .child()
            .is_some_and(|content| focus_widget_or_descendant(&content));
        scrolled.set_focusable(scroller_was_focusable);
        return focused_content || scrolled.grab_focus();
    }
    page.child_focus(gtk4::DirectionType::TabForward) || page.grab_focus()
}

fn focus_widget_or_descendant(widget: &gtk4::Widget) -> bool {
    if widget.is_focusable()
        && widget.root().is_some_and(|root| {
            let Ok(window) = root.downcast::<gtk4::Window>() else {
                return false;
            };
            // ScrolledWindow may otherwise retain focus even when its direct
            // child accepts grab_focus(). Set and verify the leaf explicitly.
            gtk4::prelude::GtkWindowExt::set_focus(&window, Some(widget));
            gtk4::prelude::GtkWindowExt::focus(&window).as_ref() == Some(widget)
        })
    {
        return true;
    }
    widget.child_focus(gtk4::DirectionType::TabForward) || widget.grab_focus()
}

pub(super) fn focus_visible_content(
    stack: &gtk4::Stack,
    column_view: &impl IsA<gtk4::Widget>,
) -> bool {
    match target_for_page(stack.visible_child_name().as_deref()) {
        TrackContentFocusTarget::ColumnView => column_view.grab_focus(),
        TrackContentFocusTarget::VisiblePage => stack
            .visible_child()
            .is_some_and(|child| focus_visible_page(&child)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::track_list::{
        STACK_PAGE_EMPTY, STACK_PAGE_IMPORT_ERRORS, STACK_PAGE_LIST, STACK_PAGE_MISSING,
    };

    #[test]
    fn acc_5_dedicated_sources_focus_the_visible_surface() {
        assert_eq!(
            target_for_page(Some(STACK_PAGE_LIST)),
            TrackContentFocusTarget::ColumnView
        );
        for page in [
            STACK_PAGE_MISSING,
            STACK_PAGE_IMPORT_ERRORS,
            STACK_PAGE_EMPTY,
        ] {
            assert_eq!(
                target_for_page(Some(page)),
                TrackContentFocusTarget::VisiblePage
            );
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_5_dedicated_source_focus_never_targets_the_hidden_track_table() {
        gtk4::init().unwrap();
        let stack = gtk4::Stack::new();
        let track_table = gtk4::Button::with_label("Track table");
        let missing_page = gtk4::ScrolledWindow::new();
        let auto_clean = gtk4::Button::with_label("Auto-clean");
        missing_page.set_child(Some(&auto_clean));
        stack.add_named(&track_table, Some(STACK_PAGE_LIST));
        stack.add_named(&missing_page, Some(STACK_PAGE_MISSING));
        stack.set_visible_child_name(STACK_PAGE_MISSING);
        let window = gtk4::Window::builder().child(&stack).build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(focus_visible_content(&stack, &track_table));
        assert_eq!(
            gtk4::prelude::GtkWindowExt::focus(&window).as_ref(),
            Some(auto_clean.upcast_ref())
        );
        assert!(
            missing_page.is_focusable(),
            "scroller fallback must be restored"
        );

        window.close();
    }
}
