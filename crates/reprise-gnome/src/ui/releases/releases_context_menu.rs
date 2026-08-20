//! Selection-aware context-menu input for Releases rows.

use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use reprise_core::artist_news_history::HistoryEntry;

use super::releases_menu;
use super::releases_model::ReleaseObject;
use super::releases_view::Shared;

/// CTX-2: a secondary click on a row outside the selection makes that row
/// the selection before the menu opens; inside it, the selection stands.
pub(super) fn claim_row_for_menu(shared: &Rc<Shared>, position: u32) {
    if !shared.model.selection().is_selected(position) {
        shared.model.selection().select_range(position, 1, true);
    }
}

pub(super) fn selected_entries(shared: &Rc<Shared>) -> Vec<HistoryEntry> {
    shared
        .model
        .selected_mbids()
        .into_iter()
        .filter_map(|mbid| shared.model.position_of(&mbid))
        .filter_map(|position| {
            shared
                .model
                .store()
                .item(position)
                .and_downcast::<ReleaseObject>()
        })
        .map(|object| object.entry())
        .collect()
}

pub(super) fn wire_cell(
    widget: &impl IsA<gtk4::Widget>,
    item: &gtk4::ListItem,
    shared: &Rc<Shared>,
) {
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = crate::ui::source_context_surface::secondary_click();
    let item = item.clone();
    let shared = shared.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let position = item.position();
        if position == gtk4::INVALID_LIST_POSITION {
            return;
        }
        let Some(parent) = gesture.widget() else {
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        claim_row_for_menu(&shared, position);
        let menu = releases_menu::build(&releases_menu::summarize(&selected_entries(&shared)));
        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_has_arrow(false);
        popover.set_parent(&parent);
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
        popover.popup();
    });
    widget.upcast_ref::<gtk4::Widget>().add_controller(gesture);
}

pub(super) fn wire(column_view: &gtk4::ColumnView, shared: &Rc<Shared>) {
    let group = gio::SimpleActionGroup::new();
    for (name, hidden) in [
        (releases_menu::ACTION_HIDE, true),
        (releases_menu::ACTION_RESTORE, false),
    ] {
        let shared = shared.clone();
        let action = gio::SimpleAction::new(name, None);
        action.connect_activate(move |_, _| {
            let mbids = selected_entries(&shared)
                .into_iter()
                .map(|entry| entry.release_group_mbid)
                .collect();
            super::releases_hide::set_hidden_batch(&shared, mbids, hidden);
        });
        group.add_action(&action);
    }
    for name in [
        releases_menu::ACTION_GO_TO_ARTIST,
        releases_menu::ACTION_GO_TO_ALBUM,
    ] {
        let action = gio::SimpleAction::new(name, None);
        action.connect_activate(move |_, _| {
            tracing::warn!(action = name, "releases menu action not wired yet");
        });
        group.add_action(&action);
    }
    column_view.insert_action_group(releases_menu::ACTION_GROUP, Some(&group));

    let keys = crate::ui::source_context_surface::context_keys();
    let menu_parent = column_view.clone();
    let shared = shared.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !crate::ui::source_context_surface::is_context_menu_shortcut(key, modifiers) {
            return gtk4::glib::Propagation::Proceed;
        }
        let entries = selected_entries(&shared);
        if entries.is_empty() {
            return gtk4::glib::Propagation::Proceed;
        }
        let menu = releases_menu::build(&releases_menu::summarize(&entries));
        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        present_keyboard_popover(&menu_parent, &popover);
        gtk4::glib::Propagation::Stop
    });
    column_view.add_controller(keys);
}

fn present_keyboard_popover(column_view: &gtk4::ColumnView, popover: &gtk4::PopoverMenu) {
    popover.set_parent(column_view);
    popover.set_has_arrow(false);
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
        column_view.width() / 2,
        column_view.height() / 2,
        1,
        1,
    )));
    crate::ui::popover_lifecycle::unparent_after_actions(popover.upcast_ref());
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(column_view);
    if let Some(initial_focus) = crate::ui::transient_focus::popover_menu_initial_focus(popover) {
        focus_guard.bind_popover(popover.upcast_ref(), &initial_focus);
    } else {
        tracing::warn!("releases context menu has no focusable menu item");
        focus_guard.restore_on_popover_close(popover.upcast_ref());
    }
    popover.popup();
}
