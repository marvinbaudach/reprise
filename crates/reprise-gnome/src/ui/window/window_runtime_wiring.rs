//! Post-composition wiring for the main window.
//!
//! `window::build` constructs the object graph. This module connects runtime
//! callbacks, startup restoration, scan/watcher triggers, and smoke hooks once
//! every participant exists.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::session::SessionState;
use reprise_core::library::watcher::WatcherHandle;
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use super::album_view::AlbumView;
use super::cover_download_batch::CoverDownloadBatch;
use super::device_view::DeviceViewPage;
use super::first_run::FirstRunDecision;
use super::info_panel::InfoPanel;
use super::library_chrome::LibraryTitle;
use super::library_player_bar::LibraryPlayerBarShell;
use super::library_shell::LibraryViews;
use super::minimal_view::MinimalView;
use super::player_controller::PlayerController;
use super::preferences::PreferencesContext;
use super::scan_flow::ScanControls;
use super::sidebar::Sidebar;
use super::stats_view::StatsView;
use super::track_list::TrackList;

const SMOKE_QUIT_ENV_VAR: &str = "REPRISE_SMOKE_QUIT";
const SMOKE_QUIT_DELAY_SECS_ENV_VAR: &str = "REPRISE_SMOKE_QUIT_DELAY_SECS";
const SMOKE_QUIT_DELAY_SECS_DEFAULT: u32 = 3;

pub(in crate::ui) struct RuntimeWiring<'a> {
    pub(in crate::ui) app: &'a adw::Application,
    pub(in crate::ui) window: &'a adw::ApplicationWindow,
    pub(in crate::ui) conn: &'a Rc<RefCell<Connection>>,
    pub(in crate::ui) db_path: &'a Path,
    pub(in crate::ui) header: &'a adw::HeaderBar,
    pub(in crate::ui) search_entry: &'a gtk4::SearchEntry,
    pub(in crate::ui) sidebar_toggle: &'a gtk4::ToggleButton,
    pub(in crate::ui) sidebar_page: &'a adw::NavigationPage,
    pub(in crate::ui) split_view: &'a adw::NavigationSplitView,
    pub(in crate::ui) track_list: &'a Rc<TrackList>,
    pub(in crate::ui) sidebar: &'a Rc<Sidebar>,
    pub(in crate::ui) player: &'a Option<Rc<PlayerController>>,
    pub(in crate::ui) stats_view: StatsView,
    pub(in crate::ui) content_stack: &'a gtk4::Stack,
    pub(in crate::ui) device_view: &'a Rc<DeviceViewPage>,
    pub(in crate::ui) library_views: &'a LibraryViews,
    pub(in crate::ui) library_title: &'a Rc<LibraryTitle>,
    pub(in crate::ui) window_title: &'a adw::WindowTitle,
    pub(in crate::ui) album_view: &'a AlbumView,
    pub(in crate::ui) scan_controls: &'a ScanControls,
    pub(in crate::ui) toast_overlay: &'a adw::ToastOverlay,
    pub(in crate::ui) watcher_state: &'a Rc<RefCell<Option<WatcherHandle>>>,
    pub(in crate::ui) library_player_bar: &'a LibraryPlayerBarShell,
    pub(in crate::ui) info_panel: &'a Rc<InfoPanel>,
    pub(in crate::ui) session_state: &'a SessionState,
    pub(in crate::ui) geometry_guard: &'a Rc<Cell<bool>>,
    pub(in crate::ui) scan_button: &'a gtk4::Button,
    pub(in crate::ui) minimal_view: &'a Rc<MinimalView>,
    pub(in crate::ui) preferences: &'a Rc<PreferencesContext>,
    pub(in crate::ui) cover_batch: &'a Rc<CoverDownloadBatch>,
    pub(in crate::ui) first_run_decision: FirstRunDecision,
}

