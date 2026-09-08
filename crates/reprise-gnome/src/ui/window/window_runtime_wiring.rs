//! Post-composition wiring for the main window.
//!
//! `window::build` constructs the object graph. This module connects runtime
//! callbacks, startup restoration, scan/watcher triggers, and smoke hooks once
//! every participant exists.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library::session::SessionState;
use reprise_core::library::watcher::WatcherHandle;

use super::cover_download_batch::CoverDownloadBatch;
use super::first_run::FirstRunDecision;
use super::library_player_bar::LibraryPlayerBarShell;
use super::lyrics_batch::LyricsBatch;
use super::minimal_view::MinimalView;
use super::now_playing::NowPlayingPanel;
use super::player_controller::PlayerController;
use super::preferences::PreferencesContext;
use super::scan_flow::ScanControls;
use super::sidebar::Sidebar;
use super::stats_view::StatsView;
use super::track_list::TrackList;
use super::{
    library_shell, podcast_refresh_scheduler, section_search as section_search_ui,
    section_search_wiring, spectrogram_backend, table_columns, window_navigation, window_smoke,
};
use crate::ui::{
    compact_mode_controls, compact_mode_suggestion, first_run, help,
    library_doctor as library_doctor_ui, lyrics_smoke, mounts, playlist_io, primary_menu,
    scan_flow, scan_worker, session_restore as session_restore_ui, shortcuts,
    spectrogram_batch_progress, startup_quiet, startup_report, view_session as view_session_ui,
};

#[path = "window_artwork_permission_wiring.rs"]
mod artwork_permission_wiring;
#[path = "wiring/clear_all.rs"]
mod clear_all;
#[path = "wiring/close.rs"]
mod close;
#[path = "wiring/compact_mode.rs"]
mod compact_mode;
#[path = "wiring/deep_link.rs"]
mod deep_link;
#[path = "window_deferred_source_wiring.rs"]
mod deferred_source_wiring;
#[path = "wiring/deferred_sources.rs"]
mod deferred_sources;
#[path = "window_external_changes_wiring.rs"]
pub(in crate::ui) mod external_changes_wiring;
#[path = "wiring/library_doctor.rs"]
mod library_doctor;
#[path = "wiring/listeners.rs"]
mod listeners;
#[path = "wiring/menu.rs"]
mod menu;
#[path = "wiring/nav_back.rs"]
mod nav_back;
#[path = "wiring/playing_source.rs"]
mod playing_source;
#[path = "window_playing_source_wiring.rs"]
mod playing_source_wiring;
#[path = "wiring/section_search.rs"]
mod section_search;
#[path = "wiring/session_restore.rs"]
mod session_restore;
#[path = "wiring/view_session.rs"]
mod view_session;
#[path = "wiring/mod.rs"]
mod wiring;

use wiring::WiringScratch;

#[derive(Clone, Copy)]
pub(in crate::ui) struct RuntimeWiring<'a> {
    pub(in crate::ui) app: &'a adw::Application,
    pub(in crate::ui) window: &'a adw::ApplicationWindow,
    pub(in crate::ui) conn: &'a Rc<Db>,
    pub(in crate::ui) db_path: &'a Path,
    pub(in crate::ui) header: &'a adw::HeaderBar,
    pub(in crate::ui) search_entry: &'a gtk4::SearchEntry,
    pub(in crate::ui) search: &'a super::search_popover::SearchPopover,
    pub(in crate::ui) search_toggle: &'a gtk4::ToggleButton,
    pub(in crate::ui) sidebar_toggle: &'a gtk4::ToggleButton,
    pub(in crate::ui) sidebar_page: &'a adw::NavigationPage,
    pub(in crate::ui) split_view: &'a adw::OverlaySplitView,
    pub(in crate::ui) track_list: &'a Rc<TrackList>,
    pub(in crate::ui) sidebar: &'a Rc<Sidebar>,
    pub(in crate::ui) player: &'a Option<Rc<PlayerController>>,
    pub(in crate::ui) stats_view: &'a super::content_stack::DeferredPage<StatsView>,
    pub(in crate::ui) concerts_view:
        &'a super::content_stack::DeferredPage<crate::ui::concerts::ConcertsView>,
    pub(in crate::ui) releases_view: &'a Rc<crate::ui::releases::ReleasesView>,
    pub(in crate::ui) podcasts_view:
        &'a super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    pub(in crate::ui) youtube_view:
        &'a super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    pub(in crate::ui) radio_view:
        &'a super::content_stack::DeferredPage<crate::ui::radio::RadioView>,
    pub(in crate::ui) podcasts_runtime: &'a Rc<crate::ui::podcasts::PodcastsRuntime>,
    pub(in crate::ui) device_sync:
        &'a Rc<crate::ui::device_sync::device_sync_runtime::DeviceSyncRuntime>,
    pub(in crate::ui) content_stack: &'a gtk4::Stack,
    pub(in crate::ui) library_doctor_navigation: &'a adw::NavigationView,
    pub(in crate::ui) doctor_chrome: &'a Rc<super::library_chrome::DoctorChrome>,
    pub(in crate::ui) window_title: &'a adw::WindowTitle,
    pub(in crate::ui) scan_controls: &'a ScanControls,
    pub(in crate::ui) toast_overlay: &'a adw::ToastOverlay,
    pub(in crate::ui) watcher_state: &'a Rc<RefCell<Option<WatcherHandle>>>,
    pub(in crate::ui) library_player_bar: &'a LibraryPlayerBarShell,
    pub(in crate::ui) info_panel: &'a Rc<NowPlayingPanel>,
    pub(in crate::ui) session_state: &'a SessionState,
    pub(in crate::ui) geometry_guard: &'a Rc<Cell<bool>>,
    pub(in crate::ui) scan_button: &'a gtk4::Button,
    pub(in crate::ui) minimal_view: &'a Rc<MinimalView>,
    pub(in crate::ui) preferences: &'a Rc<PreferencesContext>,
    pub(in crate::ui) cover_batch: &'a Rc<CoverDownloadBatch>,
    pub(in crate::ui) lyrics_batch: &'a Rc<LyricsBatch>,
    pub(in crate::ui) first_run_decision: FirstRunDecision,
    pub(in crate::ui) nav_history: &'a Rc<crate::ui::nav_history::NavHistory>,
    pub(in crate::ui) content_nav: &'a adw::NavigationView,
    pub(in crate::ui) active_content_focus: &'a super::library_shell::ActiveContentFocus,
    pub(in crate::ui) metadata_navigator: &'a super::metadata_navigation::MetadataNavigator,
}

pub(in crate::ui) fn wire(args: RuntimeWiring<'_>) {
    let scratch = WiringScratch::new(&args);

    // This order is load-bearing: listeners precede session restore, and close
    // wiring follows view-session wiring.
    deferred_sources::wire_deferred_sources(&args);
    library_doctor::wire_library_doctor(&args, &scratch);
    compact_mode::wire_compact_mode(&args);
    menu::wire_menu(&args, &scratch);
    playing_source::wire_playing_source(&args);
    nav_back::wire_nav_back(&args, &scratch);
    section_search::wire_section_search(&args, &scratch);
    clear_all::wire_clear_all(&args, &scratch);
    listeners::wire_listeners(&args);
    view_session::wire_view_session(&args, &scratch);
    close::wire_close(&args);
    session_restore::wire_session_restore(&args, &scratch);
    deep_link::wire_deep_link(&args, &scratch);
}
