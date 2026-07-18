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
use crate::ui::library_player_bar::LibraryPlayerBarShell;
use crate::ui::now_playing::NowPlayingPanel;
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

pub(in crate::ui) const SMOKE_ENV: &str = "REPRISE_SMOKE_PREFERENCES";

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
    pub(in crate::ui) conn: Rc<RefCell<Connection>>,
    pub(in crate::ui) track_list: Rc<TrackList>,
    pub(in crate::ui) sidebar: Rc<Sidebar>,
    pub(in crate::ui) split_view: adw::OverlaySplitView,
    pub(in crate::ui) sidebar_page: adw::NavigationPage,
    pub(in crate::ui) status_bar: StatusBar,
    pub(in crate::ui) library_player_bar: LibraryPlayerBarShell,
    pub(in crate::ui) info_panel: Rc<NowPlayingPanel>,
    pub(in crate::ui) scan_button: gtk4::Button,
    scan_controls: ScanControls,
    pub(in crate::ui) library_folder_rows: RefCell<Vec<glib::WeakRef<adw::ActionRow>>>,
    pub(in crate::ui) player: Option<Rc<PlayerController>>,
    pub(in crate::ui) syncing_effect_controls: Cell<bool>,
    pub(in crate::ui) equalizer_controls: RefCell<Vec<adw::SwitchRow>>,
    pub(in crate::ui) equalizer_surfaces: RefCell<Vec<gtk4::Widget>>,
    pub(in crate::ui) replaygain_mode: RefCell<Option<adw::ComboRow>>,
    pub(in crate::ui) listenbrainz: Rc<ScrobbleRuntime>,
    pub(in crate::ui) syncing_listenbrainz: Cell<bool>,
    pub(in crate::ui) listenbrainz_activation_pending: Cell<bool>,
    pub(in crate::ui) lastfm: Rc<ScrobbleRuntime>,
    pub(in crate::ui) syncing_lastfm: Cell<bool>,
    pub(in crate::ui) lastfm_activation_pending: Cell<bool>,
    pub(in crate::ui) artist_news: Rc<ArtistNewsRuntime>,
    pub(in crate::ui) decorations: Rc<WindowDecorations>,
    pub(in crate::ui) device_sync: Rc<DeviceSyncRuntime>,
    preferences_dialog: RefCell<glib::WeakRef<adw::Dialog>>,
    preferences_navigation: RefCell<glib::WeakRef<adw::NavigationView>>,
    preferences_stack: RefCell<glib::WeakRef<adw::ViewStack>>,
}

