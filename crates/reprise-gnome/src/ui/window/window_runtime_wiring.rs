//! Post-composition wiring for the main window.
//!
//! `window::build` constructs the object graph. This module connects runtime
//! callbacks, startup restoration, scan/watcher triggers, and smoke hooks once
//! every participant exists.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::browser::navigation::NavigationIntent;
use reprise_core::browser::{AlbumKey, ArtistKey, BrowserPlace};
use reprise_core::library::session::SessionState;
use reprise_core::library::watcher::WatcherHandle;
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use super::cover_download_batch::CoverDownloadBatch;
use super::device_view::DeviceViewPage;
use super::first_run::FirstRunDecision;
use super::library_player_bar::LibraryPlayerBarShell;
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
    pub(in crate::ui) concerts_view: &'a Rc<crate::ui::concerts::ConcertsView>,
    pub(in crate::ui) releases_view: &'a Rc<crate::ui::releases::ReleasesView>,
    pub(in crate::ui) content_stack: &'a gtk4::Stack,
    pub(in crate::ui) device_view: &'a Rc<DeviceViewPage>,
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
    pub(in crate::ui) first_run_decision: FirstRunDecision,
    pub(in crate::ui) nav_history: &'a Rc<crate::ui::nav_history::NavHistory>,
    pub(in crate::ui) content_nav: &'a adw::NavigationView,
    pub(in crate::ui) active_content_focus: &'a super::library_shell::ActiveContentFocus,
    pub(in crate::ui) metadata_navigator: &'a super::metadata_navigation::MetadataNavigator,
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
        concerts_view,
        releases_view,
        content_stack,
        device_view,
        window_title,
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
        content_nav,
        active_content_focus,
        metadata_navigator,
    } = args;

    let refresh_doctor_views = {
        let stats = stats_view.clone();
        let conn = conn.clone();
        Rc::new(move || {
            stats.refresh(&conn);
        }) as Rc<dyn Fn()>
    };
    let library_doctor = super::library_doctor::LibraryDoctorCoordinator::new(
        super::library_doctor::LibraryDoctorContext {
            conn,
            db_path,
            navigation: content_nav,
            window,
            track_list,
            scan_controls,
            fingerprint: Arc::new(reprise_platform_linux::fingerprint::GstreamerFingerprintBackend),
            preferences,
            sidebar,
            toast_overlay,
            refresh_views: refresh_doctor_views,
        },
    );
    {
        let library_doctor = Rc::downgrade(&library_doctor);
        stats_view.set_on_unify_spellings(move |ids| {
            if let Some(library_doctor) = library_doctor.upgrade() {
                library_doctor.open_for_selection(ids);
            }
        });
    }

    let active_content_focus = active_content_focus.clone();

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
    let menu_library_doctor = library_doctor;
    let stop_player = player.as_ref().map(|player| {
        let player = Rc::downgrade(player);
        Rc::new(move || {
            if let Some(player) = player.upgrade() {
                player.reset_to_stopped();
            }
        }) as Rc<dyn Fn()>
    });
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
            on_library_doctor: Rc::new(move || menu_library_doctor.open()),
            on_sync_device: Rc::new(move || {
                sync_preferences.present_page("synchronization");
            }),
            on_stop_playback: stop_player,
            on_preferences: Rc::new(move || menu_preferences.present()),
        },
        scan_controls,
    );
    app.set_accels_for_action("win.open-primary-menu", &["F10"]);
    scan_controls.set_on_scan_state_changed({
        let library_menu = library_menu.clone();
        move |is_scanning| {
            super::primary_menu::update_library_section(&library_menu, is_scanning);
        }
    });

    if let Some(player) = player {
        // BROWSE-4: every metadata surface emits one of the same three
        // semantic navigation intents. The router owns history and anchors.
        let reveal_playing_track: Rc<dyn Fn()> = {
            let player = Rc::downgrade(player);
            let navigator = metadata_navigator.clone();
            Rc::new(move || {
                let Some(player) = player.upgrade() else {
                    return;
                };
                let Some(track_id) = player.current_track_id() else {
                    return;
                };
                let origin = player.current_play_origin().map_or_else(
                    || BrowserPlace::from(ViewSource::Library),
                    |origin| origin.place,
                );
                navigator.navigate(
                    NavigationIntent::RevealTrack {
                        origin: Box::new(origin),
                        track_id,
                    },
                    "playing track link",
                );
            })
        };
        let reveal_playing_album: Rc<dyn Fn()> = {
            let player = Rc::downgrade(player);
            let navigator = metadata_navigator.clone();
            Rc::new(move || {
                let Some(player) = player.upgrade() else {
                    return;
                };
                let Some((album, album_artist)) = player.current_album_identity() else {
                    return;
                };
                navigator.navigate(
                    NavigationIntent::OpenAlbum {
                        album: AlbumKey::new(album, album_artist),
                        anchor_track_id: player.current_track_id(),
                    },
                    "playing album link",
                );
            })
        };
        let reveal_playing_artist: Rc<dyn Fn()> = {
            let player = Rc::downgrade(player);
            let navigator = metadata_navigator.clone();
            Rc::new(move || {
                let Some(player) = player.upgrade() else {
                    return;
                };
                let Some(artist) = player.current_artist_identity() else {
                    return;
                };
                navigator.navigate(
                    NavigationIntent::OpenArtist {
                        artist: ArtistKey::new(artist),
                        anchor_track_id: player.current_track_id(),
                    },
                    "playing artist link",
                );
            })
        };
        {
            let reveal = reveal_playing_album.clone();
            player.connect_cover_clicked(move || reveal());
        }
        {
            let reveal = reveal_playing_track.clone();
            player.set_on_title_click(move || reveal());
        }
        {
            let reveal = reveal_playing_artist.clone();
            player.connect_artist_clicked(move || reveal());
        }
        {
            let reveal = reveal_playing_album.clone();
            info_panel.set_on_album_reveal(move || reveal());
        }
        {
            let reveal = reveal_playing_track.clone();
            info_panel.set_on_track_reveal(move || reveal());
        }
        {
            let reveal = reveal_playing_artist.clone();
            info_panel.set_on_artist_reveal(move || reveal());
        }
        let jump_action = gtk4::gio::SimpleAction::new("jump-to-now-playing", None);
        jump_action.connect_activate(move |_, _| reveal_playing_track());
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
            let window_title = window_title.clone();
            let active_content_focus = active_content_focus.clone();
            back_action.connect_activate(move |_, _| {
                let Some(place) = nav_history.go_back_from(track_list.browser_place()) else {
                    tracing::debug!("nav back: history is empty");
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
                    &window_title,
                    &active_content_focus,
                    "nav back",
                );
                nav_history.end_back();
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
            let window_title = window_title.clone();
            let active_content_focus = active_content_focus.clone();
            forward_action.connect_activate(move |_, _| {
                let Some(place) = nav_history.go_forward_from(track_list.browser_place()) else {
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
                    &window_title,
                    &active_content_focus,
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
        // input-parity: ACC-8 keyboard=alt-left-right
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
        // `REPRISE_SMOKE_JUMP=1` fires the NAV-9b jump action ~2s after
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
    {
        let navigator = metadata_navigator.clone();
        track_list.set_on_scope_cleared(move || {
            navigator.leave_scope();
        });
    }

    cover_batch.start();
    app.set_accels_for_action("win.toggle-minimal-view", &["<Control>m"]);
    app.set_accels_for_action("win.preferences", &["<Control>comma"]);
    app.set_accels_for_action("win.keyboard-shortcuts", &["<Control>question"]);
    app.set_accels_for_action("win.help", &[super::help::HELP_ACCELERATOR]);

    super::window_navigation::wire_sidebar_toggle(sidebar_toggle, split_view, sidebar_page, conn);
    let show_content_if_collapsed =
        super::window_navigation::show_content_callback(split_view, content_nav);
    super::library_shell::wire_source_routing(
        sidebar,
        nav_history,
        track_list,
        stats_view,
        concerts_view,
        releases_view,
        conn,
        content_stack,
        device_view,
        window_title,
        show_content_if_collapsed,
        &active_content_focus,
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
    super::view_session::arm_smoke(
        search_entry,
        track_list,
        sidebar,
        window_title,
        &search_restore_guard,
    );
    let focus_active_content: Rc<dyn Fn() -> bool> = {
        let active_content_focus = active_content_focus.clone();
        Rc::new(move || active_content_focus.focus())
    };
    super::shortcuts::wire(
        app,
        window,
        search_bar,
        search_entry,
        focus_active_content,
        player.clone(),
    );

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
    start_external_changes_refresh(db_path, track_list, sidebar);
    crate::ui::instrumental::conversion_wiring::install(
        &crate::ui::instrumental::conversion_wiring::ConversionWiring {
            conn,
            db_path,
            window,
            content_stack,
            toast_overlay,
            track_list,
            player,
        },
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

    super::session_restore::restore_runtime(player.as_ref(), session_state);
    let restored_place = session_state
        .browser_place
        .clone()
        .unwrap_or_else(|| BrowserPlace::from(ViewSource::Library));
    let restored_root = session_state
        .library_root
        .clone()
        .unwrap_or_else(|| BrowserPlace::from(ViewSource::Library));
    nav_history.restore(restored_place.clone(), restored_root);
    nav_history.begin_back();
    super::library_shell::route_to_place(
        &crate::ui::nav_history::NavPlace::browser(restored_place),
        sidebar,
        track_list,
        content_stack,
        window_title,
        &active_content_focus,
        "session restore",
    );
    nav_history.end_back();
    super::session_restore::wire_close(
        window,
        conn,
        track_list,
        player.as_ref(),
        session_state,
        geometry_guard,
        nav_history,
    );
    super::session_restore::arm_seed_close(window);
    let present_rhythmbox_import = {
        let preferences = Rc::downgrade(preferences);
        Rc::new(move || {
            if let Some(preferences) = preferences.upgrade() {
                preferences.present_rhythmbox_import_dialog();
            }
        }) as Rc<dyn Fn()>
    };
    super::first_run::run(
        window,
        scan_button,
        scan_controls,
        conn,
        first_run_decision,
        &present_rhythmbox_import,
    );
    active_content_focus.focus_later_if_unset(window);
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

/// Wires the external-changes live refresh (multi-frontend-core package C):
/// mutations written to the same database by another process (CLI/MCP) reach
/// the running app through the change log and `events::Notifier`. The app's own
/// writes are filtered by its process writer token — it already refreshes
/// itself — so only foreign writes drive a coarse, silent refresh of the
/// sidebar and the current track list (UX rules EXT-1a..EXT-4). A notifier that
/// cannot start just means no live updates; it is never fatal.
fn start_external_changes_refresh(
    db_path: &Path,
    track_list: &Rc<TrackList>,
    sidebar: &Rc<Sidebar>,
) {
    let sidebar = Rc::downgrade(sidebar);
    let track_list = Rc::downgrade(track_list);
    crate::ui::external_changes::start(
        db_path,
        Some(reprise_core::events::writer_token()),
        Rc::new(move |plan: crate::ui::external_changes::RefreshPlan| {
            if plan.sidebar {
                match sidebar.upgrade() {
                    Some(sidebar) => sidebar.refresh("external change"),
                    None => {
                        tracing::warn!("external change: sidebar refresh skipped: sidebar is gone");
                    }
                }
            }
            if plan.track_list {
                match track_list.upgrade() {
                    Some(track_list) => track_list.reload(),
                    None => {
                        tracing::warn!(
                            "external change: track list reload skipped: track list is gone"
                        );
                    }
                }
            }
            if plan.conversion {
                // An MCP/CLI process enqueued an instrumental job. The app-hosted
                // worker idles until woken, so nudge it to claim and render the
                // new job. A no-op when the experimental feature is off (no
                // worker, no wake hook).
                crate::ui::instrumental::wake_worker();
            }
        }),
    );
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
