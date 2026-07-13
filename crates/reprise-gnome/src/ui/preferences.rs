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

use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::cover_download_batch::CoverDownloadBatch;
use crate::ui::library_player_bar::LibraryPlayerBarShell;
use crate::ui::player_controller::PlayerController;
use crate::ui::preference_plugins::{plugin_applies_live, plugin_description, plugin_title};
use crate::ui::scrobble_runtime::ScrobbleRuntime;
use crate::ui::status_bar::StatusBar;
use crate::ui::strings;
use crate::ui::track_list::TrackList;
use crate::ui::window_decorations::WindowDecorations;

pub(super) const SMOKE_ENV: &str = "REPRISE_SMOKE_PREFERENCES";
const DENSITY_CSS: &str = ".reprise-density-comfortable columnview row { min-height: 48px; }\n\
     .reprise-density-standard columnview row { min-height: 36px; }\n\
     .reprise-density-compact columnview row { min-height: 28px; }";

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

pub(super) fn replay_gain_index(mode: ReplayGainMode) -> u32 {
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
    pub(super) window: adw::ApplicationWindow,
    pub(super) conn: Rc<RefCell<Connection>>,
    pub(super) track_list: Rc<TrackList>,
    split_view: adw::NavigationSplitView,
    sidebar_page: adw::NavigationPage,
    status_bar: StatusBar,
    library_player_bar: LibraryPlayerBarShell,
    pub(super) scan_button: gtk4::Button,
    pub(super) library_folder_rows: RefCell<Vec<glib::WeakRef<adw::ActionRow>>>,
    pub(super) player: Option<Rc<PlayerController>>,
    pub(super) syncing_effect_controls: Cell<bool>,
    pub(super) equalizer_controls: RefCell<Vec<adw::SwitchRow>>,
    pub(super) replaygain_mode: RefCell<Option<adw::ComboRow>>,
    pub(super) cover_batch: Rc<CoverDownloadBatch>,
    pub(super) listenbrainz: Rc<ScrobbleRuntime>,
    pub(super) syncing_listenbrainz: Cell<bool>,
    pub(super) listenbrainz_activation_pending: Cell<bool>,
    pub(super) lastfm: Rc<ScrobbleRuntime>,
    pub(super) syncing_lastfm: Cell<bool>,
    pub(super) lastfm_activation_pending: Cell<bool>,
    pub(super) artist_news: Rc<ArtistNewsRuntime>,
    pub(super) decorations: Rc<WindowDecorations>,
    preferences_window: RefCell<glib::WeakRef<adw::Window>>,
}