impl PreferencesContext {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::ui) fn new(
        window: &adw::ApplicationWindow,
        conn: &Rc<RefCell<Connection>>,
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
            preferences_dialog: RefCell::new(glib::WeakRef::new()),
            preferences_navigation: RefCell::new(glib::WeakRef::new()),
            preferences_stack: RefCell::new(glib::WeakRef::new()),
        });
        let weak = Rc::downgrade(&context);
        context.scan_button.connect_sensitive_notify(move |button| {
            if button.is_sensitive() {
                if let Some(context) = weak.upgrade() {
                    context.refresh_library_folder_rows();
                }
            }
        });
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

    pub(in crate::ui) fn present(self: &Rc<Self>) {
        self.open(None);
    }

    fn open(self: &Rc<Self>, initial_page: Option<&str>) {
        if self.preferences_dialog.borrow().upgrade().is_some() {
            return; // dialog is already open (modal, always on top)
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
            pages,
            Some(foreground_scan_progress.widget().upcast_ref()),
        );
        shell.dialog.connect_closed(move |_| {
            let _keep_progress_alive_until_closed = &foreground_scan_progress;
        });
        self.preferences_dialog.borrow().set(Some(&shell.dialog));
        self.preferences_navigation
            .borrow()
            .set(Some(&shell.navigation));
        self.preferences_stack.borrow().set(Some(&shell.stack));

        // Navigate to the requested page by selecting its sidebar row,
        // which drives both the stack and the content title via the
        // row-selected handler.
        if let Some(page_name) = initial_page {
            if let Some(index) = super::preferences_window::page_index_by_name(page_name) {
                shell
                    .sidebar
                    .select_row(shell.sidebar.row_at_index(index).as_ref());
            }
        }

        let smoke = std::env::var(SMOKE_ENV).ok();
        let smoke_page = match smoke.as_deref() {
            Some("columns") => Some("layout"),
            Some("rhythmbox") => Some("library"),
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
        shell.dialog.present(Some(&self.window));
        tracing::debug!("preferences dialog presented");
        if smoke.is_some() {
            let dialog = shell.dialog.clone();
            glib::timeout_add_seconds_local_once(1, move || {
                dialog.force_close();
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

    /// Opens (or raises) the preferences window and navigates to `page_name`.
    pub(in crate::ui) fn present_page(self: &Rc<Self>, page_name: &str) {
        self.open(Some(page_name));
    }

    fn appearance_page(self: &Rc<Self>) -> adw::PreferencesPage {
        super::preference_appearance::build(self)
    }

    fn layout_page(self: &Rc<Self>) -> adw::PreferencesPage {
        super::preference_layout::build(self)
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

    pub(in crate::ui) fn open_rhythmbox_import(self: &Rc<Self>) {
        self.present_rhythmbox_import_dialog();
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
        // (equalizer/replaygain are added to the page after Audio Transitions
        // below, so Transitions leads the Playback page — matching the mockup.)

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
            let conn = self.conn.borrow();
            settings::get_crossfade_seconds(&conn)
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
            let conn = self.conn.borrow();
            settings::get_gapless_enabled(&conn)
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
            let conn = self.conn.borrow();
            if let Err(error) = settings::set_gapless_enabled(&conn, enabled) {
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
            let conn = self.conn.borrow();
            if let Err(error) = settings::set_crossfade_seconds(&conn, seconds) {
                tracing::warn!(%error, "could not save crossfade duration");
                return;
            }
        }
        if let Some(player) = &self.player {
            player.apply_transition();
        }
    }

    fn plugins_page(self: &Rc<Self>) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_PLUGINS))
            .icon_name("application-x-addon-symbolic")
            .build();
        let group = adw::PreferencesGroup::new();
        for descriptor in reprise_core::modules::ALL_MODULES {
            // Scrobbling services use inline ExpanderRows instead of SwitchRows.
            if descriptor.id == "listenbrainz" {
                group.add(&self.build_listenbrainz_row());
                continue;
            }
            if descriptor.id == "lastfm" {
                group.add(&self.build_lastfm_row());
                continue;
            }

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
                let result = if descriptor.id == "artist_news" {
                    context
                        .artist_news
                        .set_enabled(&context.conn.borrow(), active)
                } else {
                    reprise_core::modules::set_enabled(&context.conn.borrow(), descriptor, active)
                };
                if let Err(error) = result {
                    tracing::warn!(%error, module = descriptor.id, "could not save plugin state");
                    syncing_notify.set(true);
                    row.set_active(!active);
                    syncing_notify.set(false);
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
        }
        page.add(&group);
        page
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
mod tests {
    use super::*;

    #[test]
    fn gapless_control_is_available_only_without_crossfade_overlap() {
        assert!(gapless_control_state(0).sensitive);
        assert_eq!(gapless_control_state(0).subtitle, strings::GAPLESS_SUBTITLE);
        assert!(!gapless_control_state(1).sensitive);
        assert_eq!(
            gapless_control_state(10).subtitle,
            strings::GAPLESS_CROSSFADE_ACTIVE_SUBTITLE
        );
    }

    #[test]
    fn only_runtime_safe_plugins_apply_without_restart() {
        assert!(!plugin_applies_live("cover_download"));
        assert!(plugin_applies_live("listenbrainz"));
        assert!(plugin_applies_live("lastfm"));
        assert!(plugin_applies_live("artist_news"));
        assert!(!plugin_applies_live("artist_portrait"));
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
}
