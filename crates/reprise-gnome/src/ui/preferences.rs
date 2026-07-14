use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings::{self, ListDensity, PlayerBarPosition, ReplayGainMode};
use rusqlite::Connection;

use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::device_sync_runtime::DeviceSyncRuntime;
use crate::ui::info_panel::InfoPanel;
use crate::ui::library_player_bar::LibraryPlayerBarShell;
use crate::ui::player_controller::PlayerController;
use crate::ui::preference_playback::build_equalizer_surface;
use crate::ui::preference_plugins::{plugin_applies_live, plugin_description, plugin_title};
use crate::ui::scan_flow::ScanControls;
use crate::ui::scan_progress::ScanProgressView;
use crate::ui::scrobble_runtime::ScrobbleRuntime;
use crate::ui::sidebar::Sidebar;
use crate::ui::status_bar::StatusBar;
use crate::ui::strings;
use crate::ui::track_list::TrackList;
use crate::ui::window_decorations::WindowDecorations;

pub(super) const SMOKE_ENV: &str = "REPRISE_SMOKE_PREFERENCES";

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

fn apply_system_color_scheme() {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);
}

pub(super) struct PreferencesContext {
    pub(super) window: adw::ApplicationWindow,
    pub(super) conn: Rc<RefCell<Connection>>,
    pub(super) track_list: Rc<TrackList>,
    pub(super) sidebar: Rc<Sidebar>,
    pub(super) split_view: adw::NavigationSplitView,
    pub(super) sidebar_page: adw::NavigationPage,
    pub(super) status_bar: StatusBar,
    pub(super) library_player_bar: LibraryPlayerBarShell,
    pub(super) info_panel: Rc<InfoPanel>,
    pub(super) scan_button: gtk4::Button,
    scan_controls: ScanControls,
    pub(super) library_folder_rows: RefCell<Vec<glib::WeakRef<adw::ActionRow>>>,
    pub(super) player: Option<Rc<PlayerController>>,
    pub(super) syncing_effect_controls: Cell<bool>,
    pub(super) equalizer_controls: RefCell<Vec<adw::SwitchRow>>,
    pub(super) equalizer_surfaces: RefCell<Vec<gtk4::Widget>>,
    pub(super) replaygain_mode: RefCell<Option<adw::ComboRow>>,
    pub(super) listenbrainz: Rc<ScrobbleRuntime>,
    pub(super) syncing_listenbrainz: Cell<bool>,
    pub(super) listenbrainz_activation_pending: Cell<bool>,
    pub(super) lastfm: Rc<ScrobbleRuntime>,
    pub(super) syncing_lastfm: Cell<bool>,
    pub(super) lastfm_activation_pending: Cell<bool>,
    pub(super) artist_news: Rc<ArtistNewsRuntime>,
    pub(super) decorations: Rc<WindowDecorations>,
    pub(super) device_sync: Rc<DeviceSyncRuntime>,
    preferences_window: RefCell<glib::WeakRef<adw::Window>>,
    preferences_navigation: RefCell<glib::WeakRef<adw::NavigationView>>,
}