impl PreferencesContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        window: &adw::ApplicationWindow,
        conn: &Rc<RefCell<Connection>>,
        track_list: &Rc<TrackList>,
        split_view: &adw::NavigationSplitView,
        sidebar_page: &adw::NavigationPage,
        status_bar: &StatusBar,
        library_player_bar: &LibraryPlayerBarShell,
        scan_button: &gtk4::Button,
        player: Option<&Rc<PlayerController>>,
        cover_batch: &Rc<CoverDownloadBatch>,
        listenbrainz: &Rc<ScrobbleRuntime>,
        lastfm: &Rc<ScrobbleRuntime>,
        artist_news: &Rc<ArtistNewsRuntime>,
        decorations: &Rc<WindowDecorations>,
    ) -> Rc<Self> {
        let context = Rc::new(Self {
            window: window.clone(),
            conn: conn.clone(),
            track_list: track_list.clone(),
            split_view: split_view.clone(),
            sidebar_page: sidebar_page.clone(),
            status_bar: status_bar.clone(),
            library_player_bar: library_player_bar.clone(),
            scan_button: scan_button.clone(),
            library_folder_rows: RefCell::new(Vec::new()),
            player: player.cloned(),
            syncing_effect_controls: Cell::new(false),
            equalizer_controls: RefCell::new(Vec::new()),
            replaygain_mode: RefCell::new(None),
            cover_batch: cover_batch.clone(),
            listenbrainz: listenbrainz.clone(),
            syncing_listenbrainz: Cell::new(false),
            listenbrainz_activation_pending: Cell::new(false),
            lastfm: lastfm.clone(),
            syncing_lastfm: Cell::new(false),
            lastfm_activation_pending: Cell::new(false),
            artist_news: artist_news.clone(),
            decorations: decorations.clone(),
            preferences_window: RefCell::new(glib::WeakRef::new()),
        });
        let weak = Rc::downgrade(&context);
        context.scan_button.connect_sensitive_notify(move |button| {
            if button.is_sensitive() {
                if let Some(context) = weak.upgrade() {
                    context.refresh_library_folder_rows();
                }
            }
        });
        install_density_css(context.track_list.root_widget().upcast_ref());
        context.apply_initial();
        context
    }

    fn apply_initial(&self) {
        let (color_scheme, density, sidebar_visible, status_visible, decorations) = {
            let conn = self.conn.borrow();
            (
                settings::get_color_scheme(&conn),
                settings::get_list_density(&conn),
                settings::get_sidebar_visible(&conn),
                settings::get_status_visible(&conn),
                settings::get_window_decoration_mode(&conn),
            )
        };
        apply_color_scheme(color_scheme);
        apply_density(self.track_list.root_widget().upcast_ref(), density);
        super::window_navigation::apply_sidebar_visibility(
            &self.split_view,
            &self.sidebar_page,
            sidebar_visible,
        );
        self.status_bar.set_enabled(status_visible);
        self.decorations.apply(decorations);
    }

    pub(super) fn present(self: &Rc<Self>) {
        if let Some(window) = self.preferences_window.borrow().upgrade() {
            window.present();
            return;
        }
        self.equalizer_controls.borrow_mut().clear();
        self.replaygain_mode.borrow_mut().take();
        use super::preferences_window::{PageId, PAGE_ORDER};
        let pages = PAGE_ORDER.map(|id| {
            let page = match id {
                PageId::Playback => self.playback_page(),
                PageId::Appearance => self.appearance_page(),
                PageId::Layout => self.layout_page(),
                PageId::Library => self.library_page(),
                PageId::Plugins => self.plugins_page(),
            };
            (id, page)
        });
        let shell = super::preferences_window::build(&self.window, pages);
        self.preferences_window.borrow().set(Some(&shell.window));
        let smoke = std::env::var(SMOKE_ENV).ok();
        if smoke.as_deref() == Some("exercise") {
            let context = Rc::downgrade(self);
            let exercised = Rc::new(Cell::new(false));
            shell.window.connect_map(move |_| {
                if exercised.replace(true) {
                    return;
                }
                let context = context.clone();
                glib::idle_add_local_once(move || {
                    if let Some(context) = context.upgrade() {
                        context.apply_smoke();
                    }
                });
            });
        }
        shell.window.present();
        tracing::debug!("preferences window presented");
        if smoke.is_some() {
            let window = shell.window.clone();
            glib::timeout_add_seconds_local_once(1, move || {
                window.close();
            });
        }
    }

    fn apply_smoke(&self) {
        let conn = self.conn.borrow();
        let _ = settings::set_color_scheme(&conn, ColorScheme::Dark);
        let _ = settings::set_list_density(&conn, ListDensity::Compact);
        let _ = settings::set_sidebar_visible(&conn, false);
        let _ = settings::set_status_visible(&conn, false);
        let _ = settings::set_window_decoration_mode(
            &conn,
            reprise_core::library::settings::WindowDecorationMode::System,
        );
        let _ = settings::set_player_bar_position(&conn, PlayerBarPosition::Top);
        let _ = settings::set_equalizer_bands(&conn, equalizer_preset(1));
        drop(conn);
        apply_color_scheme(ColorScheme::Dark);
        apply_density(
            self.track_list.root_widget().upcast_ref(),
            ListDensity::Compact,
        );
        super::window_navigation::apply_sidebar_visibility(
            &self.split_view,
            &self.sidebar_page,
            false,
        );
        self.status_bar.set_enabled(false);
        self.decorations
            .apply(reprise_core::library::settings::WindowDecorationMode::System);
        self.library_player_bar.set_position(PlayerBarPosition::Top);
        self.set_equalizer_enabled(true);
        self.set_replay_gain_mode(ReplayGainMode::Track);
        tracing::info!("preferences smoke applied appearance, layout, and audio settings");
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
        let selected_scheme = {
            let conn = self.conn.borrow();
            color_scheme_index(settings::get_color_scheme(&conn))
        };
        let scheme = adw::ComboRow::builder()
            .title(strings::text(strings::COLOR_SCHEME))
            .model(&model)
            .selected(selected_scheme)
            .build();
        let weak = Rc::downgrade(self);
        scheme.connect_selected_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let value = color_scheme_from_index(row.selected());
            let saved = {
                let conn = context.conn.borrow();
                settings::set_color_scheme(&conn, value)
            };
            if saved.is_ok() {
                apply_color_scheme(value);
            }
        });
        group.add(&scheme);
        group.add(&super::preference_window_decorations::row(self));
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
        let selected_position = {
            let conn = self.conn.borrow();
            bar_position_index(settings::get_player_bar_position(&conn))
        };
        let bar = adw::ComboRow::builder()
            .title(strings::text(strings::PLAYER_BAR_POSITION))
            .model(&positions)
            .selected(selected_position)
            .build();
        let weak = Rc::downgrade(self);
        bar.connect_selected_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let value = bar_position_from_index(row.selected());
            let saved = {
                let conn = context.conn.borrow();
                settings::set_player_bar_position(&conn, value)
            };
            if saved.is_ok() {
                context.library_player_bar.set_position(value);
            }
        });
        group.add(&bar);

        let sidebar_visible = {
            let conn = self.conn.borrow();
            settings::get_sidebar_visible(&conn)
        };
        let sidebar = adw::SwitchRow::builder()
            .title(strings::text(strings::SHOW_SIDEBAR))
            .active(sidebar_visible)
            .build();
        let weak = Rc::downgrade(self);
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
        group.add(&sidebar);

        let status_visible = {
            let conn = self.conn.borrow();
            settings::get_status_visible(&conn)
        };
        let status = adw::SwitchRow::builder()
            .title(strings::text(strings::SHOW_STATUS_LINE))
            .active(status_visible)
            .build();
        let weak = Rc::downgrade(self);
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
        group.add(&status);

        let densities = gtk4::StringList::new(&[
            &strings::text(strings::DENSITY_COMFORTABLE),
            &strings::text(strings::DENSITY_STANDARD),
            &strings::text(strings::DENSITY_COMPACT),
        ]);
        let selected_density = {
            let conn = self.conn.borrow();
            density_index(settings::get_list_density(&conn))
        };
        let density = adw::ComboRow::builder()
            .title(strings::text(strings::LIST_DENSITY))
            .model(&densities)
            .selected(selected_density)
            .build();
        let weak = Rc::downgrade(self);
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
        let equalizer_enabled = {
            let conn = self.conn.borrow();
            settings::get_equalizer_enabled(&conn)
        };
        let enabled = adw::SwitchRow::builder()
            .title(strings::text(strings::ENABLE_EQUALIZER))
            .active(equalizer_enabled)
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
        let selected_mode = {
            let conn = self.conn.borrow();
            replay_gain_index(settings::get_replay_gain_mode(&conn))
        };
        let mode = adw::ComboRow::builder()
            .title(strings::text(strings::REPLAYGAIN_MODE))
            .model(&modes)
            .selected(selected_mode)
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
            let active = reprise_core::modules::is_enabled(&self.conn.borrow(), descriptor)
                .unwrap_or(descriptor.default_enabled);
            let row = adw::SwitchRow::builder()
                .title(plugin_title(descriptor))
                .subtitle(subtitle)
                .use_markup(false)
                .active(active)
                .build();
            let syncing = Rc::new(Cell::new(false));
            let weak = Rc::downgrade(self);
            let descriptor = *descriptor;
            let syncing_notify = syncing.clone();
            row.connect_active_notify(move |row| {
                let Some(context) = weak.upgrade() else {
                    return;
                };
                if syncing_notify.get() {
                    return;
                }
                let active = row.is_active();
                if descriptor.id == "cover_download" {
                    if let Some(action) = context
                        .window
                        .lookup_action(crate::ui::primary_menu::ACTION_DOWNLOAD_MISSING_COVERS)
                    {
                        action.change_state(&active.to_variant());
                    }
                } else if descriptor.id == "listenbrainz" {
                    context.change_listenbrainz_activation(row, active);
                } else if descriptor.id == "lastfm" {
                    context.change_lastfm_activation(row, active);
                } else {
                    let result = if descriptor.id == "artist_news" {
                        context.artist_news.set_enabled(&context.conn.borrow(), active)
                    } else {
                        reprise_core::modules::set_enabled(
                            &context.conn.borrow(),
                            descriptor,
                            active,
                        )
                    };
                    if let Err(error) = result {
                        tracing::warn!(%error, module = descriptor.id, "could not save plugin state");
                        syncing_notify.set(true);
                        row.set_active(!active);
                        syncing_notify.set(false);
                    }
                }
            });
            if descriptor.id == "artist_news" {
                let alive = glib::WeakRef::new();
                alive.set(Some(&row));
                let target = alive.clone();
                let syncing = syncing.clone();
                self.artist_news.subscribe_enabled(
                    move || alive.upgrade().is_some(),
                    move |enabled| {
                        let Some(row) = target.upgrade() else { return };
                        syncing.set(true);
                        row.set_active(enabled);
                        syncing.set(false);
                    },
                );
            }
            group.add(&row);
            if descriptor.id == "cover_download" {
                self.add_cover_download_progress(&group);
            } else if descriptor.id == "listenbrainz" {
                self.add_listenbrainz_account(&group, &row);
            } else if descriptor.id == "lastfm" {
                self.add_lastfm_account(&group, &row);
            }
        }
        page.add(&group);
        page
    }
}

pub(super) fn action_row(title: &str, callback: Rc<dyn Fn()>) -> adw::ActionRow {
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
        assert!(plugin_applies_live("listenbrainz"));
        assert!(plugin_applies_live("lastfm"));
        assert!(plugin_applies_live("artist_news"));
        assert!(plugin_applies_live("lastfm"));
        assert!(!plugin_applies_live("equalizer"));
        assert!(!plugin_applies_live("replaygain"));
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
