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
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    let shared = shared.clone();
    let listbox = listbox.clone();
    let row_for_menu = row.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        if !row_for_menu.is_selected() {
            listbox.unselect_all();
            listbox.select_row(Some(&row_for_menu));
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
        row_for_menu.insert_action_group("missingrow", Some(&action_group));
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
        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&row_for_menu);
        popover.set_has_arrow(false);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
        popover.popup();
    });
    row.add_controller(gesture);
}

pub(super) fn install_card_context_menu(shared: &Rc<Shared>, card: &gtk4::Box) {
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    let shared = shared.clone();
    let card_for_menu = card.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let action_group = gio::SimpleActionGroup::new();
        let action = gio::SimpleAction::new("search-folder", None);
        let shared_for_action = shared.clone();
        action.connect_activate(move |_, _| {
            let targets = collect_group_targets(&shared_for_action, &MissingGroupKind::Deleted);
            super::missing_dialogs::search_folder(locate_context(&shared_for_action), targets);
        });
        action_group.add_action(&action);
        card_for_menu.insert_action_group("missingcard", Some(&action_group));
        let menu = gio::Menu::new();
        menu.append(
            Some(&strings::issue_text(strings::MISSING_SEARCH_FOLDER)),
            Some("missingcard.search-folder"),
        );
        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&card_for_menu);
        popover.set_has_arrow(false);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
        popover.popup();
    });
    card.add_controller(gesture);
}
