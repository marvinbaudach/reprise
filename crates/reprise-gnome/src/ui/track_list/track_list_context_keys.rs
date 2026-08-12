//! Keyboard access to the track-list context menu.

use gtk4::gdk;
use gtk4::prelude::*;

use super::popover_lifecycle;
use super::track_list_context_menu;
use super::Shared;

/// Shared with the album grid's keyboard context menu
/// (`album_view_actions`): Menu key or Shift+F10, per GNOME convention.
pub(in crate::ui) fn is_context_menu_shortcut(key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
    key == gdk::Key::Menu
        || (key == gdk::Key::F10 && modifiers.contains(gdk::ModifierType::SHIFT_MASK))
}

pub(in crate::ui) fn wire(column_view: &gtk4::ColumnView, shared: &std::rc::Rc<Shared>) {
    let controller = gtk4::EventControllerKey::new();
    let column_view_handle = column_view.clone();
    let shared = shared.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        if !is_context_menu_shortcut(key, modifiers) {
            return gtk4::glib::Propagation::Proceed;
        }
        if track_list_context_menu::current_selection_positions(&shared).is_empty() {
            return gtk4::glib::Propagation::Proceed;
        }

        let menu = track_list_context_menu::build_context_menu_model(&shared);
        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        present_keyboard_popover(&column_view_handle, &popover);
        tracing::debug!("track context menu opened from keyboard");
        gtk4::glib::Propagation::Stop
    });
    column_view.add_controller(controller);
}

pub(super) fn present_keyboard_popover(
    column_view: &gtk4::ColumnView,
    popover: &gtk4::PopoverMenu,
) {
    popover.set_parent(column_view);
    popover.set_has_arrow(false);
    let target = gdk::Rectangle::new(column_view.width() / 2, column_view.height() / 2, 1, 1);
    popover.set_pointing_to(Some(&target));
    popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(column_view);
    if let Some(initial_focus) = crate::ui::transient_focus::popover_menu_initial_focus(popover) {
        focus_guard.bind_popover(popover.upcast_ref(), &initial_focus);
    } else {
        tracing::warn!("track context menu has no focusable menu item");
        focus_guard.restore_on_popover_close(popover.upcast_ref());
    }
    popover.popup();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_key_and_shift_f10_open_the_context_menu() {
        assert!(is_context_menu_shortcut(
            gdk::Key::Menu,
            gdk::ModifierType::empty()
        ));
        assert!(is_context_menu_shortcut(
            gdk::Key::F10,
            gdk::ModifierType::SHIFT_MASK
        ));
    }

    #[test]
    fn plain_f10_and_unrelated_keys_do_not_open_it() {
        assert!(!is_context_menu_shortcut(
            gdk::Key::F10,
            gdk::ModifierType::empty()
        ));
        assert!(!is_context_menu_shortcut(
            gdk::Key::Return,
            gdk::ModifierType::SHIFT_MASK
        ));
    }
}
