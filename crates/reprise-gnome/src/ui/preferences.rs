use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings::{self, ColorScheme, ListDensity, PlayerBarPosition};
use rusqlite::Connection;

use crate::ui::status_bar::StatusBar;
use crate::ui::strings;
use crate::ui::track_list::TrackList;

pub(super) const SMOKE_ENV: &str = "REPRISE_SMOKE_PREFERENCES";

fn plugin_applies_live(id: &str) -> bool {
    id == reprise_core::modules::COVER_DOWNLOAD_MODULE.id
}

fn color_scheme_from_index(index: u32) -> ColorScheme {
    match index {
        1 => ColorScheme::Light,
        2 => ColorScheme::Dark,
        _ => ColorScheme::System,
    }
}

fn color_scheme_index(value: ColorScheme) -> u32 {
    match value {
        ColorScheme::System => 0,
        ColorScheme::Light => 1,
        ColorScheme::Dark => 2,
    }
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
    if index == 1 {
        PlayerBarPosition::Top
    } else {
        PlayerBarPosition::Bottom
    }
}

fn bar_position_index(value: PlayerBarPosition) -> u32 {
    match value {
        PlayerBarPosition::Bottom => 0,
        PlayerBarPosition::Top => 1,
    }
}

fn apply_color_scheme(value: ColorScheme) {
    let value = match value {
        ColorScheme::System => adw::ColorScheme::Default,
        ColorScheme::Light => adw::ColorScheme::ForceLight,
        ColorScheme::Dark => adw::ColorScheme::ForceDark,
    };
    adw::StyleManager::default().set_color_scheme(value);
}

