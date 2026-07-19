use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use reprise_core::library::relink::RelinkTarget;
use reprise_core::queries::MissingGroupKind;

use super::missing_view::{
    collect_group_targets, confirm_remove, locate_context, selected_ids, Shared,
};
use crate::ui::strings;

pub(super) fn install_row_context_menu(
    shared: &Rc<Shared>,
    listbox: &gtk4::ListBox,
    row: &gtk4::ListBoxRow,
    target: RelinkTarget,
    removable: bool,
    locatable: bool,
) {
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    let shared_for_pointer = shared.clone();
    let listbox_for_pointer = listbox.clone();
    let row_for_menu = row.clone();
    let target_for_pointer = target.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        show_row_menu(
            &shared_for_pointer,
            &listbox_for_pointer,
            &row_for_menu,
            &target_for_pointer,
            removable,
            locatable,
            x,
            y,
        );
    });
    row.add_controller(gesture);

    let keys = gtk4::EventControllerKey::new();
    let shared = shared.clone();
    let listbox = listbox.clone();
    let row_for_keys = row.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !crate::ui::track_list::track_list_context_keys::is_context_menu_shortcut(key, modifiers)
        {
            return gtk4::glib::Propagation::Proceed;
        }
        show_row_menu(
            &shared,
            &listbox,
            &row_for_keys,
            &target,
            removable,
            locatable,
            f64::from(row_for_keys.width()) / 2.0,
            f64::from(row_for_keys.height()) / 2.0,
        );
        gtk4::glib::Propagation::Stop
    });
    row.add_controller(keys);
}

#[allow(clippy::too_many_arguments)]
fn show_row_menu(
    shared: &Rc<Shared>,
    listbox: &gtk4::ListBox,
    row: &gtk4::ListBoxRow,
    target: &RelinkTarget,
    removable: bool,
    locatable: bool,
    x: f64,
    y: f64,
) {
    if !row.is_selected() {
        listbox.unselect_all();
        listbox.select_row(Some(row));
    }
    let action_group = gio::SimpleActionGroup::new();
    if removable {
        let action = gio::SimpleAction::new("remove", None);
        let shared = shared.clone();
        let listbox = listbox.clone();
        action.connect_activate(move |_, _| confirm_remove(&shared, selected_ids(&listbox)));
        action_group.add_action(&action);
    }
    if locatable {
        let action = gio::SimpleAction::new("locate", None);
        let shared = shared.clone();
        let target = target.clone();
        action.connect_activate(move |_, _| {
            super::missing_dialogs::locate_file(locate_context(&shared), target.clone());
        });
        action_group.add_action(&action);
    }
    row.insert_action_group("missingrow", Some(&action_group));
    let menu = gio::Menu::new();
    if locatable {
        menu.append(
            Some(&strings::issue_text(strings::MISSING_LOCATE)),
            Some("missingrow.locate"),
        );
    }
    if removable {
        menu.append(
            Some(&strings::issue_text(strings::MISSING_REMOVE)),
            Some("missingrow.remove"),
        );
    }
    show_menu(row.upcast_ref(), &menu, x, y);
}

pub(super) fn install_card_context_menu(shared: &Rc<Shared>, card: &gtk4::Box, label: &str) {
    // a11y-semantics: role=group name=missing-group state=menu action=shift-f10
    card.set_focusable(true);
    card.set_accessible_role(gtk4::AccessibleRole::Group);
    card.update_property(&[
        gtk4::accessible::Property::Label(label),
        gtk4::accessible::Property::HasPopup(true),
        gtk4::accessible::Property::KeyShortcuts("Menu Shift+F10"),
    ]);
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    let shared_for_pointer = shared.clone();
    let card_for_menu = card.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        show_card_menu(&shared_for_pointer, &card_for_menu, x, y);
    });
    card.add_controller(gesture);

    let keys = gtk4::EventControllerKey::new();
    let shared = shared.clone();
    let card_for_keys = card.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !crate::ui::track_list::track_list_context_keys::is_context_menu_shortcut(key, modifiers)
        {
            return gtk4::glib::Propagation::Proceed;
        }
        show_card_menu(
            &shared,
            &card_for_keys,
            f64::from(card_for_keys.width()) / 2.0,
            f64::from(card_for_keys.height()) / 2.0,
        );
        gtk4::glib::Propagation::Stop
    });
    card.add_controller(keys);
}

fn show_card_menu(shared: &Rc<Shared>, card: &gtk4::Box, x: f64, y: f64) {
    let action_group = gio::SimpleActionGroup::new();
    let action = gio::SimpleAction::new("search-folder", None);
    let shared_for_action = shared.clone();
    action.connect_activate(move |_, _| {
        let targets = collect_group_targets(&shared_for_action, &MissingGroupKind::Deleted);
        super::missing_dialogs::search_folder(locate_context(&shared_for_action), targets);
    });
    action_group.add_action(&action);
    card.insert_action_group("missingcard", Some(&action_group));
    let menu = gio::Menu::new();
    menu.append(
        Some(&strings::issue_text(strings::MISSING_SEARCH_FOLDER)),
        Some("missingcard.search-folder"),
    );
    show_menu(card.upcast_ref(), &menu, x, y);
}

fn show_menu(parent: &gtk4::Widget, menu: &gio::Menu, x: f64, y: f64) {
    let popover = gtk4::PopoverMenu::from_model(Some(menu));
    popover.set_parent(parent);
    popover.set_has_arrow(false);
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(parent);
    focus_guard.restore_on_popover_close(popover.upcast_ref());
    popover.popup();
}
