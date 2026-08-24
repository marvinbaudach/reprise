use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::equalizer::EqualizerPreset;
use reprise_core::library::settings::{self, ListDensity, PlayerBarPosition, ReplayGainMode};

use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::concerts::ConcertsRuntime;
use crate::ui::cover_download_worker::CoverDownloadRuntime;
use crate::ui::library_player_bar::LibraryPlayerBarShell;
use crate::ui::location_broadcast::LocationBroadcast;
use crate::ui::lyrics_batch::LyricsBatch;
use crate::ui::now_playing::NowPlayingPanel;
use crate::ui::player_controller::PlayerController;
use crate::ui::podcasts::PodcastsRuntime;
use crate::ui::preferences::preference_equalizer::build_equalizer_controls;
#[cfg(test)]
use crate::ui::preferences::preference_equalizer::EqualizerControls;
use crate::ui::scan_chrome::ScanChromeView;
use crate::ui::scan_flow::ScanControls;
use crate::ui::scrobble_runtime::ScrobbleRuntime;
use crate::ui::sidebar::Sidebar;
use crate::ui::status_bar::StatusBar;
use crate::ui::strings;
use crate::ui::track_list::TrackList;
use crate::ui::window_decorations::WindowDecorations;

pub(in crate::ui) const SMOKE_ENV: &str = "REPRISE_SMOKE_PREFERENCES";

#[derive(Clone, Copy)]
enum SettingsDeepLink {
    Location,
}

impl SettingsDeepLink {
    fn page_name(self) -> &'static str {
        match self {
            Self::Location => "location",
        }
    }
}

#[derive(Clone, Copy)]
enum PluginDeepLink {
    OnlineSources,
    Artwork,
}

fn plugin_targets_for_deep_link(link: PluginDeepLink) -> &'static [&'static str] {
    match link {
        PluginDeepLink::OnlineSources => &["youtube", "podcasts", "radio"],
        PluginDeepLink::Artwork => &["artwork"],
    }
}

pub(super) fn equalizer_preset(index: u32) -> Option<EqualizerPreset> {
    EqualizerPreset::ALL.get(index as usize).copied()
}

pub(super) fn preset_label(preset: EqualizerPreset) -> &'static str {
    match preset {
        EqualizerPreset::Flat => strings::PRESET_FLAT,
        EqualizerPreset::Rock => strings::PRESET_ROCK,
        EqualizerPreset::Pop => strings::PRESET_POP,
        EqualizerPreset::Bass => strings::PRESET_BASS,
        EqualizerPreset::Classical => strings::PRESET_CLASSICAL,
        EqualizerPreset::Jazz => strings::PRESET_JAZZ,
        EqualizerPreset::Electronic => strings::PRESET_ELECTRONIC,
        EqualizerPreset::Vocal => strings::PRESET_VOCAL,
        EqualizerPreset::Headphones => strings::PRESET_HEADPHONES,
        EqualizerPreset::LateNight => strings::PRESET_LATE_NIGHT,
    }
}

fn replay_gain_from_index(index: u32) -> ReplayGainMode {
    match index {
        1 => ReplayGainMode::Track,
        2 => ReplayGainMode::Album,
        _ => ReplayGainMode::Off,
    }
}

pub(in crate::ui) fn replay_gain_index(mode: ReplayGainMode) -> u32 {
    match mode {
        ReplayGainMode::Off => 0,
        ReplayGainMode::Track => 1,
        ReplayGainMode::Album => 2,
    }
}

/// Formats the crossfade slider's live value readout: `0` reads as "Off",
/// otherwise a whole-second overlap ("4.0 s").
fn crossfade_value_label(seconds: u8) -> String {
    if seconds == 0 {
        strings::text(strings::CROSSFADE_OFF)
    } else {
        format!("{:.1} s", f64::from(seconds))
    }
}

#[derive(Debug, PartialEq, Eq)]
struct GaplessControlState {
    sensitive: bool,
    subtitle: &'static str,
}

fn gapless_control_state(crossfade_seconds: u8) -> GaplessControlState {
    if crossfade_seconds == 0 {
        GaplessControlState {
            sensitive: true,
            subtitle: strings::GAPLESS_SUBTITLE,
        }
    } else {
        GaplessControlState {
            sensitive: false,
            subtitle: strings::GAPLESS_CROSSFADE_ACTIVE_SUBTITLE,
        }
    }
}

