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
use reprise_platform_linux::waveform::GstreamerWaveformBackend;

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
        media: reprise_platform_linux::mpris::start(crate::APP_ID),
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
    let waveform_backend: Arc<dyn WaveformBackend> = Arc::new(GstreamerWaveformBackend);
    super::scan_flow::spawn_waveform_backfill(db_path.to_path_buf(), waveform_backend.clone());
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
    let artist_portrait =
        super::artist_portrait_worker::ArtistPortraitRuntime::setup(&conn.borrow());
    let device_sync = super::device_sync_smoke::runtime_from_env(conn).unwrap_or_else(|| {
        super::device_sync_runtime::DeviceSyncRuntime::new(
            conn,
            reprise_platform_linux::device_sync::DeviceMonitor::new(),
        )
    });
    super::device_sync_actions::install(app, &device_sync);
    super::device_sync_smoke::arm(&device_sync);

    let player = match build_player_backends(waveform_backend.clone()) {
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

    // Built right after `player` (needed for the Queue row's counter) and
    // before `TrackList` and `spawn_scan`/`player.set_track_list_reload`
    // (both hold an `Rc<Sidebar>`/`Weak<Sidebar>` to call `refresh` from
    // their own specific triggers — see `Sidebar::refresh`'s doc comment for
    // the trigger inventory), so every later site can just clone/downgrade
    // this one `Rc` rather than needing a construction-order-driven `Weak`-
    // then-upgrade dance.
    let sidebar = Rc::new(Sidebar::new(conn.clone(), &window, {
        let player = player.clone();
        move || match &player {
            Some(controller) => controller.queue_pending_len(),
            None => 0,
        }
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
        Box::new(move |track, ids, start_index, source| match &player {
            Some(player) => {
                let origin = {
                    let conn = conn.borrow();
                    crate::ui::playback::play_origin::resolve(&conn, &source)
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
    let queue_model = super::window_queue_model::build(&player);
    let queue_ids_provider = {
        let queue_model = queue_model.clone();
        move || queue_model.borrow().clone()
    };

    let track_list = {
        let status_bar = status_bar.clone();
        let conn_for_status = conn.clone();
        // This `on_reload` hook fires on *every* reload — initial load,
        // search-filter debounce, sort-header click, and plain source
        // switch, besides the scan-completion one — so it's kept to the one
        // thing that's cheap and correct at that frequency: the status
        // line. Stage 3 Task 4's review (finding #2) caught an earlier
        // version of this closure also calling `sidebar.refresh()` here,
        // which meant a full `ListBox` teardown/rebuild plus five DB queries
        // on every debounced keystroke and every column-sort click. The
        // sidebar now refreshes only from its own specific triggers — see
        // `Sidebar::refresh`'s doc comment for the trigger inventory, and
        // `spawn_scan`'s success arm / the `player.set_track_list_reload`
        // closure just below for two of the three call sites.
        Rc::new(TrackList::new(
            conn.clone(),
            on_activate,
            move |source, _count, _filter, _browse| {
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
    let scan_controls =
        super::scan_flow::ScanControls::new(&scan_button, &scan_progress, waveform_backend);
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
    let album_view = super::album_view::AlbumView::new(conn, track_list.shared_cover_loader());
    // Retain the assembled `ArtistView` past `build()`: its `refresh_callback`
    // and now-playing mini-EQ both hang off a pure-Rust `Rc<Inner>`, so the
    // view must stay alive. The strong clone captured by the track-change
    // wiring below (see `current_track_selection::wire`) is what keeps it so.
    let artist_view = Rc::new(super::artist_view::ArtistView::new(
        conn.clone(),
        track_list.shared_cover_loader(),
        artist_portrait.clone(),
    ));
    let library_views = super::library_shell::build_views(
        &track_content,
        album_view.widget(),
        artist_view.widget(),
    );
    // NAV-2: shared navigation history — created before the view wiring so
    // album/artist cross-navigation can record the places it leaves.
    let nav_history = Rc::new(crate::ui::nav_history::NavHistory::default());
    super::library_shell::wire_album_view(&library_views, &album_view, &track_list, &nav_history);
    if let Some(player) = &player {
        let grid = album_view.grid_widget().downgrade();
        player.set_on_playback_state_changed_album(move |state| {
            let Some(grid) = grid.upgrade() else { return };
            match state {
                reprise_core::playback::PlaybackState::Paused => {
                    grid.add_css_class("playback-paused");
                }
                _ => {
                    grid.remove_css_class("playback-paused");
                }
            }
        });
    }
    super::library_shell::wire_artist_view(&library_views, &artist_view, &track_list, &nav_history);
    // Wire playback → track-table selection and Artists-view now-playing. Done
    // here (not right after `track_list` is built) because the closure captures
    // a strong `Rc<ArtistView>`, which must exist first.
    super::current_track_selection::wire(player.as_ref(), &track_list, &artist_view);
    super::library_shell::arm_smoke_library_view(&library_views);
    let library_title = Rc::new(super::library_chrome::build_library_title(
        &header,
        &window_title,
        &library_views.stack,
    ));
    let stats_view = super::stats_view::StatsView::new(track_list.shared_cover_loader());
    stats_view.wire_year_selector(conn);
    let device_view = super::device_view::DeviceViewPage::new(&device_sync);
    let new_releases_digest = crate::ui::new_releases::digest::NewReleasesDigest::new(conn.clone());
    let content_stack = gtk4::Stack::new();
    // Size to the visible page (see the library stack's `set_hhomogeneous`):
    // Stats/Device pages must not inherit the library's minimum width, nor vice
    // versa, or the whole content is forced past the window edge (QA #3/#4).
    content_stack.set_hhomogeneous(false);
    content_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    content_stack.set_transition_duration(crate::ui::motion::STANDARD_MS);
    content_stack.add_named(&library_views.stack, Some("library"));
    content_stack.add_named(stats_view.widget(), Some("stats"));
    content_stack.add_named(device_view.widget(), Some("device"));
    content_stack.add_named(new_releases_digest.widget(), Some("new-releases"));
    content_stack.set_visible_child_name("library");
    toolbar_view.set_content(Some(&content_stack));

    let bar_position = settings::get_player_bar_position(&conn.borrow());

    // The toast layer is attached after the player-bar shell exists so
    // notifications render above the complete library chrome.
    let toast_overlay = adw::ToastOverlay::new();

    super::window_action_wiring::wire(super::window_action_wiring::ActionWiring {
        conn,
        db_path,
        window: &window,
        toast_overlay: &toast_overlay,
        track_list: &track_list,
        sidebar: &sidebar,
        album_view: &album_view,
        artist_view: &artist_view,
        player: &player,
        content_stack: &content_stack,
        library_stack: &library_views.stack,
        scan_controls: &scan_controls,
        watcher_state: &watcher_state,
    });

    let library_shell = super::library_shell::build(
        &window,
        conn,
        &sidebar,
        &toolbar_view,
        &track_list,
        player.as_ref(),
        &artist_news,
        &artist_portrait,
    );
    let sidebar_page = library_shell.sidebar_page;
    let split_view = library_shell.split_view;
    let _content_nav = library_shell.content_nav;
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
    let open_new_releases = {
        let digest = new_releases_digest.clone();
        let nav_history = nav_history.clone();
        let content_stack = content_stack.clone();
        Rc::new(move || {
            let Some(_place) = nav_history.record_new_releases() else {
                tracing::warn!("cannot open New Releases before navigation is initialized");
                return;
            };
            digest.refresh();
            content_stack.set_visible_child_name("new-releases");
        })
    };
    crate::ui::new_releases::popover::install(
        &header,
        &window,
        conn,
        db_path,
        open_new_releases,
        &artist_news,
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
        &cover_download,
        &artist_portrait,
        &decorations,
        &device_sync,
    );
    {
        let preferences = preferences.clone();
        device_view.set_on_settings(move || preferences.present_page("synchronization"));
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
        content_stack: &content_stack,
        device_view: &device_view,
        library_views: &library_views,
        library_title: &library_title,
        window_title: &window_title,
        album_view: &album_view,
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
    });

    tracing::info!("main window built");
    window.present();
    FileOpenHandler::new(&window, conn.clone(), player, &toast_overlay, sidebar)
}
