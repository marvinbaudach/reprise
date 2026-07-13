use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings::{
    self, ColorScheme, ListDensity, PlayerBarPosition, ReplayGainMode,
};
use rusqlite::Connection;

use crate::ui::player_controller::PlayerController;
use crate::ui::status_bar::StatusBar;
use crate::ui::strings;
use crate::ui::track_list::TrackList;

pub(super) const SMOKE_ENV: &str = "REPRISE_SMOKE_PREFERENCES";
const DENSITY_CSS: &str = ".reprise-density-comfortable columnview row { min-height: 48px; }\n\
     .reprise-density-standard columnview row { min-height: 36px; }\n\
     .reprise-density-compact columnview row { min-height: 28px; }";

fn plugin_applies_live(id: &str) -> bool {
    matches!(id, "cover_download" | "equalizer" | "replaygain")
}

fn plugin_title(descriptor: &reprise_core::modules::ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "cover_download" => strings::DOWNLOAD_MISSING_COVERS,
        "equalizer" => strings::EQUALIZER,
        "replaygain" => strings::REPLAYGAIN,
        _ => return descriptor.name.to_string(),
    };
    strings::text(message)
}

fn plugin_description(descriptor: &reprise_core::modules::ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "mpris" => strings::PLUGIN_MPRIS_DESCRIPTION,
        "cover_download" => strings::PLUGIN_COVER_DESCRIPTION,
        "equalizer" => strings::PLUGIN_EQUALIZER_DESCRIPTION,
        "replaygain" => strings::PLUGIN_REPLAYGAIN_DESCRIPTION,
        _ => return descriptor.description.to_string(),
    };
    strings::text(message)
}