fn apply_gapless_control_state(row: &adw::SwitchRow, crossfade_seconds: u8) {
    let state = gapless_control_state(crossfade_seconds);
    row.set_sensitive(state.sensitive);
    row.set_subtitle(&strings::text(state.subtitle));
}

pub(in crate::ui) struct PreferencesContext {
    pub(in crate::ui) window: adw::ApplicationWindow,
    pub(in crate::ui) conn: Rc<Db>,
    pub(in crate::ui) track_list: Rc<TrackList>,
    pub(in crate::ui) sidebar: Rc<Sidebar>,
    pub(in crate::ui) split_view: adw::OverlaySplitView,
    pub(in crate::ui) sidebar_page: adw::NavigationPage,
    pub(in crate::ui) status_bar: StatusBar,
    pub(in crate::ui) library_player_bar: LibraryPlayerBarShell,
    pub(in crate::ui) info_panel: Rc<NowPlayingPanel>,
    pub(in crate::ui) scan_button: gtk4::Button,
    pub(in crate::ui) scan_controls: ScanControls,
    pub(in crate::ui) library_folder_rows: RefCell<Vec<glib::WeakRef<adw::ActionRow>>>,
    pub(in crate::ui) player: Option<Rc<PlayerController>>,
    pub(in crate::ui) syncing_effect_controls: Cell<bool>,
    pub(in crate::ui) equalizer_controls: RefCell<Vec<adw::SwitchRow>>,
    // The Layout page's preview and switches point at each other through one
    // request handler; the context owns the strong end so the handler can hold
    // a weak one and the page's widgets are released with the dialog.
    pub(in crate::ui) layout_controls:
        RefCell<Option<std::rc::Rc<super::preference_layout::LayoutControls>>>,
    // Owned here, not by the master switch that drives it: the section holds
    // the widget whose callback has to reach the section.
    pub(in crate::ui) online_section:
        RefCell<Option<std::rc::Rc<super::preference_plugins::OnlineSection>>>,
    pub(in crate::ui) background_bar:
        RefCell<Option<super::preference_background_bar::BackgroundBar>>,
    pub(in crate::ui) equalizer_surfaces: RefCell<Vec<gtk4::Widget>>,
    pub(in crate::ui) replaygain_mode: RefCell<Option<adw::ComboRow>>,
    pub(in crate::ui) listenbrainz: Rc<ScrobbleRuntime>,
    pub(in crate::ui) syncing_listenbrainz: Cell<bool>,
    pub(in crate::ui) listenbrainz_activation_pending: Cell<bool>,
    pub(in crate::ui) lastfm: Rc<ScrobbleRuntime>,
    pub(in crate::ui) syncing_lastfm: Cell<bool>,
    pub(in crate::ui) lastfm_activation_pending: Cell<bool>,
    pub(in crate::ui) artist_news: Rc<ArtistNewsRuntime>,
    pub(in crate::ui) concerts: Rc<ConcertsRuntime>,
    pub(in crate::ui) location_broadcast: Rc<LocationBroadcast>,
    pub(in crate::ui) podcasts: Rc<PodcastsRuntime>,
    pub(in crate::ui) cover_download: CoverDownloadRuntime,
    pub(in crate::ui) lyrics_batch: Rc<LyricsBatch>,
    pub(in crate::ui) cover_batch: Rc<crate::ui::cover_download_batch::CoverDownloadBatch>,
    pub(in crate::ui) artist_portrait: Rc<ArtistPortraitRuntime>,
    pub(in crate::ui) decorations: Rc<WindowDecorations>,
    preferences_dialog: RefCell<glib::WeakRef<adw::Dialog>>,
    preferences_navigation: RefCell<glib::WeakRef<adw::NavigationView>>,
    preferences_stack: RefCell<glib::WeakRef<adw::ViewStack>>,
    preferences_sidebar: RefCell<glib::WeakRef<gtk4::ListBox>>,
    pub(in crate::ui) location_city_row: RefCell<glib::WeakRef<adw::ActionRow>>,
    pub(super) connectivity: Cell<reprise_core::connectivity::Connectivity>,
    pub(super) on_artwork_permission_changed:
        RefCell<Option<super::preference_online_module_effects::ArtworkPermissionCallback>>,
    pub(in crate::ui) plugin_rows: RefCell<HashMap<&'static str, glib::WeakRef<gtk4::Widget>>>,
    pub(in crate::ui) pending_plugin_targets: RefCell<Vec<&'static str>>,
}

