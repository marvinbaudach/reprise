use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings::{self, ListDensity, PlayerBarPosition};

use super::preference_choice_cards::{self, ChoiceCardSpec};
use super::preference_visual_strings as visual_strings;
use super::preferences::{action_row, apply_density, PreferencesContext};
use super::strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryWindowControl {
    Sidebar,
    BrowseBar,
    InfoPanel,
    StatusLine,
}

fn library_window_controls() -> [LibraryWindowControl; 4] {
    [
        LibraryWindowControl::Sidebar,
        LibraryWindowControl::BrowseBar,
        LibraryWindowControl::InfoPanel,
        LibraryWindowControl::StatusLine,
    ]
}

#[derive(Debug, Clone, Copy)]
struct LibraryWindowStates {
    sidebar: bool,
    browse_bar: bool,
    info_panel: bool,
    status_line: bool,
}

impl LibraryWindowStates {
    fn active(self, control: LibraryWindowControl) -> bool {
        match control {
            LibraryWindowControl::Sidebar => self.sidebar,
            LibraryWindowControl::BrowseBar => self.browse_bar,
            LibraryWindowControl::InfoPanel => self.info_panel,
            LibraryWindowControl::StatusLine => self.status_line,
        }
    }
}

fn control_title(control: LibraryWindowControl) -> String {
    match control {
        LibraryWindowControl::Sidebar => strings::text(strings::SHOW_SIDEBAR),
        LibraryWindowControl::BrowseBar => visual_strings::text(visual_strings::SHOW_FILTERS),
        LibraryWindowControl::InfoPanel => {
            visual_strings::text(visual_strings::SHOW_INFORMATION_PANEL)
        }
        LibraryWindowControl::StatusLine => strings::text(strings::SHOW_STATUS_LINE),
    }
}

fn control_save_failure(control: LibraryWindowControl) -> &'static str {
    match control {
        LibraryWindowControl::Sidebar => visual_strings::SIDEBAR_VISIBILITY_SAVE_FAILED,
        LibraryWindowControl::BrowseBar => visual_strings::FILTER_VISIBILITY_SAVE_FAILED,
        LibraryWindowControl::InfoPanel => visual_strings::INFORMATION_VISIBILITY_SAVE_FAILED,
        LibraryWindowControl::StatusLine => visual_strings::STATUS_VISIBILITY_SAVE_FAILED,
    }
}

fn build_library_window_rows(
    states: LibraryWindowStates,
    on_changed: &Rc<dyn Fn(LibraryWindowControl, bool) -> bool>,
) -> Vec<adw::SwitchRow> {
    library_window_controls()
        .into_iter()
        .map(|control| {
            let active = states.active(control);
            let row = adw::SwitchRow::builder()
                .title(control_title(control))
                .active(active)
                .build();
            let committed = Rc::new(Cell::new(active));
            let syncing = Rc::new(Cell::new(false));
            let on_changed = on_changed.clone();
            row.connect_active_notify(move |row| {
                if syncing.get() {
                    return;
                }
                let requested = row.is_active();
                if on_changed(control, requested) {
                    committed.set(requested);
                    return;
                }
                syncing.set(true);
                row.set_active(committed.get());
                syncing.set(false);
            });
            row
        })
        .collect()
}

fn apply_window_control(
    context: &PreferencesContext,
    control: LibraryWindowControl,
    active: bool,
) -> Result<(), rusqlite::Error> {
    {
        let conn = context.conn.borrow();
        match control {
            LibraryWindowControl::Sidebar => settings::set_sidebar_visible(&conn, active),
            LibraryWindowControl::BrowseBar => settings::set_browse_visible(&conn, active),
            LibraryWindowControl::InfoPanel => settings::set_info_panel_visible(&conn, active),
            LibraryWindowControl::StatusLine => settings::set_status_visible(&conn, active),
        }
    }?;
    match control {
        LibraryWindowControl::Sidebar => super::window_navigation::apply_sidebar_visibility(
            &context.split_view,
            &context.sidebar_page,
            active,
        ),
        LibraryWindowControl::BrowseBar => context.track_list.set_browse_visible(active),
        LibraryWindowControl::InfoPanel => {
            context.info_panel.apply_persisted_visibility(active);
        }
        LibraryWindowControl::StatusLine => {
            context.status_bar.set_enabled(active);
            if active {
                context.track_list.reload();
            }
        }
    }
    Ok(())
}

