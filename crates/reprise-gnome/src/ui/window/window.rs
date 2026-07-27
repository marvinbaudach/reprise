//! Builds the main application window: an `adw::OverlaySplitView` whose left
//! sidebar holds `ui::sidebar::Sidebar` and whose content holds the
//! pre-existing libadwaita `ToolbarView` — a header
//! bar over the full Library layout, compact track statistics at the
//! content's bottom-right corner, the player bar, and an `adw::ToastOverlay`
//! wrapping the track content so scan errors can surface a toast.
//!
//! ## Sidebar toggle
//!
//! `AdwOverlaySplitView` keeps the content mounted while its Start-positioned
//! sidebar collapses into an overlay below 800 px. The persistent header
//! toggle controls `show-sidebar`; responsive collapse never overwrites the
//! user's explicit hidden preference.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use reprise_core::library::settings;
use reprise_core::library::watcher::WatcherHandle;
use reprise_core::playback::{PlaybackError, PlayerEvent};
use reprise_core::view_source::ViewSource;
use reprise_core::waveform::WaveformBackend;
use reprise_platform_linux::player::Player;

use super::cover_download_worker;
use super::file_open::FileOpenHandler;
use super::player_controller::{PlayerController, PlayerControllerBackends};
use super::scan_progress::ScanProgressView;
use super::sidebar::Sidebar;
use super::status_bar::StatusBar;
use super::strings;
use super::track_content;
use super::track_list::{OnActivate, TrackList};

const MIN_WIDTH: i32 = 600;
const MIN_HEIGHT: i32 = 400;

fn build_player_backends(
    waveform: Arc<dyn WaveformBackend>,
    media: reprise_core::media_integration::MediaIntegrationHandles,
) -> Result<PlayerControllerBackends, PlaybackError> {
    let (sender, playback_events) = async_channel::unbounded::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        if let Err(error) = sender.try_send(event) {
            tracing::warn!(%error, "player event dropped: UI receiver is gone");
        }
    }))?;

    Ok(PlayerControllerBackends {
        playback: Box::new(player),
        playback_events,
        media,
        waveform,
    })
}