fn equalizer_preset(index: u32) -> [f64; 10] {
    match index {
        1 => [4.0, 3.0, 2.0, 0.0, -1.0, 0.0, 2.0, 3.0, 4.0, 4.0],
        2 => [-1.0, 1.0, 3.0, 4.0, 2.0, 0.0, -1.0, -1.0, 1.0, 2.0],
        3 => [7.0, 6.0, 5.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        _ => [0.0; 10],
    }
}

fn replay_gain_from_index(index: u32) -> ReplayGainMode {
    match index {
        1 => ReplayGainMode::Track,
        2 => ReplayGainMode::Album,
        _ => ReplayGainMode::Off,
    }
}

fn replay_gain_index(mode: ReplayGainMode) -> u32 {
    match mode {
        ReplayGainMode::Off => 0,
        ReplayGainMode::Track => 1,
        ReplayGainMode::Album => 2,
    }
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

fn density_class(density: ListDensity) -> &'static str {
    match density {
        ListDensity::Comfortable => "reprise-density-comfortable",
        ListDensity::Standard => "reprise-density-standard",
        ListDensity::Compact => "reprise-density-compact",
    }
}

fn install_density_css(widget: &gtk4::Widget) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(DENSITY_CSS);
    gtk4::style_context_add_provider_for_display(
        &widget.display(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn apply_density(widget: &gtk4::Widget, density: ListDensity) {
    for class in [
        "reprise-density-comfortable",
        "reprise-density-standard",
        "reprise-density-compact",
    ] {
        widget.remove_css_class(class);
    }
    widget.add_css_class(density_class(density));
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
    player: Option<Rc<PlayerController>>,
    syncing_effect_controls: Cell<bool>,
    equalizer_controls: RefCell<Vec<adw::SwitchRow>>,
    replaygain_plugin: RefCell<Option<adw::SwitchRow>>,
    replaygain_mode: RefCell<Option<adw::ComboRow>>,
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
        player: Option<&Rc<PlayerController>>,
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
            player: player.cloned(),
            syncing_effect_controls: Cell::new(false),
            equalizer_controls: RefCell::new(Vec::new()),
            replaygain_plugin: RefCell::new(None),
            replaygain_mode: RefCell::new(None),
            on_minimal: Rc::new(on_minimal),
        });
        install_density_css(context.track_list.root_widget().upcast_ref());
        context.apply_initial();
        context
    }

    fn apply_initial(&self) {
        let (color_scheme, density, sidebar_visible, status_visible) = {
            let conn = self.conn.borrow();
            (
                settings::get_color_scheme(&conn),
                settings::get_list_density(&conn),
                settings::get_sidebar_visible(&conn),
                settings::get_status_visible(&conn),
            )
        };
        apply_color_scheme(color_scheme);
        apply_density(self.track_list.root_widget().upcast_ref(), density);
        self.sidebar_page.set_visible(sidebar_visible);
        self.status_bar.set_enabled(status_visible);
    }

    pub(super) fn present(self: &Rc<Self>) {
        self.equalizer_controls.borrow_mut().clear();
        self.replaygain_plugin.borrow_mut().take();
        self.replaygain_mode.borrow_mut().take();
        let dialog = adw::PreferencesDialog::new();
        dialog.add(&self.appearance_page());
        dialog.add(&self.layout_page());
        dialog.add(&self.library_page());
        dialog.add(&self.plugins_page());
        dialog.add(&self.playback_page());
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
        let _ = settings::set_equalizer_bands(&conn, equalizer_preset(1));
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
        self.set_equalizer_enabled(true);
        self.set_replay_gain_mode(ReplayGainMode::Track);
        tracing::info!("preferences smoke applied appearance, layout, and audio settings");
    }

    fn apply_audio_effects(&self) {
        let effects = {
            let conn = self.conn.borrow();
            super::audio_effects::stored(&conn)
        };
        if let Some(player) = &self.player {
            if let Err(error) = player.set_audio_effects(effects) {
                tracing::warn!(%error, "could not apply audio effects");
                let active = player.active_audio_effects();
                {
                    let conn = self.conn.borrow();
                    if let Err(persist_error) = super::audio_effects::persist(&conn, &active) {
                        tracing::warn!(%persist_error, "could not restore active audio settings");
                    }
                }
                self.syncing_effect_controls.set(true);
                for row in self.equalizer_controls.borrow().iter() {
                    row.set_active(active.equalizer_enabled);
                }
                if let Some(row) = self.replaygain_plugin.borrow().as_ref() {
                    row.set_active(active.replay_gain != ReplayGainMode::Off);
                }
                if let Some(row) = self.replaygain_mode.borrow().as_ref() {
                    row.set_selected(replay_gain_index(active.replay_gain));
                }
                self.syncing_effect_controls.set(false);
                player.show_toast(&strings::text(strings::AUDIO_EFFECTS_FAILED));
            }
        }
    }

    fn set_equalizer_enabled(&self, active: bool) {
        if let Err(error) = settings::set_equalizer_enabled(&self.conn.borrow(), active) {
            tracing::warn!(%error, "could not save equalizer state");
            return;
        }
        self.syncing_effect_controls.set(true);
        for row in self.equalizer_controls.borrow().iter() {
            row.set_active(active);
        }
        self.syncing_effect_controls.set(false);
        self.apply_audio_effects();
    }

    fn set_replay_gain_mode(&self, mode: ReplayGainMode) {
        if let Err(error) = settings::set_replay_gain_mode(&self.conn.borrow(), mode) {
            tracing::warn!(%error, "could not save ReplayGain mode");
            return;
        }
        self.syncing_effect_controls.set(true);
        if let Some(row) = self.replaygain_plugin.borrow().as_ref() {
            row.set_active(mode != ReplayGainMode::Off);
        }
        if let Some(row) = self.replaygain_mode.borrow().as_ref() {
            row.set_selected(replay_gain_index(mode));
        }
        self.syncing_effect_controls.set(false);
        self.apply_audio_effects();
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

    fn playback_page(self: &Rc<Self>) -> adw::PreferencesPage {
        const BAND_LABELS: [&str; 10] = [
            "31 Hz", "62 Hz", "125 Hz", "250 Hz", "500 Hz", "1 kHz", "2 kHz", "4 kHz", "8 kHz",
            "16 kHz",
        ];

        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_PLAYBACK))
            .icon_name("audio-speakers-symbolic")
            .build();
        let equalizer = adw::PreferencesGroup::builder()
            .title(strings::text(strings::EQUALIZER))
            .build();
        let enabled = adw::SwitchRow::builder()
            .title(strings::text(strings::ENABLE_EQUALIZER))
            .active(settings::get_equalizer_enabled(&self.conn.borrow()))
            .build();
        let weak = Rc::downgrade(self);
        enabled.connect_active_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            if context.syncing_effect_controls.get() {
                return;
            }
            context.set_equalizer_enabled(row.is_active());
        });
        self.equalizer_controls.borrow_mut().push(enabled.clone());
        equalizer.add(&enabled);

        let presets = gtk4::StringList::new(&[
            &strings::text(strings::PRESET_FLAT),
            &strings::text(strings::PRESET_ROCK),
            &strings::text(strings::PRESET_POP),
            &strings::text(strings::PRESET_BASS),
        ]);
        let stored_bands = settings::get_equalizer_bands(&self.conn.borrow());
        let selected = (0..4)
            .find(|index| equalizer_preset(*index) == stored_bands)
            .unwrap_or(gtk4::INVALID_LIST_POSITION);
        let preset = adw::ComboRow::builder()
            .title(strings::text(strings::EQUALIZER_PRESET))
            .model(&presets)
            .selected(selected)
            .build();
        equalizer.add(&preset);

        let updating_preset = Rc::new(Cell::new(false));
        let mut scales = Vec::with_capacity(BAND_LABELS.len());
        for (index, label) in BAND_LABELS.into_iter().enumerate() {
            let row = adw::ActionRow::builder().title(label).build();
            let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, -12.0, 12.0, 1.0);
            scale.set_width_request(220);
            scale.set_value(stored_bands[index]);
            scale.set_digits(0);
            scale.set_draw_value(true);
            let weak = Rc::downgrade(self);
            let preset = preset.clone();
            let updating = updating_preset.clone();
            scale.connect_value_changed(move |scale| {
                if updating.get() {
                    return;
                }
                let Some(context) = weak.upgrade() else {
                    return;
                };
                updating.set(true);
                preset.set_selected(gtk4::INVALID_LIST_POSITION);
                updating.set(false);
                let mut bands = settings::get_equalizer_bands(&context.conn.borrow());
                bands[index] = scale.value();
                if let Err(error) = settings::set_equalizer_bands(&context.conn.borrow(), bands) {
                    tracing::warn!(%error, "could not save equalizer bands");
                    return;
                }
                context.apply_audio_effects();
            });
            row.add_suffix(&scale);
            equalizer.add(&row);
            scales.push(scale);
        }
        let weak = Rc::downgrade(self);
        let updating = updating_preset.clone();
        preset.connect_selected_notify(move |row| {
            if updating.get() || row.selected() > 3 {
                return;
            }
            let Some(context) = weak.upgrade() else {
                return;
            };
            let bands = equalizer_preset(row.selected());
            if let Err(error) = settings::set_equalizer_bands(&context.conn.borrow(), bands) {
                tracing::warn!(%error, "could not save equalizer preset");
                return;
            }
            updating.set(true);
            for (scale, value) in scales.iter().zip(bands) {
                scale.set_value(value);
            }
            updating.set(false);
            context.apply_audio_effects();
        });
        page.add(&equalizer);

        let replaygain = adw::PreferencesGroup::builder()
            .title(strings::text(strings::REPLAYGAIN))
            .build();
        let modes = gtk4::StringList::new(&[
            &strings::text(strings::REPLAYGAIN_OFF),
            &strings::text(strings::REPLAYGAIN_TRACK),
            &strings::text(strings::REPLAYGAIN_ALBUM),
        ]);
        let mode = adw::ComboRow::builder()
            .title(strings::text(strings::REPLAYGAIN_MODE))
            .model(&modes)
            .selected(replay_gain_index(settings::get_replay_gain_mode(
                &self.conn.borrow(),
            )))
            .build();
        let weak = Rc::downgrade(self);
        mode.connect_selected_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            if context.syncing_effect_controls.get() {
                return;
            }
            context.set_replay_gain_mode(replay_gain_from_index(row.selected()));
        });
        self.replaygain_mode.borrow_mut().replace(mode.clone());
        replaygain.add(&mode);
        page.add(&replaygain);
        page
    }

    fn plugins_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_PLUGINS))
            .icon_name("application-x-addon-symbolic")
            .build();
        let group = adw::PreferencesGroup::new();
        for descriptor in reprise_core::modules::ALL_MODULES {
            let description = plugin_description(descriptor);
            let subtitle = if plugin_applies_live(descriptor.id) {
                description
            } else {
                format!(
                    "{} · {}",
                    description,
                    strings::text(strings::RESTART_REQUIRED)
                )
            };
            let row = adw::SwitchRow::builder()
                .title(plugin_title(descriptor))
                .subtitle(subtitle)
                .active(match descriptor.id {
                    "equalizer" => settings::get_equalizer_enabled(&self.conn.borrow()),
                    "replaygain" => {
                        settings::get_replay_gain_mode(&self.conn.borrow()) != ReplayGainMode::Off
                    }
                    _ => reprise_core::modules::is_enabled(&self.conn.borrow(), descriptor)
                        .unwrap_or(descriptor.default_enabled),
                })
                .build();
            let weak = Rc::downgrade(self);
            let descriptor = *descriptor;
            row.connect_active_notify(move |row| {
                let Some(context) = weak.upgrade() else {
                    return;
                };
                let active = row.is_active();
                if descriptor.id == "cover_download" {
                    if let Some(action) = context
                        .window
                        .lookup_action(crate::ui::primary_menu::ACTION_DOWNLOAD_MISSING_COVERS)
                    {
                        action.change_state(&active.to_variant());
                    }
                } else if descriptor.id == "equalizer" {
                    if context.syncing_effect_controls.get() {
                        return;
                    }
                    context.set_equalizer_enabled(active);
                } else if descriptor.id == "replaygain" {
                    if context.syncing_effect_controls.get() {
                        return;
                    }
                    let mode = if active {
                        ReplayGainMode::Track
                    } else {
                        ReplayGainMode::Off
                    };
                    context.set_replay_gain_mode(mode);
                } else if let Err(error) =
                    reprise_core::modules::set_enabled(&context.conn.borrow(), descriptor, active)
                {
                    tracing::warn!(%error, module = descriptor.id, "could not save plugin state");
                }
            });
            if descriptor.id == "equalizer" {
                self.equalizer_controls.borrow_mut().push(row.clone());
            } else if descriptor.id == "replaygain" {
                self.replaygain_plugin.borrow_mut().replace(row.clone());
            }
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
        assert!(plugin_applies_live("equalizer"));
        assert!(plugin_applies_live("replaygain"));
        assert!(!plugin_applies_live("mpris"));
        assert!(!plugin_applies_live("foreign"));
    }

    #[test]
    fn equalizer_presets_are_bounded_and_flat_is_zero() {
        assert_eq!(equalizer_preset(0), [0.0; 10]);
        for index in 0..4 {
            assert!(equalizer_preset(index)
                .into_iter()
                .all(|gain| (-12.0..=12.0).contains(&gain)));
        }
    }

    #[test]
    fn every_density_has_one_stable_css_class_and_rule() {
        for density in [
            ListDensity::Comfortable,
            ListDensity::Standard,
            ListDensity::Compact,
        ] {
            assert!(DENSITY_CSS.contains(density_class(density)));
        }
    }
}