pub(in crate::ui) fn wire(args: RuntimeWiring<'_>) {
    let RuntimeWiring {
        app,
        window,
        conn,
        db_path,
        header,
        search_entry,
        sidebar_toggle,
        sidebar_page,
        split_view,
        track_list,
        sidebar,
        player,
        stats_view,
        content_stack,
        device_view,
        library_views,
        library_title,
        window_title,
        album_view,
        scan_controls,
        toast_overlay,
        watcher_state,
        library_player_bar,
        info_panel,
        session_state,
        geometry_guard,
        scan_button,
        minimal_view,
        preferences,
        cover_batch,
        first_run_decision,
    } = args;

    let minimal_toggle = minimal_view.clone();
    let compact_preferences = preferences.clone();
    super::compact_mode_controls::install(
        window,
        minimal_view,
        player.as_ref().map(|player| &player.compact_player),
        conn,
        Rc::new(move || compact_preferences.present()),
    );

    let rescan_conn = conn.clone();
    let rescan_scan_controls = scan_controls.clone();
    let rescan_toast_overlay = toast_overlay.clone();
    let rescan_db_path = db_path.to_path_buf();
    let rescan_track_list = track_list.clone();
    let rescan_sidebar = sidebar.clone();
    let rescan_watcher_state = watcher_state.clone();
    let sync_preferences = preferences.clone();
    let menu_preferences = preferences.clone();
    let stats_sidebar = sidebar.clone();
    let cancel_scan_controls = scan_controls.clone();
    let library_menu = super::primary_menu::install(
        header,
        window,
        track_list,
        super::primary_menu::Callbacks {
            on_minimal_view: Rc::new(move || minimal_toggle.toggle()),
            on_my_stats: Rc::new(move || {
                stats_sidebar.refresh_and_select(ViewSource::MyStats, "primary menu");
            }),
            on_rescan_library: Rc::new(move || {
                super::scan_flow::trigger_rescan_of_library_root(
                    &rescan_conn,
                    &rescan_scan_controls,
                    &rescan_toast_overlay,
                    rescan_db_path.clone(),
                    rescan_track_list.clone(),
                    rescan_sidebar.clone(),
                    &rescan_watcher_state,
                );
            }),
            on_cancel_scan: Rc::new(move || cancel_scan_controls.request_cancel()),
            on_sync_device: Rc::new(move || {
                sync_preferences.present_page("synchronization");
            }),
            on_preferences: Rc::new(move || menu_preferences.present()),
        },
        scan_controls,
    );
    scan_controls.set_on_scan_state_changed({
        let library_menu = library_menu.clone();
        move |is_scanning| {
            super::primary_menu::update_library_section(&library_menu, is_scanning);
        }
    });

    let nav_history = Rc::new(crate::ui::nav_history::NavHistory::default());
    if let Some(player) = player {
        let sidebar_for_queue = sidebar.clone();
        player.bar.connect_queue_clicked(move || {
            sidebar_for_queue.refresh_and_select(ViewSource::Queue, "player bar queue button");
        });

        // NAV-9 "Jump to Now Playing": cover/title clicks and Ctrl+L
        // navigate to the playing track's home (its play origin), then
        // select + center its row. The sidebar routing pushes the left
        // place onto the NAV-2 history, so Back returns here.
        let jump_to_now_playing = {
            let player = Rc::downgrade(player);
            let sidebar = sidebar.clone();
            let track_list = track_list.clone();
            Rc::new(move || {
                let Some(player) = player.upgrade() else {
                    return;
                };
                let origin = player
                    .current_play_origin()
                    .map_or(ViewSource::Library, |origin| origin.source);
                // An explicit jump supersedes NAV-5's remembered viewport
                // for the target — centering must own the scroll position.
                track_list.forget_view_state(&origin);
                // Re-baseline the sidebar's dedup: cross-navigation paths
                // (album/artist cards, smoke hooks) switch the table without
                // it, and a stale baseline would swallow this selection.
                crate::ui::sidebar_session::sync_current_source(
                    &sidebar.shared,
                    &track_list.current_source(),
                );
                sidebar.refresh_and_select(origin, "jump to now playing");
                // Deferred one main-loop round: the routed reload above has
                // scheduled idle work of its own; centering runs after the
                // rebuilt list exists (select_current_track keeps its own
                // no-geometry fallback).
                let player = Rc::downgrade(&player);
                gtk4::glib::idle_add_local_once(move || {
                    if let Some(player) = player.upgrade() {
                        player.notify_restored_current_track();
                    }
                });
            })
        };
        {
            let jump = jump_to_now_playing.clone();
            player.connect_cover_clicked(move || jump());
        }
        {
            let jump = jump_to_now_playing.clone();
            player.set_on_title_click(move || jump());
        }
        let jump_action = gtk4::gio::SimpleAction::new("jump-to-now-playing", None);
        jump_action.connect_activate(move |_, _| jump_to_now_playing());
        window.add_action(&jump_action);
        app.set_accels_for_action("win.jump-to-now-playing", &["<Control>l"]);

        // NAV-2 Back: pop the most recent place and route there without
        // re-recording (begin/end_back around the synchronous re-route).
        let back_action = gtk4::gio::SimpleAction::new("nav-back", None);
        {
            let nav_history = nav_history.clone();
            let sidebar = sidebar.clone();
            let track_list = track_list.clone();
            back_action.connect_activate(move |_, _| {
                let Some(target) = nav_history.pop() else {
                    tracing::debug!("nav back: history is empty");
                    return;
                };
                nav_history.begin_back();
                crate::ui::sidebar_session::sync_current_source(
                    &sidebar.shared,
                    &track_list.current_source(),
                );
                sidebar.refresh_and_select(target, "nav back");
                nav_history.end_back();
            });
        }
        window.add_action(&back_action);
        app.set_accels_for_action("win.nav-back", &["<Alt>Left"]);

        // Dev/verification hook (permanent, like `REPRISE_SMOKE_ACTIVATE`):
        // `REPRISE_SMOKE_JUMP=1` fires the NAV-9 jump action ~2s after
        // startup (past the other smoke hooks' idle work) and the NAV-2
        // back action ~2s later — the exact same `gio` actions Ctrl+L and
        // Alt+Left run. Headless E2E asserts the resulting routing +
        // selection log lines.
        if std::env::var("REPRISE_SMOKE_JUMP").is_ok() {
            // Mirrors the acceptance repro: open the Queue THROUGH the
            // sidebar (like the player bar's queue button), then jump, then
            // back — each step two seconds apart, past startup idle work.
            let sidebar_for_smoke = sidebar.clone();
            gtk4::glib::timeout_add_seconds_local_once(2, move || {
                tracing::info!("smoke: selecting queue via sidebar");
                sidebar_for_smoke.refresh_and_select(ViewSource::Queue, "smoke jump precondition");
            });
            let window_for_jump = window.clone();
            gtk4::glib::timeout_add_seconds_local_once(4, move || {
                tracing::info!("smoke: firing jump-to-now-playing");
                gtk4::gio::prelude::ActionGroupExt::activate_action(
                    &window_for_jump,
                    "jump-to-now-playing",
                    None,
                );
            });
            let window_for_back = window.clone();
            gtk4::glib::timeout_add_seconds_local_once(6, move || {
                tracing::info!("smoke: firing nav-back");
                gtk4::gio::prelude::ActionGroupExt::activate_action(
                    &window_for_back,
                    "nav-back",
                    None,
                );
            });
        }
    }

    header.pack_end(search_entry);
    cover_batch.start();
    app.set_accels_for_action("win.toggle-minimal-view", &["<Control>m"]);
    app.set_accels_for_action("win.preferences", &["<Control>comma"]);
    app.set_accels_for_action("win.keyboard-shortcuts", &["<Control>question"]);
    app.set_accels_for_action("win.help", &[super::help::HELP_ACCELERATOR]);

    super::window_navigation::wire_sidebar_toggle(sidebar_toggle, split_view, sidebar_page, conn);
    let show_content_if_collapsed = super::window_navigation::show_content_callback(split_view);
    super::library_shell::wire_source_routing(
        sidebar,
        &nav_history,
        track_list,
        stats_view,
        conn,
        content_stack,
        device_view,
        library_views,
        library_title,
        window_title,
        show_content_if_collapsed,
    );

    let track_list_weak = Rc::downgrade(track_list);
    sidebar.set_on_tracks_added(move || match track_list_weak.upgrade() {
        Some(track_list) => track_list.reload(),
        None => tracing::warn!("track list reload skipped: track list is gone"),
    });
    let sidebar_weak = Rc::downgrade(sidebar);
    track_list.set_on_sidebar_playlist_drop(move |playlist_id, playlist_name, ids| {
        match sidebar_weak.upgrade() {
            Some(sidebar) => sidebar.handle_playlist_drop(playlist_id, playlist_name, ids),
            None => {
                tracing::warn!("sidebar is gone; cannot dispatch simulated playlist drop");
                false
            }
        }
    });
    let sidebar_weak = Rc::downgrade(sidebar);
    track_list.set_on_sidebar_queue_drop(move |ids| match sidebar_weak.upgrade() {
        Some(sidebar) => sidebar.handle_queue_drop(ids),
        None => {
            tracing::warn!("sidebar is gone; cannot dispatch simulated queue drop");
            false
        }
    });

    let search_restore_guard = super::view_session::new_search_restore_guard();
    super::view_session::wire_search(
        search_entry,
        track_list.clone(),
        search_restore_guard.clone(),
    );
    {
        use gtk4::prelude::EditableExt as _;
        let album_filter = album_view.filter_callback();
        search_entry.connect_search_changed(move |entry| album_filter(&entry.text()));
    }
    super::view_session::arm_smoke(
        search_entry,
        track_list,
        sidebar,
        window_title,
        &search_restore_guard,
    );
    super::shortcuts::wire(app, window, search_entry, track_list, player.clone());

    super::scan_flow::wire_scan_button(
        scan_controls,
        window,
        toast_overlay,
        db_path.to_path_buf(),
        track_list.clone(),
        sidebar.clone(),
        watcher_state.clone(),
    );
    super::scan_flow::arm_smoke_rescan(
        scan_controls,
        toast_overlay,
        db_path.to_path_buf(),
        track_list.clone(),
        sidebar.clone(),
        watcher_state.clone(),
    );
    start_persisted_watcher(conn, db_path, track_list, sidebar, watcher_state);

    super::playlist_io::wire_import_action(window, toast_overlay, conn.clone(), sidebar);
    super::playlist_io::arm_smoke_m3u(conn.clone(), toast_overlay, sidebar.clone());
    super::window_smoke::arm_bar_position(conn, library_player_bar);
    super::lyrics_smoke::arm(player.as_ref(), info_panel, conn);

    super::session_restore::restore_runtime(
        search_entry,
        track_list,
        sidebar,
        window_title,
        &search_restore_guard,
        player.as_ref(),
        session_state,
    );
    let restored_source = super::view_session::snapshot(track_list).source;
    library_title.set_library_navigation_visible(matches!(restored_source, ViewSource::Library));
    super::session_restore::wire_close(
        window,
        conn,
        track_list,
        player.as_ref(),
        session_state,
        geometry_guard,
    );
    super::session_restore::arm_seed_close(window);
    super::first_run::run(window, scan_button, conn, first_run_decision);
    minimal_view.apply_initial();
    arm_smoke_quit(window);
}

