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
use super::library_chrome::LibraryTitle;
use super::library_player_bar::LibraryPlayerBarShell;
use super::library_shell::LibraryViews;
use super::minimal_view::MinimalView;
use super::now_playing::NowPlayingPanel;
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
    pub(in crate::ui) search_bar: &'a gtk4::SearchBar,
    pub(in crate::ui) sidebar_toggle: &'a gtk4::ToggleButton,
    pub(in crate::ui) sidebar_page: &'a adw::NavigationPage,
    pub(in crate::ui) split_view: &'a adw::OverlaySplitView,
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
    pub(in crate::ui) info_panel: &'a Rc<NowPlayingPanel>,
    pub(in crate::ui) session_state: &'a SessionState,
    pub(in crate::ui) geometry_guard: &'a Rc<Cell<bool>>,
    pub(in crate::ui) scan_button: &'a gtk4::Button,
    pub(in crate::ui) minimal_view: &'a Rc<MinimalView>,
    pub(in crate::ui) preferences: &'a Rc<PreferencesContext>,
    pub(in crate::ui) cover_batch: &'a Rc<CoverDownloadBatch>,
    pub(in crate::ui) first_run_decision: FirstRunDecision,
    pub(in crate::ui) nav_history: &'a Rc<crate::ui::nav_history::NavHistory>,
}

