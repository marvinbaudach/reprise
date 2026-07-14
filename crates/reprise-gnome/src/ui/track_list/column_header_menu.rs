//! Native right-click visibility menu shared by every track-column header.

use std::rc::{Rc, Weak};

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::column_layout::{self, ColumnId, ColumnLayout};
use crate::ui::strings;
use crate::ui::track_list::TrackList;

const ACTION_GROUP: &str = "track-columns";

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeaderMenuSpec {
    id: ColumnId,
    label: String,
    action_name: &'static str,
    active: bool,
    enabled: bool,
}

fn header_menu_specs(layout: &ColumnLayout) -> Vec<HeaderMenuSpec> {
    layout
        .order
        .iter()
        .copied()
        .map(|id| HeaderMenuSpec {
            id,
            label: column_layout::column_label(id),
            action_name: id.as_str(),
            active: layout.visible.contains(&id),
            enabled: !matches!(id, ColumnId::Cover | ColumnId::Title),
        })
        .collect()
}

fn change_visibility(
    action: &gio::SimpleAction,
    track_list_weak: &Weak<TrackList>,
    id: ColumnId,
    visible: bool,
) {
    let Some(track_list) = track_list_weak.upgrade() else {
        return;
    };
    let current = track_list.current_column_layout();
    let next = column_layout::set_column_visible(&current, id, visible);
    if next == current {
        action.set_state(&current.visible.contains(&id).to_variant());
        return;
    }
    if let Err(error) = track_list.apply_column_layout(&next) {
        tracing::warn!(%error, column = id.as_str(), visible, "could not save column visibility from header menu");
        action.set_state(&current.visible.contains(&id).to_variant());
        track_list.toast(&strings::text(strings::COLUMN_LAYOUT_SAVE_FAILED));
        return;
    }
    tracing::info!(
        column = id.as_str(),
        visible,
        "column header visibility changed"
    );
}

pub(super) fn install(track_list: &Rc<TrackList>) {
    let layout = track_list.current_column_layout();
    let menu = gio::Menu::new();
    let actions = gio::SimpleActionGroup::new();
    let mut visibility_actions = std::collections::HashMap::new();

    for spec in header_menu_specs(&layout) {
        let action =
            gio::SimpleAction::new_stateful(spec.action_name, None, &spec.active.to_variant());
        action.set_enabled(spec.enabled);
        if spec.enabled {
            let id = spec.id;
            let track_list_weak = Rc::downgrade(track_list);
            let activate_track_list = track_list_weak.clone();
            action.connect_activate(move |action, _| {
                let visible = !action
                    .state()
                    .and_then(|state| state.get::<bool>())
                    .unwrap_or_default();
                change_visibility(action, &activate_track_list, id, visible);
            });
            action.connect_change_state(move |action, value| {
                let Some(visible) = value.and_then(glib::Variant::get::<bool>) else {
                    return;
                };
                change_visibility(action, &track_list_weak, id, visible);
            });
        }
        actions.add_action(&action);
        menu.append(
            Some(&spec.label),
            Some(&format!("{ACTION_GROUP}.{}", spec.action_name)),
        );
        visibility_actions.insert(spec.id, action);
    }

    *track_list.column_visibility_actions.borrow_mut() = visibility_actions;
    *track_list.column_visibility_menu.borrow_mut() = Some(menu.clone());
    track_list
        .shared
        .column_view
        .insert_action_group(ACTION_GROUP, Some(&actions));
    track_list.column_registry.set_header_menu(&menu);
}

pub(super) fn sync(track_list: &TrackList, layout: &ColumnLayout) {
    let actions = track_list.column_visibility_actions.borrow().clone();
    let menu = track_list.column_visibility_menu.borrow().clone();
    if let Some(menu) = menu {
        menu.remove_all();
        for spec in header_menu_specs(layout) {
            if let Some(action) = actions.get(&spec.id) {
                action.set_state(&spec.active.to_variant());
            }
            menu.append(
                Some(&spec.label),
                Some(&format!("{ACTION_GROUP}.{}", spec.action_name)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::ui::column_layout::{ColumnId, ColumnLayout};

    #[test]
    fn menu_specs_follow_layout_order_and_mark_fixed_columns() {
        let layout = ColumnLayout::default();
        let specs = header_menu_specs(&layout);

        assert_eq!(
            specs.iter().map(|spec| spec.id).collect::<Vec<_>>(),
            layout.order
        );
        assert_eq!(specs.len(), layout.order.len());
        for spec in specs {
            let fixed = matches!(spec.id, ColumnId::Cover | ColumnId::Title);
            assert_eq!(spec.enabled, !fixed);
            assert_eq!(spec.active, layout.visible.contains(&spec.id));
            assert_eq!(spec.action_name, spec.id.as_str());
        }
    }

    #[test]
    fn hidden_optional_columns_still_remain_available_in_the_menu() {
        let layout = ColumnLayout::default();
        let specs = header_menu_specs(&layout);
        let track_number = specs
            .iter()
            .find(|spec| spec.id == ColumnId::TrackNumber)
            .unwrap();
        let play_count = specs
            .iter()
            .find(|spec| spec.id == ColumnId::PlayCount)
            .unwrap();

        assert!(track_number.enabled);
        assert!(!track_number.active);
        assert!(play_count.enabled);
        assert!(!play_count.active);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn installed_header_menu_toggles_and_persists_visibility() {
        gtk4::init().unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let runtime = crate::ui::cover_download_worker::setup();
        let conn = Rc::new(RefCell::new(conn));
        let track_list = Rc::new(TrackList::new(
            conn.clone(),
            Box::new(|_, _, _| {}),
            |_, _, _, _| {},
            Vec::new,
            runtime,
        ));
        install(&track_list);

        for id in track_list.current_column_layout().order {
            assert!(track_list
                .column_registry
                .column(id)
                .unwrap()
                .header_menu()
                .is_some());
        }
        let artist = track_list
            .column_visibility_actions
            .borrow()
            .get(&ColumnId::Artist)
            .unwrap()
            .clone();
        artist.activate(None);

        assert!(!track_list.column_registry.is_visible(ColumnId::Artist));
        assert!(!track_list
            .current_column_layout()
            .visible
            .contains(&ColumnId::Artist));
        assert!(!artist.state().unwrap().get::<bool>().unwrap());

        let reordered = column_layout::move_column_after(
            &track_list.current_column_layout(),
            ColumnId::Artist,
            ColumnId::Album,
        );
        track_list.apply_column_layout(&reordered).unwrap();
        let menu = track_list
            .column_registry
            .column(ColumnId::Title)
            .unwrap()
            .header_menu()
            .unwrap();
        let third_label = menu
            .item_attribute_value(2, gio::MENU_ATTRIBUTE_LABEL, None)
            .unwrap()
            .get::<String>()
            .unwrap();
        assert_eq!(third_label, column_layout::column_label(ColumnId::Album));
    }
}