fn start_persisted_watcher(
    conn: &Rc<RefCell<Connection>>,
    db_path: &Path,
    track_list: &Rc<TrackList>,
    sidebar: &Rc<Sidebar>,
    watcher_state: &Rc<RefCell<Option<WatcherHandle>>>,
) {
    let root = {
        let conn = conn.borrow();
        reprise_core::library::settings::get_library_root(&conn)
    };
    match root {
        Ok(Some(root)) => super::scan_flow::start_or_restart_watcher(
            watcher_state,
            &PathBuf::from(root),
            db_path.to_path_buf(),
            Rc::downgrade(track_list),
            Rc::downgrade(sidebar),
        ),
        Ok(None) => tracing::debug!("no persisted library root; watcher not started at startup"),
        Err(error) => tracing::error!(%error, "failed to read persisted library root at startup"),
    }
}

fn arm_smoke_quit(window: &adw::ApplicationWindow) {
    if std::env::var(SMOKE_QUIT_ENV_VAR).is_err() {
        return;
    }
    let delay_secs = std::env::var(SMOKE_QUIT_DELAY_SECS_ENV_VAR)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(SMOKE_QUIT_DELAY_SECS_DEFAULT);
    tracing::info!(
        delay_secs,
        "{SMOKE_QUIT_ENV_VAR} set: arming headless smoke-quit timer"
    );
    let window = window.clone();
    glib::timeout_add_seconds_local(delay_secs, move || {
        tracing::info!("smoke-quit timer fired: closing main window");
        window.close();
        glib::ControlFlow::Break
    });
}
