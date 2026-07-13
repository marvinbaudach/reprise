use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings::{self, ListDensity, PlayerBarPosition};

use super::preference_choice_cards::{self, ChoiceCardSpec};
use super::preference_visual_strings as visual_strings;
use super::preferences::{action_row, apply_density, PreferencesContext};
use super::strings;

fn density_from_index(index: u32) -> ListDensity {
    match index {
        0 => ListDensity::Comfortable,
        2 => ListDensity::Compact,
        _ => ListDensity::Standard,
    }
}

fn density_index(value: ListDensity) -> u32 {
    match value {
        ListDensity::Comfortable => 0,
        ListDensity::Standard => 1,
        ListDensity::Compact => 2,
    }
}

fn bar_position_from_index(index: u32) -> PlayerBarPosition {
    if index == 0 {
        PlayerBarPosition::Top
    } else {
        PlayerBarPosition::Bottom
    }
}

fn bar_position_index(value: PlayerBarPosition) -> u32 {
    match value {
        PlayerBarPosition::Top => 0,
        PlayerBarPosition::Bottom => 1,
    }
}

fn player_bar_preview(position: PlayerBarPosition) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.add_css_class("reprise-choice-preview");
    root.set_height_request(88);
    root.set_overflow(gtk4::Overflow::Hidden);
    let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    bar.set_height_request(14);
    bar.add_css_class("reprise-preview-player");
    let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    let sidebar = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    sidebar.set_width_request(38);
    sidebar.add_css_class("reprise-preview-sidebar");
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_hexpand(true);
    content.add_css_class("reprise-preview-content");
    body.append(&sidebar);
    body.append(&content);
    if position == PlayerBarPosition::Top {
        root.append(&bar);
        root.append(&body);
    } else {
        root.append(&body);
        root.append(&bar);
    }
    root
}

pub(super) fn build(context: &Rc<PreferencesContext>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::PREFERENCES_LAYOUT))
        .icon_name("view-grid-symbolic")
        .build();

    let player_bar_group = adw::PreferencesGroup::builder()
        .title(visual_strings::text(visual_strings::PLAYER_BAR))
        .description(strings::text(strings::PLAYER_BAR_POSITION))
        .build();
    let selected_position = {
        let conn = context.conn.borrow();
        bar_position_index(settings::get_player_bar_position(&conn))
    };
    let on_position_selected: Rc<dyn Fn(u32) -> bool> = {
        let weak = Rc::downgrade(context);
        Rc::new(move |index| {
            let Some(context) = weak.upgrade() else {
                return false;
            };
            let value = bar_position_from_index(index);
            let saved = {
                let conn = context.conn.borrow();
                settings::set_player_bar_position(&conn, value)
            };
            match saved {
                Ok(()) => {
                    context.library_player_bar.set_position(value);
                    true
                }
                Err(error) => {
                    tracing::warn!(%error, "could not save player bar position");
                    context.track_list.toast(&visual_strings::text(
                        visual_strings::PLAYER_BAR_POSITION_SAVE_FAILED,
                    ));
                    false
                }
            }
        })
    };
    let cards = preference_choice_cards::build(
        vec![
            ChoiceCardSpec::new(
                strings::text(strings::POSITION_TOP),
                &player_bar_preview(PlayerBarPosition::Top),
            ),
            ChoiceCardSpec::new(
                strings::text(strings::POSITION_BOTTOM),
                &player_bar_preview(PlayerBarPosition::Bottom),
            ),
        ],
        selected_position,
        &on_position_selected,
    );
    player_bar_group.add(&cards.root);
    page.add(&player_bar_group);

    let window_group = adw::PreferencesGroup::builder()
        .title(visual_strings::text(visual_strings::LIBRARY_WINDOW))
        .build();
    let sidebar = adw::SwitchRow::builder()
        .title(strings::text(strings::SHOW_SIDEBAR))
        .active({
            let conn = context.conn.borrow();
            settings::get_sidebar_visible(&conn)
        })
        .build();
    let weak = Rc::downgrade(context);
    sidebar.connect_active_notify(move |row| {
        let Some(context) = weak.upgrade() else {
            return;
        };
        let active = row.is_active();
        let saved = {
            let conn = context.conn.borrow();
            settings::set_sidebar_visible(&conn, active)
        };
        if saved.is_ok() {
            super::window_navigation::apply_sidebar_visibility(
                &context.split_view,
                &context.sidebar_page,
                active,
            );
        }
    });
    window_group.add(&sidebar);

    let status = adw::SwitchRow::builder()
        .title(strings::text(strings::SHOW_STATUS_LINE))
        .active({
            let conn = context.conn.borrow();
            settings::get_status_visible(&conn)
        })
        .build();
    let weak = Rc::downgrade(context);
    status.connect_active_notify(move |row| {
        let Some(context) = weak.upgrade() else {
            return;
        };
        let active = row.is_active();
        let saved = {
            let conn = context.conn.borrow();
            settings::set_status_visible(&conn, active)
        };
        if saved.is_ok() {
            context.status_bar.set_enabled(active);
            if active {
                context.track_list.reload();
            }
        }
    });
    window_group.add(&status);

    let densities = gtk4::StringList::new(&[
        &strings::text(strings::DENSITY_COMFORTABLE),
        &strings::text(strings::DENSITY_STANDARD),
        &strings::text(strings::DENSITY_COMPACT),
    ]);
    let density = adw::ComboRow::builder()
        .title(strings::text(strings::LIST_DENSITY))
        .model(&densities)
        .selected({
            let conn = context.conn.borrow();
            density_index(settings::get_list_density(&conn))
        })
        .build();
    let weak = Rc::downgrade(context);
    density.connect_selected_notify(move |row| {
        let Some(context) = weak.upgrade() else {
            return;
        };
        let value = density_from_index(row.selected());
        let saved = {
            let conn = context.conn.borrow();
            settings::set_list_density(&conn, value)
        };
        if saved.is_ok() {
            apply_density(context.track_list.root_widget().upcast_ref(), value);
        }
    });
    window_group.add(&density);
    page.add(&window_group);

    let columns_group = adw::PreferencesGroup::builder()
        .title(visual_strings::text(visual_strings::COLUMNS))
        .build();
    let weak = Rc::downgrade(context);
    columns_group.add(&action_row(
        strings::EDIT_COLUMN_LAYOUT,
        Rc::new(move || {
            if let Some(context) = weak.upgrade() {
                crate::ui::column_layout_editor::present(&context.window, &context.track_list);
            }
        }),
    ));
    page.add(&columns_group);
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_bar_cards_round_trip_top_then_bottom() {
        for (index, value) in [PlayerBarPosition::Top, PlayerBarPosition::Bottom]
            .into_iter()
            .enumerate()
        {
            assert_eq!(bar_position_index(value), index as u32);
            assert_eq!(bar_position_from_index(index as u32), value);
        }
    }

    #[test]
    fn density_combo_round_trips_every_typed_value() {
        for (index, value) in [
            ListDensity::Comfortable,
            ListDensity::Standard,
            ListDensity::Compact,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(density_index(value), index as u32);
            assert_eq!(density_from_index(index as u32), value);
        }
    }
}