pub(in crate::ui) fn wire(args: RuntimeWiring<'_>) {
    let RuntimeWiring {
        app,
        window,
        conn,
        db_path,
        header,
        search_entry,
        search_bar,
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
        nav_history,
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
    let cancel_scan_controls = scan_controls.clone();
    let library_menu = super::primary_menu::install(
        header,
        window,
        track_list,
        super::primary_menu::Callbacks {
            on_minimal_view: Rc::new(move || minimal_toggle.toggle()),
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

    if let Some(player) = player {
        // NAV-9a remains Ctrl+L only: jump to the loaded track's origin,
        // select it and center its row.
        let jump_to_current_track = super::current_track_jump::runtime_coordinator(
            &super::current_track_jump::JumpContext {
                player: Rc::downgrade(player),
                sidebar: sidebar.clone(),
                track_list: track_list.clone(),
                nav_history: nav_history.clone(),
                content_stack: content_stack.clone(),
                library_stack: library_views.stack.clone(),
                album_grid: album_view.grid_widget().clone(),
            },
        );

        // GRID-5 is a separate player-surface action: route to Albums,
        // visibly clear search/filter, focus/scroll/pulse the loaded album,
        // and fall back to NAV-9a only if that album is absent.
        let reveal_playing_album =
            super::album_grid_reveal::coordinator(super::album_grid_reveal::RevealSteps {
                current_album: {
                    let player = Rc::downgrade(player);
                    Rc::new(move || {
                        player
                            .upgrade()
                            .and_then(|player| player.current_album_identity())
                    })
                },
                route_to_albums: {
                    let nav_history = nav_history.clone();
                    let sidebar = sidebar.clone();
                    let track_list = track_list.clone();
                    let content_stack = content_stack.clone();
                    let library_stack = library_views.stack.clone();
                    let album_grid = album_view.grid_widget().clone();
                    Rc::new(move || {
                        let place = crate::ui::nav_history::NavPlace::source(
                            ViewSource::Library,
                            Some(super::library_shell::LIBRARY_VIEW_ALBUMS.to_owned()),
                        );
                        super::album_grid_reveal::route_with_history(&nav_history, &place, || {
                            super::library_shell::route_to_place(
                                &place,
                                &sidebar,
                                &track_list,
                                &content_stack,
                                &library_stack,
                                &album_grid,
                                "reveal playing album",
                            );
                        });
                    })
                },
                clear_search: {
                    let search_entry = search_entry.clone();
                    Rc::new(move || search_entry.set_text(""))
                },
                reveal_album: album_view.reveal_callback(),
                fallback_to_track: jump_to_current_track.clone(),
            });
        {
            let reveal = reveal_playing_album.clone();
            player.connect_cover_clicked(move || reveal());
        }
        {
            let reveal = reveal_playing_album.clone();
            player.set_on_title_click(move || reveal());
        }
        {
            let reveal = reveal_playing_album.clone();
            info_panel.set_on_album_reveal(move || reveal());
        }
        let jump_action = gtk4::gio::SimpleAction::new("jump-to-now-playing", None);
        jump_action.connect_activate(move |_, _| jump_to_current_track());
        window.add_action(&jump_action);
        app.set_accels_for_action("win.jump-to-now-playing", &["<Control>l"]);

        // NAV-2 Back: pop the most recent place and route there without
        // re-recording (begin/end_back around the synchronous re-route).
        let back_action = gtk4::gio::SimpleAction::new("nav-back", None);
        {
            let nav_history = nav_history.clone();
            let sidebar = sidebar.clone();
            let track_list = track_list.clone();
            let content_stack = content_stack.clone();
            let library_stack = library_views.stack.clone();
            let album_grid = album_view.grid_widget().clone();
            let restore_album_focus = album_view.restore_focus_callback();
            back_action.connect_activate(move |_, _| {
                let Some(place) = nav_history.go_back() else {
                    tracing::debug!("nav back: history is empty");
                    return;
                };
                let current_source = track_list.current_source();
                super::album_grid_reveal::route_back_restoring_album_focus(
                    &current_source,
                    &place,
                    || {
                        nav_history.begin_back();
                        crate::ui::sidebar_session::sync_current_source(
                            &sidebar.shared,
                            &track_list.current_source(),
                        );
                        // Remember the restored place as current — row-less targets
                        // (Album/Artist) never reach the sidebar choke point that
                        // would otherwise do this. Suppressed by begin_back above.
                        nav_history.record_route(&place);
                        super::library_shell::route_to_place(
                            &place,
                            &sidebar,
                            &track_list,
                            &content_stack,
                            &library_stack,
                            &album_grid,
                            "nav back",
                        );
                        nav_history.end_back();
                    },
                    &restore_album_focus,
                );
            });
        }
        window.add_action(&back_action);
        app.set_accels_for_action("win.nav-back", &["<Alt>Left"]);

        // NAV-2 Forward: the browser counterpart — returns to the place the
        // last Back left, until a new navigation invalidates it.
        let forward_action = gtk4::gio::SimpleAction::new("nav-forward", None);
        {
            let nav_history = nav_history.clone();
            let sidebar = sidebar.clone();
            let track_list = track_list.clone();
            let content_stack = content_stack.clone();
            let library_stack = library_views.stack.clone();
            let album_grid = album_view.grid_widget().clone();
            forward_action.connect_activate(move |_, _| {
                let Some(place) = nav_history.go_forward() else {
                    tracing::debug!("nav forward: nothing ahead");
                    return;
                };
                nav_history.begin_back();
                crate::ui::sidebar_session::sync_current_source(
                    &sidebar.shared,
                    &track_list.current_source(),
                );
                nav_history.record_route(&place);
                super::library_shell::route_to_place(
                    &place,
                    &sidebar,
                    &track_list,
                    &content_stack,
                    &library_stack,
                    &album_grid,
                    "nav forward",
                );
                nav_history.end_back();
            });
        }
        window.add_action(&forward_action);
        app.set_accels_for_action("win.nav-forward", &["<Alt>Right"]);

        // Browser-style mouse navigation buttons: 8 (back) / 9 (forward)
        // fire the same actions as Alt+Left / Alt+Right. One gesture
        // listening to all buttons, claiming ONLY 8/9 so every other button
        // passes through untouched; capture phase on the toplevel so it
        // works over every view.
        let mouse_nav = gtk4::GestureClick::builder()
            .button(0)
            .propagation_phase(gtk4::PropagationPhase::Capture)
            .build();
        {
            let window = window.downgrade();
            mouse_nav.connect_pressed(move |gesture, _n, _x, _y| {
                let action = match gesture.current_button() {
                    8 => "nav-back",
                    9 => "nav-forward",
                    _ => return,
                };
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                if let Some(window) = window.upgrade() {
                    gtk4::gio::prelude::ActionGroupExt::activate_action(&window, action, None);
                }
            });
        }
        window.add_controller(mouse_nav);

        // Dev/verification hook (permanent, like `REPRISE_SMOKE_ACTIVATE`):
        // `REPRISE_SMOKE_JUMP=1` fires the NAV-9a jump action ~2s after
        // startup (past the other smoke hooks' idle work) and the NAV-2
        // back action ~2s later — the exact same `gio` actions Ctrl+L and
        // Alt+Left run. Headless E2E asserts the resulting routing +
        // selection log lines.
        if std::env::var("REPRISE_SMOKE_JUMP").is_ok() {
            // Mirrors the acceptance repro: open Queue through the sidebar,
            // then jump, then back — each step two seconds apart, past
            // startup idle work.
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

        // Dev/verification hook (permanent, like `REPRISE_SMOKE_JUMP`):
        // `REPRISE_SMOKE_FOCUS_ALBUMS=1` opens the Albums tab and hands
        // keyboard focus to the album grid — the deterministic entry point
        // for keyboard-flow E2E (focus ring, Enter, Menu key) without
        // walking the window's whole Tab chain.
        if std::env::var("REPRISE_SMOKE_FOCUS_ALBUMS").is_ok() {
            let library_stack = library_views.stack.clone();
            let grid = album_view.grid_widget().downgrade();
            gtk4::glib::timeout_add_seconds_local_once(2, move || {
                library_stack.set_visible_child_name(super::library_shell::LIBRARY_VIEW_ALBUMS);
                let Some(grid) = grid.upgrade() else { return };
                let granted = grid.grab_focus();
                tracing::info!(granted, "smoke: focused album grid");
            });
        }

        // Dev/verification hook (permanent, like `REPRISE_SMOKE_JUMP`):
        // `REPRISE_SMOKE_ALBUM_BACK=1` drives the album cross-navigation
        // round trip headless — opens the Albums tab, activates the first
        // album card (the same `activate` signal the Enter key fires on a
        // focused cell), fires NAV-2 back, then NAV-2 forward. Headless E2E
        // asserts the "history nav: routing to place" lines restore the
        // albums tab and then the album detail again.
        if std::env::var("REPRISE_SMOKE_ALBUM_BACK").is_ok() {
            let library_stack = library_views.stack.clone();
            gtk4::glib::timeout_add_seconds_local_once(4, move || {
                tracing::info!("smoke: opening albums tab");
                library_stack.set_visible_child_name(super::library_shell::LIBRARY_VIEW_ALBUMS);
            });
            let grid = album_view.grid_widget().downgrade();
            gtk4::glib::timeout_add_seconds_local_once(6, move || {
                let Some(grid) = grid.upgrade() else { return };
                tracing::info!("smoke: activating first album card");
                gtk4::prelude::ObjectExt::emit_by_name::<()>(&grid, "activate", &[&0u32]);
            });
            let window_for_back = window.clone();
            gtk4::glib::timeout_add_seconds_local_once(8, move || {
                tracing::info!("smoke: firing nav-back after album");
                gtk4::gio::prelude::ActionGroupExt::activate_action(
                    &window_for_back,
                    "nav-back",
                    None,
                );
            });
            let window_for_forward = window.clone();
            gtk4::glib::timeout_add_seconds_local_once(10, move || {
                tracing::info!("smoke: firing nav-forward after back");
                gtk4::gio::prelude::ActionGroupExt::activate_action(
                    &window_for_forward,
                    "nav-forward",
                    None,
                );
            });
        }
    }

    let clear_all = gtk4::gio::SimpleAction::new("clear-all-filters", None);
    {
        let track_list = track_list.clone();
        let search_entry = search_entry.clone();
        clear_all.connect_activate(move |_, _| {
            track_list.clear_all_restrictions();
            search_entry.set_text("");
        });
    }
    window.add_action(&clear_all);
    {
        let inner = track_list.clone();
        let entry = search_entry.clone();
        track_list.set_on_search_cleared(move || {
            inner.set_filter("");
            entry.set_text("");
        });
    }
    {
        let window = window.clone();
        track_list.set_on_clear_all(move || {
            gtk4::prelude::ActionGroupExt::activate_action(&window, "clear-all-filters", None);
        });
    }

    cover_batch.start();
    app.set_accels_for_action("win.toggle-minimal-view", &["<Control>m"]);
    app.set_accels_for_action("win.preferences", &["<Control>comma"]);
    app.set_accels_for_action("win.keyboard-shortcuts", &["<Control>question"]);
    app.set_accels_for_action("win.help", &[super::help::HELP_ACCELERATOR]);

    super::window_navigation::wire_sidebar_toggle(sidebar_toggle, split_view, sidebar_page, conn);
    let show_content_if_collapsed = super::window_navigation::show_content_callback(split_view);
    super::library_shell::wire_source_routing(
        sidebar,
        nav_history,
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
    super::shortcuts::wire(app, window, search_bar, search_entry, player.clone());

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
    start_persisted_watcher(
        conn,
        db_path,
        scan_controls,
        track_list,
        sidebar,
        watcher_state,
    );
    super::mounts::install(&super::mounts::MountWiring {
        conn,
        db_path,
        controls: scan_controls,
        toast_overlay,
        track_list,
        sidebar,
        watcher_state,
    });

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
    // NAV-2: session restore selects the source silently (no `on_select`),
    // so seed the history's "current place" here — without it the FIRST
    // cross-navigation after startup (e.g. opening an album from the grid)
    // would have no previous place to push and Back would do nothing.
    nav_history.record_route(&crate::ui::nav_history::NavPlace::source(
        restored_source,
        Some(super::library_shell::LIBRARY_VIEW_TRACKS.to_owned()),
    ));
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
    scan_controls: &ScanControls,
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
            scan_controls.clone(),
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
