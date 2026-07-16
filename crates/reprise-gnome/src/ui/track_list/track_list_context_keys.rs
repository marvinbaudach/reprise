//! Keyboard access to the track-list context menu.

use gtk4::gdk;
use gtk4::prelude::*;

use super::popover_lifecycle;
use super::track_list_context_menu;
use super::Shared;

fn is_context_menu_shortcut(key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
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
        popover.set_parent(&column_view_handle);
        popover.set_has_arrow(false);
        let target = gdk::Rectangle::new(
            column_view_handle.width() / 2,
            column_view_handle.height() / 2,
            1,
            1,
        );
        popover.set_pointing_to(Some(&target));
        popover_lifecycle::unparent_after_actions(popover.upcast_ref());
        popover.popup();
        tracing::debug!("track context menu opened from keyboard");
        gtk4::glib::Propagation::Stop
    });
    column_view.add_controller(controller);
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