impl PreferencesContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        window: &adw::ApplicationWindow,
        conn: &Rc<RefCell<Connection>>,
        track_list: &Rc<TrackList>,
        sidebar: &Rc<Sidebar>,
        split_view: &adw::NavigationSplitView,
        sidebar_page: &adw::NavigationPage,
        status_bar: &StatusBar,
        library_player_bar: &LibraryPlayerBarShell,
        info_panel: &Rc<InfoPanel>,
        scan_button: &gtk4::Button,
        scan_controls: &ScanControls,
        player: Option<&Rc<PlayerController>>,
        listenbrainz: &Rc<ScrobbleRuntime>,
        lastfm: &Rc<ScrobbleRuntime>,
        artist_news: &Rc<ArtistNewsRuntime>,
        decorations: &Rc<WindowDecorations>,
        device_sync: &Rc<DeviceSyncRuntime>,
    ) -> Rc<Self> {
        let context = Rc::new(Self {
            window: window.clone(),
            conn: conn.clone(),
            track_list: track_list.clone(),
            sidebar: sidebar.clone(),
            split_view: split_view.clone(),
            sidebar_page: sidebar_page.clone(),
            status_bar: status_bar.clone(),
            library_player_bar: library_player_bar.clone(),
            info_panel: info_panel.clone(),
            scan_button: scan_button.clone(),
            scan_controls: scan_controls.clone(),
            library_folder_rows: RefCell::new(Vec::new()),
            player: player.cloned(),
            syncing_effect_controls: Cell::new(false),
            equalizer_controls: RefCell::new(Vec::new()),
            equalizer_surfaces: RefCell::new(Vec::new()),
            replaygain_mode: RefCell::new(None),
            listenbrainz: listenbrainz.clone(),
            syncing_listenbrainz: Cell::new(false),
            listenbrainz_activation_pending: Cell::new(false),
            lastfm: lastfm.clone(),
            syncing_lastfm: Cell::new(false),
            lastfm_activation_pending: Cell::new(false),
            artist_news: artist_news.clone(),
            decorations: decorations.clone(),
            device_sync: device_sync.clone(),
            preferences_window: RefCell::new(glib::WeakRef::new()),
            preferences_navigation: RefCell::new(glib::WeakRef::new()),
        });
        let weak = Rc::downgrade(&context);
        context.scan_button.connect_sensitive_notify(move |button| {
            if button.is_sensitive() {
                if let Some(context) = weak.upgrade() {
                    context.refresh_library_folder_rows();
                }
            }
        });
        super::list_density::install(context.track_list.column_view_widget());
        context.apply_initial();
        context
    }

    fn apply_initial(&self) {
        let (density, sidebar_visible, browse_visible, info_visible, status_visible, decorations) = {
            let conn = self.conn.borrow();
            (
                settings::get_list_density(&conn),
                settings::get_sidebar_visible(&conn),
                settings::get_browse_visible(&conn),
                settings::get_info_panel_visible(&conn),
                settings::get_status_visible(&conn),
                settings::get_window_decoration_mode(&conn),
            )
        };
        apply_system_color_scheme();
        super::list_density::apply(self.track_list.column_view_widget(), density);
        super::window_navigation::apply_sidebar_visibility(
            &self.split_view,
            &self.sidebar_page,
            sidebar_visible,
        );
        self.track_list.set_browse_visible(browse_visible);
        self.info_panel.apply_persisted_visibility(info_visible);
        self.status_bar.set_enabled(status_visible);
        self.decorations.apply(decorations);
        tracing::info!(
            sidebar_visible,
            browse_visible,
            info_visible,
            status_visible,
            "persisted library layout applied"
        );
    }

    pub(super) fn present(self: &Rc<Self>) {
        if let Some(window) = self.preferences_window.borrow().upgrade() {
            window.present();
            return;
        }
        self.equalizer_controls.borrow_mut().clear();
        self.equalizer_surfaces.borrow_mut().clear();
        self.replaygain_mode.borrow_mut().take();
        use super::preferences_window::{PageId, PAGE_ORDER};
        let pages = PAGE_ORDER.map(|id| {
            let page = match id {
                PageId::Playback => self.playback_page(),
                PageId::Appearance => self.appearance_page(),
                PageId::Layout => self.layout_page(),
                PageId::Library => self.library_page(),
                PageId::Synchronization => super::preference_sync::build_page(&self.device_sync),
                PageId::Plugins => self.plugins_page(),
            };
            (id, page)
        });
        let foreground_scan_progress = ScanProgressView::new();
        self.scan_controls
            .attach_progress_view(&foreground_scan_progress);
        let shell = super::preferences_window::build(
            &self.window,
            pages,
            Some(foreground_scan_progress.widget().upcast_ref()),
        );
        shell.window.connect_destroy(move |_| {
            let _keep_progress_alive_until_destroy = &foreground_scan_progress;
        });
        self.preferences_window.borrow().set(Some(&shell.window));
        self.preferences_navigation
            .borrow()
            .set(Some(&shell.navigation));
        let smoke = std::env::var(SMOKE_ENV).ok();
        if matches!(smoke.as_deref(), Some("layout" | "columns")) {
            shell.stack.set_visible_child_name("layout");
        }
        if smoke.as_deref() == Some("rhythmbox") {
            shell.stack.set_visible_child_name("library");
        }
        if smoke.as_deref() == Some("columns") {
            self.open_column_layout_editor();
        }
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
        let _ = settings::set_list_density(&conn, ListDensity::Compact);
        let _ = settings::set_sidebar_visible(&conn, false);
        let _ = settings::set_browse_visible(&conn, false);
        let _ = settings::set_info_panel_visible(&conn, false);
        let _ = settings::set_status_visible(&conn, false);
        let _ = settings::set_window_decoration_mode(
            &conn,
            reprise_core::library::settings::WindowDecorationMode::System,
        );
        let _ = settings::set_player_bar_position(&conn, PlayerBarPosition::Top);
        let _ = settings::set_equalizer_bands(&conn, equalizer_preset(1));
        drop(conn);
        apply_system_color_scheme();
        super::list_density::apply(self.track_list.column_view_widget(), ListDensity::Compact);
        super::window_navigation::apply_sidebar_visibility(
            &self.split_view,
            &self.sidebar_page,
            false,
        );
        self.track_list.set_browse_visible(false);
        self.info_panel.apply_persisted_visibility(false);
        self.status_bar.set_enabled(false);
        self.decorations
            .apply(reprise_core::library::settings::WindowDecorationMode::System);
        self.library_player_bar.set_position(PlayerBarPosition::Top);
        self.set_equalizer_enabled(true);
        self.set_replay_gain_mode(ReplayGainMode::Track);
        tracing::info!("preferences smoke applied layout and audio settings");
    }

    fn appearance_page(self: &Rc<Self>) -> adw::PreferencesPage {
        super::preference_appearance::build(self)
    }

    fn layout_page(self: &Rc<Self>) -> adw::PreferencesPage {
        super::preference_layout::build(self)
    }

    pub(super) fn open_column_layout_editor(&self) {
        let navigation = self.preferences_navigation.borrow().upgrade();
        let Some(navigation) = navigation else {
            tracing::warn!("column layout editor requested without preferences navigation");
            return;
        };
        let page = super::column_layout_editor::build_navigation_page(&self.track_list);
        navigation.push(&page);
    }

    pub(super) fn open_rhythmbox_import(self: &Rc<Self>) {
        let navigation = self.preferences_navigation.borrow().upgrade();
        let Some(navigation) = navigation else {
            tracing::warn!("Rhythmbox import requested without preferences navigation");
            return;
        };
        super::preference_rhythmbox::push_import_page(self, &navigation);
    }

    pub(super) fn preferences_window(&self) -> Option<adw::Window> {
        self.preferences_window.borrow().upgrade()
    }

    fn playback_page(self: &Rc<Self>) -> adw::PreferencesPage {
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
        let weak = Rc::downgrade(self);
        let preset_for_band = preset.clone();
        let updating_for_band = updating_preset.clone();
        let on_band_changed: Rc<dyn Fn(usize, f64)> = Rc::new(move |index, value| {
            if updating_for_band.get() {
                return;
            }
            let Some(context) = weak.upgrade() else {
                return;
            };
            updating_for_band.set(true);
            preset_for_band.set_selected(gtk4::INVALID_LIST_POSITION);
            updating_for_band.set(false);
            let mut bands = settings::get_equalizer_bands(&context.conn.borrow());
            bands[index] = value;
            if let Err(error) = settings::set_equalizer_bands(&context.conn.borrow(), bands) {
                tracing::warn!(%error, "could not save equalizer bands");
                return;
            }
            context.apply_audio_effects();
        });
        let surface = build_equalizer_surface(stored_bands, equalizer_enabled, &on_band_changed);
        let scales = surface.scales.clone();
        self.equalizer_surfaces
            .borrow_mut()
            .push(surface.root.clone().upcast());
        equalizer.add(&surface.root);

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
                if descriptor.id == "listenbrainz" {
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
            if descriptor.id == "listenbrainz" {
                self.add_listenbrainz_controls(&row);
            } else if descriptor.id == "lastfm" {
                self.add_lastfm_controls(&row);
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

    #[test]
    fn only_runtime_safe_plugins_apply_without_restart() {
        assert!(!plugin_applies_live("cover_download"));
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
    #[ignore = "requires a display; run via xvfb-run"]
    fn appearance_always_restores_the_system_color_scheme() {
        if gtk4::init().is_err() {
            return;
        }
        let style = adw::StyleManager::default();
        style.set_color_scheme(adw::ColorScheme::ForceDark);

        apply_system_color_scheme();

        assert_eq!(style.color_scheme(), adw::ColorScheme::Default);
    }
}