fn visible_columns_subtitle(context: &PreferencesContext) -> String {
    let layout = context.track_list.current_column_layout();
    layout
        .order
        .into_iter()
        .filter(|id| layout.visible.contains(id))
        .map(super::column_layout::column_label)
        .collect::<Vec<_>>()
        .join(" · ")
}

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
    let states = {
        let conn = context.conn.borrow();
        LibraryWindowStates {
            sidebar: settings::get_sidebar_visible(&conn),
            browse_bar: settings::get_browse_visible(&conn),
            info_panel: settings::get_info_panel_visible(&conn),
            status_line: settings::get_status_visible(&conn),
        }
    };
    let on_window_control_changed: Rc<dyn Fn(LibraryWindowControl, bool) -> bool> = {
        let weak = Rc::downgrade(context);
        Rc::new(move |control, active| {
            let Some(context) = weak.upgrade() else {
                return false;
            };
            match apply_window_control(&context, control, active) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(%error, ?control, "could not save library window control");
                    context
                        .track_list
                        .toast(&visual_strings::text(control_save_failure(control)));
                    false
                }
            }
        })
    };
    for row in build_library_window_rows(states, &on_window_control_changed) {
        window_group.add(&row);
    }

    let densities = gtk4::StringList::new(&[
        &strings::text(strings::DENSITY_COMFORTABLE),
        &strings::text(strings::DENSITY_STANDARD),
        &strings::text(strings::DENSITY_COMPACT),
    ]);
    let selected_density = {
        let conn = context.conn.borrow();
        density_index(settings::get_list_density(&conn))
    };
    let density = adw::ComboRow::builder()
        .title(strings::text(strings::LIST_DENSITY))
        .model(&densities)
        .selected(selected_density)
        .build();
    let committed_density = Rc::new(Cell::new(selected_density));
    let syncing_density = Rc::new(Cell::new(false));
    let weak = Rc::downgrade(context);
    let committed_density_for_change = committed_density.clone();
    let syncing_density_for_change = syncing_density.clone();
    density.connect_selected_notify(move |row| {
        let Some(context) = weak.upgrade() else {
            return;
        };
        if syncing_density_for_change.get() {
            return;
        }
        let value = density_from_index(row.selected());
        let saved = {
            let conn = context.conn.borrow();
            settings::set_list_density(&conn, value)
        };
        match saved {
            Ok(()) => {
                committed_density_for_change.set(row.selected());
                apply_density(context.track_list.root_widget().upcast_ref(), value);
            }
            Err(error) => {
                tracing::warn!(%error, "could not save list density");
                syncing_density_for_change.set(true);
                row.set_selected(committed_density_for_change.get());
                syncing_density_for_change.set(false);
                context
                    .track_list
                    .toast(&visual_strings::text(visual_strings::DENSITY_SAVE_FAILED));
            }
        }
    });
    window_group.add(&density);
    page.add(&window_group);

    let columns_group = adw::PreferencesGroup::builder()
        .title(visual_strings::text(visual_strings::COLUMNS))
        .build();
    let weak = Rc::downgrade(context);
    let columns = action_row(
        strings::EDIT_COLUMN_LAYOUT,
        Rc::new(move || {
            if let Some(context) = weak.upgrade() {
                context.open_column_layout_editor();
            }
        }),
    );
    columns.set_subtitle(&visible_columns_subtitle(context));
    columns_group.add(&columns);
    page.add(&columns_group);
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_window_controls_cover_every_visible_region_once() {
        assert_eq!(
            library_window_controls(),
            [
                LibraryWindowControl::Sidebar,
                LibraryWindowControl::BrowseBar,
                LibraryWindowControl::InfoPanel,
                LibraryWindowControl::StatusLine,
            ]
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn layout_widgets_include_four_rollback_switches_and_both_preview_edges() {
        if gtk4::init().is_err() {
            return;
        }
        let reject: Rc<dyn Fn(LibraryWindowControl, bool) -> bool> = Rc::new(|_, _| false);
        let rows = build_library_window_rows(
            LibraryWindowStates {
                sidebar: true,
                browse_bar: true,
                info_panel: true,
                status_line: true,
            },
            &reject,
        );
        assert_eq!(rows.len(), 4);
        rows[0].set_active(false);
        assert!(rows[0].is_active());

        let top = player_bar_preview(PlayerBarPosition::Top);
        let bottom = player_bar_preview(PlayerBarPosition::Bottom);
        assert!(top
            .first_child()
            .is_some_and(|child| child.has_css_class("reprise-preview-player")));
        assert!(bottom
            .last_child()
            .is_some_and(|child| child.has_css_class("reprise-preview-player")));
    }

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