impl PreferencesContext {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::ui) fn new(
        window: &adw::ApplicationWindow,
        conn: &Rc<Db>,
        track_list: &Rc<TrackList>,
        sidebar: &Rc<Sidebar>,
        split_view: &adw::OverlaySplitView,
        sidebar_page: &adw::NavigationPage,
        status_bar: &StatusBar,
        library_player_bar: &LibraryPlayerBarShell,
        info_panel: &Rc<NowPlayingPanel>,
        scan_button: &gtk4::Button,
        scan_controls: &ScanControls,
        player: Option<&Rc<PlayerController>>,
        listenbrainz: &Rc<ScrobbleRuntime>,
        lastfm: &Rc<ScrobbleRuntime>,
        artist_news: &Rc<ArtistNewsRuntime>,
        concerts: &Rc<ConcertsRuntime>,
        location_broadcast: &Rc<LocationBroadcast>,
        podcasts: &Rc<PodcastsRuntime>,
        cover_download: &CoverDownloadRuntime,
        lyrics_batch: &Rc<LyricsBatch>,
        cover_batch: &Rc<crate::ui::cover_download_batch::CoverDownloadBatch>,
        artist_portrait: &Rc<ArtistPortraitRuntime>,
        decorations: &Rc<WindowDecorations>,
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
            layout_controls: RefCell::new(None),
            online_section: RefCell::new(None),
            background_bar: RefCell::new(None),
            equalizer_surfaces: RefCell::new(Vec::new()),
            replaygain_mode: RefCell::new(None),
            listenbrainz: listenbrainz.clone(),
            syncing_listenbrainz: Cell::new(false),
            listenbrainz_activation_pending: Cell::new(false),
            lastfm: lastfm.clone(),
            syncing_lastfm: Cell::new(false),
            lastfm_activation_pending: Cell::new(false),
            artist_news: artist_news.clone(),
            concerts: concerts.clone(),
            location_broadcast: location_broadcast.clone(),
            podcasts: podcasts.clone(),
            cover_download: cover_download.clone(),
            lyrics_batch: lyrics_batch.clone(),
            cover_batch: cover_batch.clone(),
            artist_portrait: artist_portrait.clone(),
            decorations: decorations.clone(),
            preferences_dialog: RefCell::new(glib::WeakRef::new()),
            preferences_navigation: RefCell::new(glib::WeakRef::new()),
            preferences_stack: RefCell::new(glib::WeakRef::new()),
            preferences_sidebar: RefCell::new(glib::WeakRef::new()),
            location_city_row: RefCell::new(glib::WeakRef::new()),
            connectivity: Cell::new(reprise_core::connectivity::Connectivity::Online),
            on_artwork_permission_changed: RefCell::new(None),
            plugin_rows: RefCell::new(HashMap::new()),
            pending_plugin_targets: RefCell::new(Vec::new()),
        });
        let weak = Rc::downgrade(&context);
        context.scan_button.connect_sensitive_notify(move |button| {
            if button.is_sensitive() {
                if let Some(context) = weak.upgrade() {
                    context.refresh_library_folder_rows();
                }
            }
        });
        context.wire_sidebar_module_menu();
        context.apply_initial();
        context
    }

    fn apply_initial(&self) {
        let (density, sidebar_visible, browse_visible, info_visible, status_visible, decorations) = {
            let conn = &self.conn;
            (
                settings::get_list_density(conn),
                settings::get_sidebar_visible(conn),
                settings::get_browse_visible(conn),
                settings::get_info_panel_visible(conn),
                settings::get_status_visible(conn),
                settings::get_window_decoration_mode(conn),
            )
        };
        self.track_list.apply_list_density(density);
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

    pub(in crate::ui) fn present(self: &Rc<Self>) {
        self.open(None);
    }

    fn open(self: &Rc<Self>, initial_page: Option<&str>) {
        let already_open = self.preferences_dialog.borrow().upgrade().is_some();
        if already_open {
            if let Some(page_name) = initial_page {
                self.navigate_preferences_to(page_name);
            }
            return;
        }
        self.equalizer_controls.borrow_mut().clear();
        self.layout_controls.borrow_mut().take();
        self.online_section.borrow_mut().take();
        self.background_bar.borrow_mut().take();
        self.equalizer_surfaces.borrow_mut().clear();
        self.replaygain_mode.borrow_mut().take();
        self.plugin_rows.borrow_mut().clear();
        use super::preferences_window::PageId;
        // SET-8: handed to the shell as a factory rather than six finished
        // pages, so only the page in sight is built. See
        // `preferences_window::build` for why the shell calls this
        // synchronously. Weak, because the closure lives in the stack, the
        // stack in the dialog — a strong handle would keep this surface alive
        // for as long as the dialog and outlive its own owner.
        let context = Rc::downgrade(self);
        let page_factory: Rc<dyn Fn(PageId) -> adw::PreferencesPage> = Rc::new(move |id| {
            let Some(context) = context.upgrade() else {
                return adw::PreferencesPage::new();
            };
            match id {
                PageId::Playback => context.playback_page(),
                PageId::Appearance => context.appearance_page(),
                PageId::Layout => context.layout_page(),
                PageId::Library => context.library_page(),
                PageId::Location => context.location_page(),
                PageId::Plugins => context.plugins_page(),
            }
        });
        // `SET-18`: nothing goes into the head any more. The library scan
        // keeps its own presentation, but it is given a place in the footer
        // instead of an overlay across the title; the plugin batches get one
        // named row each next to it, so no two jobs share a slot.
        let foreground_scan_progress = ScanChromeView::new();
        self.scan_controls
            .attach_chrome_view(&foreground_scan_progress);
        let background_bar = super::preference_background_bar::BackgroundBar::new();
        background_bar.adopt_scan_chrome(
            foreground_scan_progress.line_widget(),
            foreground_scan_progress.chip_widget(),
        );
        self.wire_background_bar(&background_bar);
        let shell = super::preferences_window::build(page_factory, Some(background_bar.widget()));
        let preferences_sidebar = shell.sidebar.clone();
        foreground_scan_progress.set_on_activate(move || {
            let Some(index) = super::preferences_window::page_index_by_name("library") else {
                return;
            };
            preferences_sidebar.select_row(preferences_sidebar.row_at_index(index).as_ref());
        });
        let scan_controls = self.scan_controls.clone();
        foreground_scan_progress.set_on_cancel(move || scan_controls.request_cancel());
        let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(&self.window);
        let settings_search = shell.search.clone();
        shell.dialog.connect_closed(move |_| {
            let _keep_progress_alive_until_closed = &foreground_scan_progress;
            let _keep_background_bar_alive_until_closed = &background_bar;
            settings_search.close();
        });
        self.preferences_dialog.borrow().set(Some(&shell.dialog));
        self.preferences_navigation
            .borrow()
            .set(Some(&shell.navigation));
        self.preferences_stack.borrow().set(Some(&shell.stack));
        self.preferences_sidebar.borrow().set(Some(&shell.sidebar));

        // Navigate to the requested page by selecting its sidebar row,
        // which drives both the stack and the content title via the
        // row-selected handler.
        if let Some(page_name) = initial_page {
            self.navigate_preferences_to(page_name);
        }

        let smoke = std::env::var(SMOKE_ENV).ok();
        let smoke_page = match smoke.as_deref() {
            Some("columns") => Some("layout"),
            Some(page) if super::preferences_window::page_index_by_name(page).is_some() => {
                Some(page)
            }
            _ => None,
        };
        if let Some(page) = smoke_page {
            let index = super::preferences_window::page_index_by_name(page)
                .expect("smoke page was normalized to a known page");
            shell
                .sidebar
                .select_row(shell.sidebar.row_at_index(index).as_ref());
        }
        let initial_focus =
            super::preferences_window::selected_sidebar_focus_target(&shell.sidebar);
        focus_guard.bind_closable_dialog(&shell.dialog, &initial_focus);
        if smoke.as_deref() == Some("columns") {
            self.open_column_layout_editor();
        }
        if smoke.as_deref() == Some("exercise") {
            let context = Rc::downgrade(self);
            let exercised = Rc::new(Cell::new(false));
            shell.dialog.connect_map(move |_| {
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
        let context = Rc::downgrade(self);
        shell.dialog.connect_map(move |_| {
            let context = context.clone();
            glib::idle_add_local_once(move || {
                if let Some(context) = context.upgrade() {
                    context.highlight_pending_plugin_rows();
                }
            });
        });
        shell.dialog.present(Some(&self.window));
        tracing::debug!("preferences dialog presented");
        if smoke.is_some() {
            let dialog = shell.dialog.clone();
            glib::timeout_add_seconds_local_once(1, move || {
                dialog.force_close();
            });
        }
    }

    fn navigate_preferences_to(&self, page_name: &str) {
        let sidebar = self.preferences_sidebar.borrow().upgrade();
        let Some(sidebar) = sidebar else {
            return;
        };
        let Some(index) = super::preferences_window::page_index_by_name(page_name) else {
            return;
        };
        sidebar.select_row(sidebar.row_at_index(index).as_ref());
    }

    fn apply_smoke(&self) {
        let conn = &self.conn;
        let _ = settings::set_list_density(conn, ListDensity::Compact);
        let _ = settings::set_sidebar_visible(conn, false);
        let _ = settings::set_browse_visible(conn, false);
        let _ = settings::set_info_panel_visible(conn, false);
        let _ = settings::set_status_visible(conn, false);
        let _ = settings::set_window_decoration_mode(
            conn,
            reprise_core::library::settings::WindowDecorationMode::System,
        );
        let _ = settings::set_player_bar_position(conn, PlayerBarPosition::Top);
        let _ = settings::set_equalizer_bands(
            conn,
            equalizer_preset(1)
                .unwrap_or(EqualizerPreset::Flat)
                .ten_band_levels(),
        );
        self.track_list.apply_list_density(ListDensity::Compact);
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

    pub(in crate::ui) fn present_plugins(self: &Rc<Self>, targets: &[&'static str]) {
        *self.pending_plugin_targets.borrow_mut() = targets.to_vec();
        self.open(Some("plugins"));
    }

    /// `SRC-10` addendum (Block B2): the module-off empty state's "Enable
    /// in Preferences" button lands here directly, rather than the plain
    /// Preferences root the user would otherwise have to navigate from.
    pub(in crate::ui) fn present_online_sources(self: &Rc<Self>) {
        self.present_plugins(plugin_targets_for_deep_link(PluginDeepLink::OnlineSources));
    }

    pub(in crate::ui) fn present_artwork_settings(self: &Rc<Self>) {
        self.present_plugins(plugin_targets_for_deep_link(PluginDeepLink::Artwork));
    }

    pub(in crate::ui) fn present_location_settings(self: &Rc<Self>) {
        self.open(Some(SettingsDeepLink::Location.page_name()));
        self.focus_location_city();
    }

    fn appearance_page(self: &Rc<Self>) -> adw::PreferencesPage {
        super::preference_appearance::build(self)
    }

    fn layout_page(self: &Rc<Self>) -> adw::PreferencesPage {
        super::preference_layout::build(self)
    }

    /// `NET-1a`: persists the global online-sources gate and re-derives every
    /// cached module permission. Work starts only when the persisted change
    /// creates a fresh, currently-online off-to-on transition.
    pub(in crate::ui) fn set_online_sources_enabled(
        &self,
        enabled: bool,
    ) -> Result<(), reprise_core::db::DbError> {
        reprise_core::online_sources::set_enabled(&self.conn, enabled)?;
        self.refresh_online_module_state("online sources gate toggled");
        Ok(())
    }

    pub(in crate::ui) fn open_column_layout_editor(&self) {
        let navigation = self.preferences_navigation.borrow().upgrade();
        let Some(navigation) = navigation else {
            tracing::warn!("column layout editor requested without preferences navigation");
            return;
        };
        let page = super::column_layout_editor::build_navigation_page(&self.track_list);
        navigation.push(&page);
    }

    pub(in crate::ui) fn preferences_dialog(&self) -> Option<adw::Dialog> {
        self.preferences_dialog.borrow().upgrade()
    }

    pub(in crate::ui) fn preferences_parent(&self) -> gtk4::Widget {
        self.preferences_dialog()
            .map_or_else(|| self.window.clone().upcast(), gtk4::Widget::from)
    }

    fn playback_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_PLAYBACK))
            .icon_name("audio-speakers-symbolic")
            .build();
        let equalizer_enabled = {
            let conn = &self.conn;
            settings::get_equalizer_enabled(conn)
        };
        let weak = Rc::downgrade(self);
        let on_enabled: Rc<dyn Fn(bool)> = Rc::new(move |active| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            if context.syncing_effect_controls.get() {
                return;
            }
            context.set_equalizer_enabled(active);
        });
        let stored_bands = settings::get_equalizer_bands(&self.conn);
        let weak = Rc::downgrade(self);
        let on_preset: Rc<dyn Fn([f64; 10]) -> bool> = Rc::new(move |bands| {
            let Some(context) = weak.upgrade() else {
                return false;
            };
            if let Err(error) = settings::set_equalizer_bands(&context.conn, bands) {
                tracing::warn!(%error, "could not save equalizer preset");
                return false;
            }
            context.apply_audio_effects();
            true
        });
        let weak = Rc::downgrade(self);
        let on_band: Rc<dyn Fn(usize, f64)> = Rc::new(move |index, value| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            let mut bands = settings::get_equalizer_bands(&context.conn);
            bands[index] = value;
            if let Err(error) = settings::set_equalizer_bands(&context.conn, bands) {
                tracing::warn!(%error, "could not save equalizer bands");
                return;
            }
            context.apply_audio_effects();
        });
        let controls = build_equalizer_controls(
            stored_bands,
            equalizer_enabled,
            on_enabled,
            &on_preset,
            on_band,
        );
        self.equalizer_controls
            .borrow_mut()
            .push(controls.enabled.clone());
        self.equalizer_surfaces
            .borrow_mut()
            .push(controls.root.clone().upcast());
        let equalizer = controls.group;
        // (equalizer/replaygain are added to the page after Audio Transitions
        // below, so Transitions leads the Playback page — matching the mockup.)
        let replaygain = adw::PreferencesGroup::builder()
            .title(strings::text(strings::REPLAYGAIN))
            .build();
        let modes = gtk4::StringList::new(&[
            &strings::text(strings::REPLAYGAIN_OFF),
            &strings::text(strings::REPLAYGAIN_TRACK),
            &strings::text(strings::REPLAYGAIN_ALBUM),
        ]);
        let selected_mode = {
            let conn = &self.conn;
            replay_gain_index(settings::get_replay_gain_mode(conn))
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

        // Audio Transitions: a crossfade slider + a gapless toggle in one
        // group (the "NEW" badge sits in the group header suffix). The two
        // controls are independent; the effective mode is derived from them
        // (see `settings::get_track_transition`).
        let transitions = adw::PreferencesGroup::builder()
            .title(strings::text(strings::AUDIO_TRANSITIONS))
            .build();

        // Both controls sit in one boxed-list card (crossfade on top, gapless
        // below), matching the mockup. We build the list ourselves because the
        // crossfade row is a custom widget — a full-width slider does not fit a
        // standard AdwActionRow, and a non-row widget added straight to the
        // group would fall outside its card.
        let list = gtk4::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk4::SelectionMode::None);

        // Crossfade card row: title + live value + subtitle + a 0..10 s slider
        // ("Off" at 0).
        let stored_crossfade = {
            let conn = &self.conn;
            settings::get_crossfade_seconds(conn)
        };
        let crossfade_content = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        crossfade_content.add_css_class("reprise-crossfade");
        let crossfade_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let crossfade_title = gtk4::Label::new(Some(&strings::text(strings::CROSSFADE)));
        crossfade_title.add_css_class("title");
        crossfade_title.set_xalign(0.0);
        crossfade_title.set_hexpand(true);
        let crossfade_value = gtk4::Label::new(Some(&crossfade_value_label(stored_crossfade)));
        crossfade_value.add_css_class("reprise-crossfade-value");
        crossfade_value.set_halign(gtk4::Align::End);
        crossfade_header.append(&crossfade_title);
        crossfade_header.append(&crossfade_value);
        crossfade_content.append(&crossfade_header);
        let crossfade_subtitle =
            gtk4::Label::new(Some(&strings::text(strings::CROSSFADE_SUBTITLE)));
        crossfade_subtitle.add_css_class("dim-label");
        crossfade_subtitle.set_xalign(0.0);
        crossfade_content.append(&crossfade_subtitle);
        let crossfade_scale = gtk4::Scale::with_range(
            gtk4::Orientation::Horizontal,
            f64::from(settings::CROSSFADE_SECONDS_MIN),
            f64::from(settings::CROSSFADE_SECONDS_MAX),
            1.0,
        );
        crossfade_scale.set_value(f64::from(stored_crossfade));
        crossfade_scale.set_draw_value(false);
        crossfade_scale.set_hexpand(true);
        crossfade_scale.add_css_class("reprise-crossfade-scale");
        crossfade_scale.add_mark(
            0.0,
            gtk4::PositionType::Bottom,
            Some(&strings::text(strings::CROSSFADE_OFF)),
        );
        crossfade_scale.add_mark(5.0, gtk4::PositionType::Bottom, Some("5 s"));
        crossfade_scale.add_mark(10.0, gtk4::PositionType::Bottom, Some("10 s"));
        crossfade_content.append(&crossfade_scale);
        let crossfade_lbrow = gtk4::ListBoxRow::new();
        crossfade_lbrow.set_activatable(false);
        crossfade_lbrow.set_child(Some(&crossfade_content));
        list.append(&crossfade_lbrow);

        // Gapless: a standard switch row, the second row of the same card.
        let gapless_enabled = {
            let conn = &self.conn;
            settings::get_gapless_enabled(conn)
        };
        let gapless = adw::SwitchRow::builder()
            .title(strings::text(strings::GAPLESS_PLAYBACK))
            .active(gapless_enabled)
            .build();
        apply_gapless_control_state(&gapless, stored_crossfade);
        list.append(&gapless);
        transitions.add(&list);

        let weak = Rc::downgrade(self);
        let value_label = crossfade_value.clone();
        let gapless_for_crossfade = gapless.clone();
        crossfade_scale.connect_value_changed(move |scale| {
            let seconds = scale.value().round() as u8;
            value_label.set_label(&crossfade_value_label(seconds));
            apply_gapless_control_state(&gapless_for_crossfade, seconds);
            if let Some(context) = weak.upgrade() {
                context.set_crossfade_seconds(seconds);
            }
        });
        let weak = Rc::downgrade(self);
        gapless.connect_active_notify(move |row| {
            if let Some(context) = weak.upgrade() {
                context.set_gapless_enabled(row.is_active());
            }
        });
        // Order: Audio Transitions first (matching the mockup), then the
        // Equalizer and ReplayGain groups built above.
        page.add(&transitions);
        page.add(&equalizer);
        page.add(&replaygain);
        page
    }

    /// Persists the gapless toggle and pushes the derived transition to the
    /// backend (plus a re-feed) so the change takes effect immediately.
    fn set_gapless_enabled(&self, enabled: bool) {
        {
            let conn = &self.conn;
            if let Err(error) = settings::set_gapless_enabled(conn, enabled) {
                tracing::warn!(%error, "could not save gapless setting");
                return;
            }
        }
        if let Some(player) = &self.player {
            player.apply_transition();
        }
    }

    fn set_crossfade_seconds(&self, seconds: u8) {
        {
            let conn = &self.conn;
            if let Err(error) = settings::set_crossfade_seconds(conn, seconds) {
                tracing::warn!(%error, "could not save crossfade duration");
                return;
            }
        }
        if let Some(player) = &self.player {
            player.apply_transition();
        }
    }
}

pub(in crate::ui) fn action_row(title: &str, callback: Rc<dyn Fn()>) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(strings::text(title))
        .activatable(true)
        .build();
    row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    row.connect_activated(move |_| callback());
    row
}

#[cfg(test)]
#[path = "preferences_tests.rs"]
mod tests;