/// Builds and presents the main window for `app`. `conn` is the shared,
/// already-migrated database connection; the UI layer owns it single-threaded
/// (via `Rc<RefCell<_>>`) and reads through it via `track_list::TrackList`.
/// `db_path` is the same connection's on-disk path; every call site inside
/// this function only ever needs to *clone* it into an owned `PathBuf` for a
/// scan-worker thread or the watcher (both open their own `Connection` over
/// it rather than sharing this one across threads — `rusqlite::Connection`
/// isn't `Send`), so this takes a borrow rather than owning it outright.
pub fn build(
    app: &adw::Application,
    conn: &Rc<RefCell<Connection>>,
    db_path: &Path,
) -> FileOpenHandler {
    super::style::install();
    {
        let conn = conn.borrow();
        let stored = reprise_core::library::settings::get_setting(
            &conn,
            super::style::theme::THEME_SETTING_KEY,
        )
        .ok()
        .flatten();
        let theme = stored
            .as_deref()
            .and_then(super::style::theme::Theme::from_id)
            .unwrap_or(super::style::theme::Theme::DEFAULT);
        super::style::set_theme(theme);
        let scheme = reprise_core::library::settings::get_color_scheme(&conn);
        super::style::set_color_scheme(scheme);
    }
    let session_state = {
        let conn = conn.borrow();
        super::session_restore::load(&conn)
    };
    let first_run_decision = super::first_run::initial_decision(&conn.borrow());
    let initial_view =
        super::compact_mode_controls::initial_transition(&conn.borrow(), first_run_decision);
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(strings::text(strings::APP_NAME))
        .default_width(session_state.window_width)
        .default_height(session_state.window_height)
        .width_request(MIN_WIDTH)
        .height_request(MIN_HEIGHT)
        .build();
    let waveform_backend: Arc<dyn WaveformBackend> =
        Arc::new(reprise_platform_linux::waveform::GstreamerWaveformBackend);
    super::focus_evidence::install(&window);
    super::session_restore::apply_initial_geometry(&window, &session_state);
    // Headerbar title follows the currently selected `ViewSource` (Stage 3
    // Task 4); `Library` (`ViewSource::default()`) is both `TrackList`'s and
    // `Sidebar`'s own default initial source, so this is set directly here
    // rather than through a round trip via `Sidebar::set_on_select` (not
    // wired until after `TrackList` exists — see that method's doc comment).
    let window_title = adw::WindowTitle::new(&strings::text(strings::SIDEBAR_MUSIC), "");

    let search_entry = gtk4::SearchEntry::builder()
        .placeholder_text(strings::text(strings::SEARCH_PLACEHOLDER))
        .accessible_role(gtk4::AccessibleRole::SearchBox)
        .build();
    search_entry.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::SEARCH_PLACEHOLDER,
    ))]);

    // Starts hidden until `wire_sidebar_toggle` has applied both the persisted
    // Sidebar preference and the current split-view state.
    let sidebar_toggle = gtk4::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text(strings::text(strings::SIDEBAR_TOGGLE))
        .css_classes(["flat", "reprise-panel-toggle"])
        .visible(false)
        .build();

    let header = adw::HeaderBar::new();
    header.pack_start(&sidebar_toggle);
    header.pack_start(&super::library_chrome::build_navigation_back_button());
    header.set_title_widget(Some(&window_title));
    let maintenance_actions = super::library_chrome::build_maintenance_actions();
    let scan_button = maintenance_actions.scan;

    // The player is created eagerly at window build (not lazily on first
    // activation): construction is cheap (one playbin, no I/O), the
    // `REPRISE_AUDIO_SINK` override keeps headless environments working, and
    // eager creation means the bottom bar exists — greyed out — from the
    // first frame. If GStreamer is unavailable the app degrades to a library
    // browser: error logged, no player bar, activations warn (fault
    // tolerance: never crash over a missing subsystem).
    let cover_download = cover_download_worker::setup(&conn.borrow());
    let listenbrainz = super::scrobble_runtime::ScrobbleRuntime::new(
        db_path.to_path_buf(),
        reprise_core::scrobbling::ScrobbleProvider::ListenBrainz,
        "ListenBrainz",
    );
    let lastfm = super::scrobble_runtime::ScrobbleRuntime::new(
        db_path.to_path_buf(),
        reprise_core::scrobbling::ScrobbleProvider::LastFm,
        "Last.fm",
    );
    super::preference_lastfm::bootstrap(conn, &lastfm);
    super::preference_listenbrainz::bootstrap(conn, &listenbrainz);
    super::window_smoke::arm_listenbrainz(conn, &listenbrainz);
    super::window_smoke::arm_lastfm(conn, &lastfm);
    let artist_news = super::artist_news_worker::ArtistNewsRuntime::setup(&conn.borrow());
    let concerts_runtime = crate::ui::concerts::ConcertsRuntime::setup(&conn.borrow());
    let podcasts_runtime = crate::ui::podcasts::PodcastsRuntime::setup(&conn.borrow());
    let artist_portrait =
        super::artist_portrait_worker::ArtistPortraitRuntime::setup(&conn.borrow());
    let media = std::env::var(crate::SMOKE_MPRIS_BUS_ENV_VAR).map_or_else(
        |_| reprise_platform_linux::mpris::start(crate::APP_ID),
        |bus_name| reprise_platform_linux::mpris::start_with_bus_name(crate::APP_ID, bus_name),
    );
    let device_sync = super::device_sync_smoke::runtime_from_env(conn).unwrap_or_else(|| {
        super::device_sync_runtime::DeviceSyncRuntime::new(
            conn,
            reprise_platform_linux::device_sync::DeviceMonitor::new(),
        )
    });
    device_sync
        .bind_agent_device_sync(&media.device_sync_state, media.device_sync_commands.clone());
    super::device_sync_smoke::arm(&device_sync);

    let player = match build_player_backends(waveform_backend.clone(), media) {
        Ok(backends) => Some(PlayerController::new(
            conn.clone(),
            cover_download.clone(),
            listenbrainz.clone(),
            lastfm.clone(),
            backends,
            app,
        )),
        Err(error) => {
            tracing::error!(%error, "player unavailable: playback disabled");
            None
        }
    };
    let startup_purged = match super::issues::purge_startup_tombstones(conn) {
        Ok(ids) => ids,
        Err(error) => {
            tracing::error!(%error, "startup tombstone purge failed");
            Vec::new()
        }
    };
    if let Some(player) = &player {
        player.purge_queue_ids(&startup_purged);
    }
    let queue_model = super::window_queue_model::build(&player);

    // Built right after `player` (needed for the Queue row's counter) and
    // before `TrackList` and `spawn_scan`/`player.set_track_list_reload`
    // (both hold an `Rc<Sidebar>`/`Weak<Sidebar>` to call `refresh` from
    // their own specific triggers — see `Sidebar::refresh`'s doc comment for
    // the trigger inventory), so every later site can just clone/downgrade
    // this one `Rc` rather than needing a construction-order-driven `Weak`-
    // then-upgrade dance.
    let sidebar = Rc::new(Sidebar::new(conn.clone(), &window, {
        let queue_model = queue_model.clone();
        move || queue_model.borrow().sidebar_count()
    }));
    sidebar.bind_device_sync(&device_sync);

    // Stage 3 Task 8: at most one folder watcher runs at a time. `None`
    // until either the startup check below finds a persisted `library_root`
    // or a scan (button click, "Rescan library" menu action, or the
    // `REPRISE_SMOKE_RESCAN` hook) completes and (re)arms it on the freshly
    // scanned folder — see `start_or_restart_watcher`. Replacing the stored
    // `Some(handle)` drops the previous one first (assignment order), which
    // stops its background thread and unregisters its OS-level watch before
    // the new one is armed.
    let watcher_state: Rc<RefCell<Option<WatcherHandle>>> = Rc::new(RefCell::new(None));

    let on_activate: OnActivate = {
        let player = player.clone();
        let conn = conn.clone();
        Box::new(move |track, ids, start_index, place| match &player {
            Some(player) => {
                let origin = {
                    let conn = conn.borrow();
                    crate::ui::playback::play_origin::resolve(&conn, &place)
                };
                player.play_from_view(ids, start_index, origin);
            }
            None => {
                tracing::warn!(path = %track.path, "player unavailable; ignoring activation");
            }
        })
    };

    let status_bar = StatusBar::new();

    // Stage 3 Task 3: the Queue source reads the current playback queue's
    // ids (in play order) from the controller rather than a SQL `WHERE`
    // clause (see `queries.rs`'s module doc). `player` already exists at
    // this point (built above), so this can be a plain constructor
    // parameter — unlike `toast_overlay`, which needs post-construction
    // injection because it's built *after* `track_list` (see that field's
    // doc comment in `player_controller.rs`). `None` (GStreamer unavailable)
    // degrades to an always-empty queue view, matching every other
    // player-unavailable degradation in this function.
    let queue_ids_provider = {
        let queue_model = queue_model.clone();
        move || queue_model.borrow().clone()
    };

    let track_list = {
        let status_bar = status_bar.clone();
        let conn_for_status = conn.clone();
        let player_for_reload = player.clone();
        // This `on_reload` hook fires on *every* reload — initial load,
        // search-filter debounce, sort-header click, and plain source
        // switch, besides the scan-completion one — so it is limited to two
        // cheap reads: the status line and a SELECT EXISTS that keeps idle
        // Play availability current after scans/library mutations. Stage 3
        // Task 4's review (finding #2) caught an earlier version of this
        // closure also calling `sidebar.refresh()` here, which meant a full
        // `ListBox` teardown/rebuild plus five DB queries on every debounced
        // keystroke and every column-sort click. The sidebar now refreshes
        // only from its own specific triggers — see `Sidebar::refresh`'s doc
        // comment for the trigger inventory, and `spawn_scan`'s success arm /
        // the `player.set_track_list_reload` closure just below for two of
        // the three call sites.
        Rc::new(TrackList::new(
            conn.clone(),
            on_activate,
            move |source, _count, _filter, _browse| {
                if let Some(player) = &player_for_reload {
                    player.refresh_library_availability();
                }
                if matches!(source, ViewSource::Library) {
                    status_bar.refresh(&conn_for_status);
                } else {
                    status_bar.hide();
                }
            },
            queue_ids_provider,
            cover_download.clone(),
        ))
    };
    super::column_layout_editor::install_header_popover(&track_list);
    if let Some(player) = &player {
        let sidebar = Rc::downgrade(&sidebar);
        let track_list_weak = Rc::downgrade(&track_list);
        player.add_on_queue_changed(move || {
            if let Some(sidebar) = sidebar.upgrade() {
                sidebar.refresh("up next changed");
            }
            if let Some(track_list) = track_list_weak.upgrade() {
                track_list.reload_queue_if_visible();
            }
        });
        let track_list_weak = Rc::downgrade(&track_list);
        player.set_view_refill_provider(move || match track_list_weak.upgrade() {
            Some(track_list) => track_list.transport_refill_ids(),
            None => Vec::new(),
        });
    }
    let scan_progress = ScanProgressView::new();
    let scan_controls = super::scan_flow::ScanControls::new(&scan_button, &scan_progress);
    scan_controls.set_sidebar_toggle(&sidebar_toggle);
    scan_progress.set_on_cancel({
        let scan_controls = scan_controls.clone();
        move || scan_controls.request_cancel()
    });
    sidebar.append_scan_card(scan_progress.widget());
    sidebar.append_relink_card(track_list.missing_relink_progress_widget());
    let toolbar_view = adw::ToolbarView::new();
    // No add_top_bar for scan progress — it lives in the sidebar now.
    let track_content = track_content::build(track_list.widget(), status_bar.widget());
    // NAV-2: one history for every scoped route through the canonical list.
    let nav_history = Rc::new(crate::ui::nav_history::NavHistory::default());
    super::current_track_selection::wire(player.as_ref(), &track_list);
    let stats_view = super::stats_view::StatsView::new(track_list.shared_cover_loader());
    stats_view.wire_year_selector(conn);
    let content_stack = super::content_stack::build();
    // Size to the visible page in both axes: dedicated content pages must not
    // inherit the library's minimum size, nor vice versa.
    content_stack.add_named(&track_content, Some("library"));
    content_stack.add_named(stats_view.widget(), Some("stats"));
    content_stack.set_visible_child_name("library");
    toolbar_view.set_content(Some(&content_stack));

    let active_content_focus =
        super::library_shell::ActiveContentFocus::new(&content_stack, &track_list);
    let metadata_navigator = super::metadata_navigation::MetadataNavigator::new(
        nav_history.clone(),
        &sidebar,
        &track_list,
        content_stack.clone(),
        window_title.clone(),
        active_content_focus.clone(),
    );
    let on_show_album: crate::ui::updates::release_row::OnShowAlbum = {
        let navigator = metadata_navigator.clone();
        Rc::new(move |album: &str, artist: &str| {
            navigator.navigate(
                reprise_core::browser::navigation::NavigationIntent::OpenAlbum {
                    album: reprise_core::browser::AlbumKey::new(album, artist),
                    anchor_track_id: None,
                },
                "new releases",
            );
        })
    };
    let on_open_updates_view: crate::ui::updates::popover::OnOpenView = {
        let navigator = metadata_navigator.clone();
        Rc::new(move |target| {
            navigator.navigate(
                reprise_core::browser::navigation::NavigationIntent::Sidebar(target),
                "updates jump",
            );
        })
    };
    let concerts_view = Rc::new(crate::ui::concerts::install(
        conn.clone(),
        &concerts_runtime,
    ));
    let releases_view = Rc::new(crate::ui::releases::install(
        conn.clone(),
        db_path.to_path_buf(),
        on_show_album.clone(),
    ));
    content_stack.add_named(concerts_view.root(), Some("concerts"));
    content_stack.add_named(releases_view.root(), Some("releases"));
    let source_views = super::source_views::install(
        conn,
        &podcasts_runtime,
        player.as_ref(),
        &sidebar,
        &content_stack,
    );
    let podcasts_view = source_views.podcasts;
    let radio_view = source_views.radio;
    super::source_views::wire_update_sidebar_refresh(&concerts_view, &releases_view, &sidebar);

    let bar_position = settings::get_player_bar_position(&conn.borrow());

    // The toast layer is attached after the player-bar shell exists so
    // notifications render above the complete library chrome.
    let toast_overlay = adw::ToastOverlay::new();
    podcasts_view.set_toast_overlay(&toast_overlay);
    radio_view.set_toast_overlay(&toast_overlay);
    {
        let overlay = toast_overlay.downgrade();
        concerts_view.set_on_launch_error(move |error| {
            if let Some(overlay) = overlay.upgrade() {
                crate::ui::toasts::show(&overlay, &error);
            }
        });
    }
    {
        let overlay = toast_overlay.downgrade();
        releases_view.set_on_launch_error(move |error| {
            if let Some(overlay) = overlay.upgrade() {
                crate::ui::toasts::show(&overlay, &error);
            }
        });
    }

    super::window_action_wiring::wire(super::window_action_wiring::ActionWiring {
        conn,
        db_path,
        window: &window,
        toast_overlay: &toast_overlay,
        track_list: &track_list,
        sidebar: &sidebar,
        player: &player,
        stats_view: &stats_view,
        content_stack: &content_stack,
        scan_controls: &scan_controls,
        watcher_state: &watcher_state,
        metadata_navigator: &metadata_navigator,
    });

    let library_shell = super::library_shell::build(
        &window,
        conn,
        &sidebar,
        &toolbar_view,
        &track_list,
        player.as_ref(),
        &artist_news,
    );
    let sidebar_page = library_shell.sidebar_page;
    let split_view = library_shell.split_view;
    let content_nav = library_shell.content_nav;
    let info_panel = library_shell.info_panel;
    super::device_sync_feedback::install(&header, &split_view, &toast_overlay, &device_sync);
    info_panel.retain_for_window(&window);
    if let Some(player) = &player {
        super::window_now_playing_wiring::install(player, &info_panel, &queue_model);
    }
    let player_bar_widget = player
        .as_ref()
        .map(|player| player.bar_widget().upcast_ref::<gtk4::Widget>());
    header.pack_end(&info_panel.toggle_button());
    let library_player_bar = super::library_player_bar::LibraryPlayerBarShell::new(
        &split_view,
        player_bar_widget,
        bar_position,
    );
    toast_overlay.set_child(Some(library_player_bar.widget()));
    let library_chrome =
        super::library_chrome::build(&header, &toast_overlay, &search_entry, &window);
    crate::ui::updates::popover::install(
        &header,
        &window,
        conn,
        db_path,
        &artist_news,
        &concerts_runtime,
        crate::ui::updates::popover::UpdatesCallbacks {
            on_show_album,
            on_open_view: on_open_updates_view,
        },
    );
    let compact_root = player
        .as_ref()
        .map(|player| player.compact_player.handle().upcast_ref());
    let decorations =
        super::window_decorations::WindowDecorations::new(&window, &header, compact_root);
    let content_host = decorations.content_host();
    let minimal_view = super::compact_mode_controls::build_mode(
        &window,
        &content_host,
        library_chrome.root.upcast_ref(),
        player.as_ref().map(|player| &player.compact_player),
        conn,
        initial_view,
        &toast_overlay,
    );
    {
        let minimal_view = Rc::downgrade(&minimal_view);
        decorations.set_on_mode_changed(Rc::new(move || {
            if let Some(minimal_view) = minimal_view.upgrade() {
                minimal_view.refresh_geometry();
            }
        }));
    }
    let geometry_guard = minimal_view.geometry_guard();
    let cover_batch = super::cover_download_batch::CoverDownloadBatch::new(
        conn,
        &cover_download,
        &track_list,
        player.as_ref(),
    );
    super::main_cover_download_progress::install(&scan_controls, &cover_batch);
    let preferences = super::preferences::PreferencesContext::new(
        &window,
        conn,
        &track_list,
        &sidebar,
        &split_view,
        &sidebar_page,
        &status_bar,
        &library_player_bar,
        &info_panel,
        &scan_button,
        &scan_controls,
        player.as_ref(),
        &listenbrainz,
        &lastfm,
        &artist_news,
        &concerts_runtime,
        &podcasts_runtime,
        &cover_download,
        &artist_portrait,
        &decorations,
    );
    {
        let preferences = Rc::downgrade(&preferences);
        info_panel.lyrics_view().set_on_settings(move || {
            if let Some(preferences) = preferences.upgrade() {
                preferences.present_plugins(crate::ui::preference_plugins::ONLINE_LYRICS_TARGETS);
            }
        });
    }
    super::window_runtime_wiring::wire(super::window_runtime_wiring::RuntimeWiring {
        app,
        window: &window,
        conn,
        db_path,
        header: &header,
        search_entry: &search_entry,
        search_bar: &library_chrome.search_bar,
        sidebar_toggle: &sidebar_toggle,
        sidebar_page: &sidebar_page,
        split_view: &split_view,
        track_list: &track_list,
        sidebar: &sidebar,
        player: &player,
        stats_view,
        concerts_view: &concerts_view,
        releases_view: &releases_view,
        podcasts_view: &podcasts_view,
        radio_view: &radio_view,
        podcasts_runtime: &podcasts_runtime,
        content_stack: &content_stack,
        device_sync: &device_sync,
        window_title: &window_title,
        scan_controls: &scan_controls,
        toast_overlay: &toast_overlay,
        watcher_state: &watcher_state,
        library_player_bar: &library_player_bar,
        info_panel: &info_panel,
        session_state: &session_state,
        geometry_guard: &geometry_guard,
        scan_button: &scan_button,
        minimal_view: &minimal_view,
        preferences: &preferences,
        cover_batch: &cover_batch,
        first_run_decision,
        nav_history: &nav_history,
        content_nav: &content_nav,
        active_content_focus: &active_content_focus,
        metadata_navigator: &metadata_navigator,
    });

    tracing::info!("main window built");
    window.present();
    super::runtime_performance::arm(&window, &track_list);
    FileOpenHandler::new(&window, conn.clone(), player, &toast_overlay, sidebar)
}