fn apply_density(widget: &gtk4::Widget, density: ListDensity) {
    for class in [
        "reprise-density-comfortable",
        "reprise-density-standard",
        "reprise-density-compact",
    ] {
        widget.remove_css_class(class);
    }
    let class = match density {
        ListDensity::Comfortable => "reprise-density-comfortable",
        ListDensity::Standard => "reprise-density-standard",
        ListDensity::Compact => "reprise-density-compact",
    };
    widget.add_css_class(class);
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(
        ".reprise-density-comfortable columnview row { min-height: 48px; }\n\
         .reprise-density-standard columnview row { min-height: 36px; }\n\
         .reprise-density-compact columnview row { min-height: 28px; }",
    );
    gtk4::style_context_add_provider_for_display(
        &widget.display(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub(super) struct PreferencesContext {
    window: adw::ApplicationWindow,
    conn: Rc<RefCell<Connection>>,
    track_list: Rc<TrackList>,
    sidebar_page: adw::NavigationPage,
    status_bar: StatusBar,
    toolbar_view: adw::ToolbarView,
    bottom_box: gtk4::Box,
    scan_button: gtk4::Button,
    on_minimal: Rc<dyn Fn()>,
}

impl PreferencesContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        window: &adw::ApplicationWindow,
        conn: &Rc<RefCell<Connection>>,
        track_list: &Rc<TrackList>,
        sidebar_page: &adw::NavigationPage,
        status_bar: &StatusBar,
        toolbar_view: &adw::ToolbarView,
        bottom_box: &gtk4::Box,
        scan_button: &gtk4::Button,
        on_minimal: impl Fn() + 'static,
    ) -> Rc<Self> {
        let context = Rc::new(Self {
            window: window.clone(),
            conn: conn.clone(),
            track_list: track_list.clone(),
            sidebar_page: sidebar_page.clone(),
            status_bar: status_bar.clone(),
            toolbar_view: toolbar_view.clone(),
            bottom_box: bottom_box.clone(),
            scan_button: scan_button.clone(),
            on_minimal: Rc::new(on_minimal),
        });
        context.apply_initial();
        context
    }

    fn apply_initial(&self) {
        let conn = self.conn.borrow();
        apply_color_scheme(settings::get_color_scheme(&conn));
        apply_density(
            self.track_list.root_widget().upcast_ref(),
            settings::get_list_density(&conn),
        );
        self.sidebar_page
            .set_visible(settings::get_sidebar_visible(&conn));
        self.status_bar
            .set_enabled(settings::get_status_visible(&conn));
    }

    pub(super) fn present(self: &Rc<Self>) {
        let dialog = adw::PreferencesDialog::new();
        dialog.add(&self.appearance_page());
        dialog.add(&self.layout_page());
        dialog.add(&self.library_page());
        dialog.add(&self.plugins_page());
        dialog.present(Some(&self.window));
        if let Ok(smoke) = std::env::var(SMOKE_ENV) {
            if smoke == "exercise" {
                self.apply_smoke();
            }
            glib::timeout_add_seconds_local_once(1, move || {
                dialog.close();
            });
        }
    }

    fn apply_smoke(&self) {
        let conn = self.conn.borrow();
        let _ = settings::set_color_scheme(&conn, ColorScheme::Dark);
        let _ = settings::set_list_density(&conn, ListDensity::Compact);
        let _ = settings::set_sidebar_visible(&conn, false);
        let _ = settings::set_status_visible(&conn, false);
        let _ = settings::set_player_bar_position(&conn, PlayerBarPosition::Top);
        drop(conn);
        apply_color_scheme(ColorScheme::Dark);
        apply_density(
            self.track_list.root_widget().upcast_ref(),
            ListDensity::Compact,
        );
        self.sidebar_page.set_visible(false);
        self.status_bar.set_enabled(false);
        crate::ui::window::apply_bar_position(
            &self.toolbar_view,
            &self.bottom_box,
            PlayerBarPosition::Top,
        );
        tracing::info!("preferences smoke applied appearance and layout settings");
    }

    fn appearance_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_APPEARANCE))
            .icon_name("applications-graphics-symbolic")
            .build();
        let group = adw::PreferencesGroup::new();
        let model = gtk4::StringList::new(&[
            &strings::text(strings::COLOR_SYSTEM),
            &strings::text(strings::COLOR_LIGHT),
            &strings::text(strings::COLOR_DARK),
        ]);
        let scheme = adw::ComboRow::builder()
            .title(strings::text(strings::COLOR_SCHEME))
            .model(&model)
            .selected(color_scheme_index(settings::get_color_scheme(
                &self.conn.borrow(),
            )))
            .build();
        let weak = Rc::downgrade(self);
        scheme.connect_selected_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let value = color_scheme_from_index(row.selected());
            if settings::set_color_scheme(&context.conn.borrow(), value).is_ok() {
                apply_color_scheme(value);
            }
        });
        group.add(&scheme);
        page.add(&group);
        page
    }

    fn layout_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_LAYOUT))
            .icon_name("view-grid-symbolic")
            .build();
        let group = adw::PreferencesGroup::new();
        let positions = gtk4::StringList::new(&[
            &strings::text(strings::POSITION_BOTTOM),
            &strings::text(strings::POSITION_TOP),
        ]);
        let bar = adw::ComboRow::builder()
            .title(strings::text(strings::PLAYER_BAR_POSITION))
            .model(&positions)
            .selected(bar_position_index(settings::get_player_bar_position(
                &self.conn.borrow(),
            )))
            .build();
        let weak = Rc::downgrade(self);
        bar.connect_selected_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let value = bar_position_from_index(row.selected());
            if settings::set_player_bar_position(&context.conn.borrow(), value).is_ok() {
                crate::ui::window::apply_bar_position(
                    &context.toolbar_view,
                    &context.bottom_box,
                    value,
                );
            }
        });
        group.add(&bar);

        let sidebar = adw::SwitchRow::builder()
            .title(strings::text(strings::SHOW_SIDEBAR))
            .active(settings::get_sidebar_visible(&self.conn.borrow()))
            .build();
        let weak = Rc::downgrade(self);
        sidebar.connect_active_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let active = row.is_active();
            if settings::set_sidebar_visible(&context.conn.borrow(), active).is_ok() {
                context.sidebar_page.set_visible(active);
            }
        });
        group.add(&sidebar);

        let status = adw::SwitchRow::builder()
            .title(strings::text(strings::SHOW_STATUS_LINE))
            .active(settings::get_status_visible(&self.conn.borrow()))
            .build();
        let weak = Rc::downgrade(self);
        status.connect_active_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let active = row.is_active();
            if settings::set_status_visible(&context.conn.borrow(), active).is_ok() {
                context.status_bar.set_enabled(active);
                if active {
                    context.track_list.reload();
                }
            }
        });
        group.add(&status);

        let densities = gtk4::StringList::new(&[
            &strings::text(strings::DENSITY_COMFORTABLE),
            &strings::text(strings::DENSITY_STANDARD),
            &strings::text(strings::DENSITY_COMPACT),
        ]);
        let density = adw::ComboRow::builder()
            .title(strings::text(strings::LIST_DENSITY))
            .model(&densities)
            .selected(density_index(settings::get_list_density(
                &self.conn.borrow(),
            )))
            .build();
        let weak = Rc::downgrade(self);
        density.connect_selected_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let value = density_from_index(row.selected());
            if settings::set_list_density(&context.conn.borrow(), value).is_ok() {
                apply_density(context.track_list.root_widget().upcast_ref(), value);
            }
        });
        group.add(&density);

        let weak = Rc::downgrade(self);
        group.add(&action_row(
            strings::EDIT_COLUMN_LAYOUT,
            Rc::new(move || {
                if let Some(context) = weak.upgrade() {
                    crate::ui::column_layout_editor::present(&context.window, &context.track_list);
                }
            }),
        ));
        group.add(&action_row(strings::MINIMAL_VIEW, self.on_minimal.clone()));
        page.add(&group);
        page
    }

    fn library_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_LIBRARY))
            .icon_name("folder-music-symbolic")
            .build();
        let group = adw::PreferencesGroup::new();
        let root = settings::get_library_root(&self.conn.borrow())
            .ok()
            .flatten()
            .unwrap_or_else(|| strings::text(strings::NO_LIBRARY_FOLDER));
        let folder = adw::ActionRow::builder()
            .title(strings::text(strings::LIBRARY_FOLDER))
            .subtitle(root)
            .build();
        let choose = gtk4::Button::with_label(&strings::text(strings::CHOOSE_FOLDER));
        choose.set_valign(gtk4::Align::Center);
        let scan_button = self.scan_button.clone();
        choose.connect_clicked(move |_| scan_button.emit_clicked());
        folder.add_suffix(&choose);
        group.add(&folder);

        let weak = Rc::downgrade(self);
        group.add(&action_row(
            strings::IMPORT_RHYTHMBOX_COLUMNS,
            Rc::new(move || {
                let Some(context) = weak.upgrade() else {
                    return;
                };
                if let Some(action) = context
                    .window
                    .lookup_action(crate::ui::primary_menu::ACTION_IMPORT_RHYTHMBOX_COLUMNS)
                {
                    action.activate(None);
                }
            }),
        ));
        page.add(&group);
        page
    }

    fn plugins_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_PLUGINS))
            .icon_name("application-x-addon-symbolic")
            .build();
        let group = adw::PreferencesGroup::new();
        for descriptor in reprise_core::modules::ALL_MODULES {
            let subtitle = if plugin_applies_live(descriptor.id) {
                descriptor.description.to_string()
            } else {
                format!(
                    "{} · {}",
                    descriptor.description,
                    strings::text(strings::RESTART_REQUIRED)
                )
            };
            let row = adw::SwitchRow::builder()
                .title(descriptor.name)
                .subtitle(subtitle)
                .active(
                    reprise_core::modules::is_enabled(&self.conn.borrow(), descriptor)
                        .unwrap_or(descriptor.default_enabled),
                )
                .build();
            let weak = Rc::downgrade(self);
            let descriptor = *descriptor;
            row.connect_active_notify(move |row| {
                let Some(context) = weak.upgrade() else {
                    return;
                };
                let active = row.is_active();
                if plugin_applies_live(descriptor.id) {
                    if let Some(action) = context
                        .window
                        .lookup_action(crate::ui::primary_menu::ACTION_DOWNLOAD_MISSING_COVERS)
                    {
                        action.change_state(&active.to_variant());
                    }
                } else if let Err(error) =
                    reprise_core::modules::set_enabled(&context.conn.borrow(), descriptor, active)
                {
                    tracing::warn!(%error, module = descriptor.id, "could not save plugin state");
                }
            });
            group.add(&row);
        }
        page.add(&group);
        page
    }
}

fn action_row(title: &str, callback: Rc<dyn Fn()>) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(strings::text(title))
        .activatable(true)
        .build();
    row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    row.connect_activated(move |_| callback());
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library::settings::{ColorScheme, ListDensity, PlayerBarPosition};

    #[test]
    fn combo_indices_round_trip_typed_layout_values() {
        assert_eq!(color_scheme_from_index(0), ColorScheme::System);
        assert_eq!(color_scheme_from_index(2), ColorScheme::Dark);
        assert_eq!(density_from_index(0), ListDensity::Comfortable);
        assert_eq!(density_from_index(2), ListDensity::Compact);
        assert_eq!(bar_position_from_index(0), PlayerBarPosition::Bottom);
        assert_eq!(bar_position_from_index(1), PlayerBarPosition::Top);
    }

    #[test]
    fn only_runtime_safe_plugins_apply_without_restart() {
        assert!(plugin_applies_live("cover_download"));
        assert!(!plugin_applies_live("mpris"));
        assert!(!plugin_applies_live("foreign"));
    }
}
