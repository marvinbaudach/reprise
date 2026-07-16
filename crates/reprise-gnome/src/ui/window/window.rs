//! Builds the main application window: an `adw::NavigationSplitView` (Stage
//! 3 Task 4) whose sidebar page holds `ui::sidebar::Sidebar` and whose
//! content page holds the pre-existing libadwaita `ToolbarView` — a header
//! bar over the full Library layout, compact track statistics at the
//! content's bottom-right corner, the player bar, and an `adw::ToastOverlay`
//! wrapping the track content so scan errors can surface a toast.
//!
//! ## Sidebar toggle
//!
//! `AdwNavigationSplitView` collapses into a push/pop navigation stack at
//! narrow widths on its own (Adwaita default behavior — this module doesn't
//! fight it: `adw::HeaderBar` embedded in a page inside that stack shows its
//! own back button automatically). The headerbar also gets an explicit,
//! persistent `sidebar-show-symbolic` toggle button. At wide widths it folds
//! the complete sidebar column away so the track table receives that space;
//! at narrow widths it switches between the native sidebar and content pages.
//! `NavigationSplitView` has no `show-sidebar` property (that's
//! `AdwOverlaySplitView`'s API), so `wire_sidebar_toggle` coordinates its
//! `collapsed` and `show-content` properties.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use reprise_core::library::settings;
use reprise_core::library::watcher::WatcherHandle;
use reprise_core::view_source::ViewSource;

use super::cover_download_worker;
use super::file_open::FileOpenHandler;
use super::player_controller::PlayerController;
use super::playlist_io;
use super::primary_menu;
use super::scan_progress::ScanProgressView;
use super::shortcuts;
use super::sidebar::Sidebar;
use super::status_bar::StatusBar;
use super::strings;
use super::track_content;
use super::track_list::{OnActivate, TrackList};

const MIN_WIDTH: i32 = 600;
const MIN_HEIGHT: i32 = 400;

/// Environment variable that, when set to any value, arms a one-shot timer
/// that closes the window (and thereby quits the app, since it's the only
/// window) a few seconds after it is shown. This is a standing, permanent
/// headless-verification hook — not a temporary hack — used to confirm in CI
/// or over `xvfb-run` that the app starts, builds its window, and exits
/// cleanly without a human present or a real display driving interaction.
///
/// Usage: `REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.
const SMOKE_QUIT_ENV_VAR: &str = "REPRISE_SMOKE_QUIT";
const SMOKE_QUIT_DELAY_SECS_DEFAULT: u32 = 3;
/// Overrides `SMOKE_QUIT_DELAY_SECS_DEFAULT` — added for Stage 2 Task 4's
/// queue E2E, which needs to observe several auto-advances (each a fixture
/// track's full playback) before the window closes; the 3-second default
/// (sized for the plain startup/shutdown smoke test) is too short for that.
/// Every other `REPRISE_SMOKE_QUIT=1` caller is unaffected — unset, this
/// keeps the original 3-second delay.
///
/// Usage: `REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=8 xvfb-run -a cargo run`.
const SMOKE_QUIT_DELAY_SECS_ENV_VAR: &str = "REPRISE_SMOKE_QUIT_DELAY_SECS";

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
    super::scan_flow::spawn_waveform_backfill(db_path.to_path_buf());
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
        .build();

    // Starts hidden until `wire_sidebar_toggle` has applied both the persisted
    // Sidebar preference and the current split-view state.
    let sidebar_toggle = gtk4::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text(strings::text(strings::SIDEBAR_TOGGLE))
        .css_classes(["flat", "reprise-panel-toggle"])
        .visible(false)
        .build();

    let header = adw::HeaderBar::new();
    super::library_chrome::style_header(&header, &search_entry);
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
    let cover_download = cover_download_worker::setup();
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
    let device_sync = super::device_sync_smoke::runtime_from_env(conn).unwrap_or_else(|| {
        super::device_sync_runtime::DeviceSyncRuntime::new(
            conn,
            reprise_platform_linux::device_sync::DeviceMonitor::new(),
        )
    });
    super::device_sync_smoke::arm(&device_sync);

    let player = match PlayerController::new(
        conn.clone(),
        cover_download.clone(),
        listenbrainz.clone(),
        lastfm.clone(),
        app,
    ) {
        Ok(controller) => Some(controller),
        Err(error) => {
            tracing::error!(%error, "player unavailable: playback disabled");
            None
        }
    };

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
            Some(controller) => controller.up_next_len(),
            None => 0,
        }
    }));

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
        Box::new(move |track, ids, start_index| match &player {
            Some(player) => player.play_from_view(ids, start_index),
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
        let player = player.clone();
        move || match &player {
            Some(controller) => controller.queue_ids_snapshot(),
            None => Vec::new(),
        }
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
            move |source, count, filter, browse| {
                if matches!(source, ViewSource::Library) {
                    status_bar.refresh(&conn_for_status, filter, browse);
                } else {
                    status_bar.refresh_for_source_count(count as i64);
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
        player.set_on_queue_changed(move || {
            if let Some(sidebar) = sidebar.upgrade() {
                sidebar.refresh("up next changed");
            }
            if let Some(track_list) = track_list_weak.upgrade() {
                track_list.reload_queue_if_visible();
            }
        });
    }
    let scan_progress = ScanProgressView::new();
    let scan_controls = super::scan_flow::ScanControls::new(&scan_button, &scan_progress);
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(scan_progress.widget());
    let track_content = track_content::build(track_list.widget(), status_bar.widget());
    let album_view =
        super::album_view::AlbumView::new(conn.clone(), track_list.shared_cover_loader());
    // Retain the assembled `ArtistView` past `build()`: its `refresh_callback`
    // and now-playing mini-EQ both hang off a pure-Rust `Rc<Inner>`, so the
    // view must stay alive. The strong clone captured by the track-change
    // wiring below (see `current_track_selection::wire`) is what keeps it so.
    let artist_view = Rc::new(super::artist_view::ArtistView::new(
        conn.clone(),
        track_list.shared_cover_loader(),
    ));
    let library_views = super::library_shell::build_views(
        &track_content,
        album_view.widget(),
        artist_view.widget(),
    );
    super::library_shell::wire_album_view(&library_views, &album_view, &track_list);
    super::library_shell::wire_artist_view(&library_views, &artist_view, &track_list);
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
    let content_stack = gtk4::Stack::new();
    content_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    content_stack.set_transition_duration(150);
    content_stack.add_named(&library_views.stack, Some("library"));
    content_stack.add_named(stats_view.widget(), Some("stats"));
    content_stack.set_visible_child_name("library");
    toolbar_view.set_content(Some(&content_stack));

    let bar_position = settings::get_player_bar_position(&conn.borrow());

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&toolbar_view));

    // Stage 2 Task 5 fault-tolerance seam: the toast overlay and the track
    // list are both built after the controller (see `PlayerController::
    // new`'s call above and the module doc comment on `set_toast_overlay`/
    // `set_track_list_reload`), so they're injected here instead of being
    // constructor parameters. The reload closure captures `Weak<TrackList>`/
    // `Weak<Sidebar>` — never strong `Rc`s — so the controller can't form an
    // `Rc` cycle with `track_list`'s own strong `Rc<PlayerController>` (held
    // by its `on_activate` closure). This is also sidebar-refresh trigger #3
    // from `Sidebar::refresh`'s doc comment (Stage 3 Task 4 review finding
    // #2c): `PlayerController::reload_track_list` is called from exactly one
    // place — `playback_faults.rs`'s `handle_unplayable_track`, after a
    // successful `mark_track_missing` — so refreshing the sidebar here,
    // alongside the track-list reload, is the specific "Missing badge can
    // have changed" hook rather than a blanket one.
    if let Some(player) = &player {
        player.set_toast_overlay(&toast_overlay);
        let track_list_weak = Rc::downgrade(&track_list);
        let sidebar_weak = Rc::downgrade(&sidebar);
        player.set_track_list_reload(move || {
            match track_list_weak.upgrade() {
                Some(track_list) => track_list.reload(),
                None => tracing::warn!("track list reload skipped: track list is gone"),
            }
            match sidebar_weak.upgrade() {
                Some(sidebar) => sidebar.refresh("track marked missing"),
                None => tracing::warn!("sidebar refresh skipped: sidebar is gone"),
            }
        });
    }
    // Stage 3 Task 1 backlog item (a): same post-construction injection
    // reason as the player's toast overlay above — `track_list` is built
    // before `toast_overlay` exists.
    track_list.set_toast_overlay(&toast_overlay);
    // Same reason again: the sidebar is built before `toast_overlay` exists.
    sidebar.set_toast_overlay(&toast_overlay);
    {
        let player = player.clone();
        sidebar.set_on_missing_removed(move |removed_ids| {
            if let Some(player) = &player {
                player.purge_queue_ids(removed_ids);
            }
        });
    }
    super::tag_edit_flow::wire_refresh(&track_list, &sidebar, &player);

    // Stage 3 Task 5: context menu action wiring. `track_list` stays
    // decoupled from `PlayerController`/`Sidebar` themselves (same
    // decoupling-via-closure seam as `on_activate`/`queue_ids_provider`
    // above) — these three closures are the only place that bridges them.
    // `window` already exists (built at the top of this function), so `set_
    // window` could technically be a constructor parameter, but every other
    // post-construction seam on `track_list` is wired here too, so this
    // keeps all of them in one place.
    track_list.set_window(&window);
    {
        let player = player.clone();
        track_list.set_on_play_selected(move |ids, start_index| match &player {
            Some(player) => player.play_from_view(ids, start_index),
            None => tracing::warn!("player unavailable; ignoring context menu play action"),
        });
    }
    {
        let player = player.clone();
        track_list.set_on_queue_selected(move |ids| match &player {
            Some(player) => player.append_to_queue(&ids),
            None => {
                tracing::warn!("player unavailable; ignoring context menu add-to-queue action");
            }
        });
    }
    {
        let player = player.clone();
        track_list.set_on_queue_activate(move |position| {
            if let Some(player) = &player {
                player.play_up_next_at(position);
            }
        });
    }
    {
        let player = player.clone();
        track_list.set_on_queue_remove(move |positions| {
            player
                .as_ref()
                .map_or(0, |player| player.remove_up_next_positions(positions))
        });
    }
    {
        // Stage 3 Task 6: queue drag-reorder — see `ui::track_list_dnd`'s
        // doc comment. Same decoupling-via-closure seam as `on_play_
        // selected`/`on_queue_selected` just above.
        let player = player.clone();
        track_list.set_on_queue_reorder(move |from, to| match &player {
            Some(player) => player.move_queue_item(from, to),
            None => {
                tracing::warn!("player unavailable; ignoring queue drag-reorder");
                false
            }
        });
    }
    // Task 9a: Artists detail-pane hero playback actions. Player-dependent, so
    // wired here (where `player` + `conn` + `artist_view` are all in scope)
    // rather than in `wire_artist_view`, which handles only the
    // navigation-only setters. Each closure resolves the artist's ordered track
    // ids via `query_track_ids` (album-ordered — a natural "Play all") and hands
    // them to the player.
    {
        // `player` is captured `Weak`: this closure is stored on `ArtistView`,
        // which the controller retains strongly (see
        // `current_track_selection::wire`'s doc comment), so a strong capture
        // here would close the cycle back to the controller.
        let player = player.as_ref().map(Rc::downgrade);
        let conn = conn.clone();
        artist_view.set_on_play_all(move |artist| {
            let Some(player) = player.as_ref().and_then(Weak::upgrade) else {
                return;
            };
            match artist_track_ids(&conn, artist) {
                Ok(ids) if !ids.is_empty() => player.play_from_view(ids, 0),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "artist play-all query failed"),
            }
        });
    }
    {
        // Weak `player` capture — see the `set_on_play_all` comment above.
        let player = player.as_ref().map(Rc::downgrade);
        let conn = conn.clone();
        artist_view.set_on_shuffle(move |artist| {
            let Some(player) = player.as_ref().and_then(Weak::upgrade) else {
                return;
            };
            match artist_track_ids(&conn, artist) {
                Ok(ids) if !ids.is_empty() => player.play_from_view(shuffle_ids(ids), 0),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "artist shuffle query failed"),
            }
        });
    }
    {
        // Weak `player` capture — see the `set_on_play_all` comment above.
        let player = player.as_ref().map(Rc::downgrade);
        let conn = conn.clone();
        artist_view.set_on_add_to_queue(move |artist| {
            let Some(player) = player.as_ref().and_then(Weak::upgrade) else {
                return;
            };
            match artist_track_ids(&conn, artist) {
                Ok(ids) if !ids.is_empty() => player.append_to_queue(&ids),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "artist add-to-queue query failed"),
            }
        });
    }
    {
        let conn = conn.clone();
        artist_view.set_on_go_to_folder(move |artist| open_artist_folder(&conn, &artist));
    }
    if let Some(player) = &player {
        // Task 9b: clicking the player-bar artist name deep-links to the
        // Artists tab and selects the playing album artist (no history/back
        // stack — out of scope). `player` is captured `Weak`: the closure is
        // stored on the bar, itself owned by the controller, so a strong
        // capture would cycle (same reason as `set_track_list_reload` above).
        // The two stacks are cheap GObject clones; `select_artist` is a
        // self-contained callable that holds no strong controller/view
        // reference (see `ArtistMaster::select_callback`).
        let player_weak = Rc::downgrade(player);
        let content_stack = content_stack.clone();
        let library_stack = library_views.stack.clone();
        let select_artist = artist_view.select_artist_callback();
        player.connect_artist_clicked(move || {
            let Some(player) = player_weak.upgrade() else {
                return;
            };
            let Some(artist) = player.current_track_album_artist() else {
                return;
            };
            content_stack.set_visible_child_name("library");
            // Switching to the Artists tab synchronously fires the stack's
            // `visible-child-name` notify handler, which reloads the master
            // (see `library_shell::wire_artist_view`), so the target row
            // exists by the time `select_artist` runs on the next line.
            library_stack.set_visible_child_name(super::library_shell::LIBRARY_VIEW_ARTISTS);
            select_artist(&artist);
        });
    }
    {
        // `Weak`, not a strong `Rc`: mirrors the `sidebar_weak`/`track_list_
        // weak` pattern already used for `player.set_track_list_reload`
        // just above — `track_list` must not keep `sidebar` alive past its
        // natural lifetime.
        let sidebar_weak = Rc::downgrade(&sidebar);
        track_list.set_on_playlist_mutated(move || match sidebar_weak.upgrade() {
            Some(sidebar) => sidebar.refresh("context menu playlist change"),
            None => tracing::warn!(
                "sidebar is gone; skipping refresh after context menu playlist change"
            ),
        });
    }
    {
        // Stage 3 Task 8 / Stage-3 close-out: "Remove from library" (Missing
        // source only) deletes rows outright — the Missing badge count can
        // only ever shrink from that, exactly like the missing-marking
        // trigger above, so the sidebar refresh is wired the same way. The
        // close-out fix adds a second consumer of the same callback: the
        // exact ids `queries::remove_missing_tracks` actually deleted are
        // also purged from the playback queue (`PlayerController::purge_
        // queue_ids`) — a hard-deleted track must not linger as a phantom
        // queue entry (see that method's doc comment for the full
        // invariant). `player.clone()` is a cheap `Option<Rc<_>>` clone,
        // same pattern as every other closure above that needs the
        // controller.
        let sidebar_weak = Rc::downgrade(&sidebar);
        let player = player.clone();
        track_list.set_on_library_mutated(move |removed_ids| {
            match sidebar_weak.upgrade() {
                Some(sidebar) => sidebar.refresh("track removed from library"),
                None => {
                    tracing::warn!("sidebar is gone; skipping refresh after a library removal");
                }
            }
            if let Some(player) = &player {
                player.purge_queue_ids(removed_ids);
            }
        });
    }
    {
        // Stage 3 Task 8: the ImportErrors source's own Retry/Dismiss
        // actions change the Import-errors badge count — a fifth sidebar-
        // refresh trigger alongside scan completion, playlist CRUD,
        // missing-marking, and context-menu playlist mutation (see `Sidebar
        // ::refresh`'s doc comment).
        let sidebar_weak = Rc::downgrade(&sidebar);
        track_list.set_on_import_errors_mutated(move || match sidebar_weak.upgrade() {
            Some(sidebar) => sidebar.refresh("import error mutated"),
            None => {
                tracing::warn!("sidebar is gone; skipping refresh after an import-error mutation");
            }
        });
    }
    {
        // Stage 3 Task 8: "Rescan library" (Missing source context menu)
        // re-runs the persisted library root through the exact same scan
        // flow "Scan folder…" uses — see `trigger_rescan_of_library_root`.
        // `track_list` stays decoupled from the scan machinery/settings
        // table itself, same decoupling-via-closure seam as `on_play_
        // selected`/`on_queue_selected` above.
        let conn = conn.clone();
        let scan_controls = scan_controls.clone();
        let toast_overlay = toast_overlay.clone();
        let db_path = db_path.to_path_buf();
        let track_list_for_rescan = track_list.clone();
        let sidebar_for_rescan = sidebar.clone();
        let watcher_state = watcher_state.clone();
        track_list.set_on_rescan_library(move || {
            super::scan_flow::trigger_rescan_of_library_root(
                &conn,
                &scan_controls,
                &toast_overlay,
                db_path.clone(),
                track_list_for_rescan.clone(),
                sidebar_for_rescan.clone(),
                &watcher_state,
            );
        });
    }

    let library_shell = super::library_shell::build(
        &window,
        conn,
        &sidebar,
        &toast_overlay,
        &track_list,
        player.as_ref(),
        &artist_news,
    );
    let sidebar_page = library_shell.sidebar_page;
    let split_view = library_shell.split_view;
    let _content_nav = library_shell.content_nav;
    let info_panel = library_shell.info_panel;
    info_panel.retain_for_window(&window);
    let player_bar_widget = player
        .as_ref()
        .map(|player| player.bar_widget().upcast_ref::<gtk4::Widget>());
    header.pack_end(&info_panel.toggle_button());
    let library_player_bar = super::library_player_bar::LibraryPlayerBarShell::new(
        &split_view,
        player_bar_widget,
        bar_position,
    );
    let library_chrome = super::library_chrome::build(&header, library_player_bar.widget());
    {
        let info_panel = Rc::downgrade(&info_panel);
        track_list.set_on_selection_changed(move |context| {
            if let Some(info_panel) = info_panel.upgrade() {
                info_panel.set_context(context);
            }
        });
    }
    info_panel.arm_smoke(&track_list);
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
    super::main_cover_download_progress::install(&toolbar_view, &cover_batch, &scan_controls);
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
        &decorations,
        &device_sync,
    );
    let minimal_toggle = minimal_view.clone();
    let compact_preferences = preferences.clone();
    super::compact_mode_controls::install(
        &window,
        &minimal_view,
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
    let stats_sidebar = sidebar.clone();
    primary_menu::install(
        &header,
        &window,
        &track_list,
        primary_menu::Callbacks {
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
            on_sync_device: Rc::new(move || {
                sync_preferences.present_page("synchronization");
            }),
            on_preferences: Rc::new(move || preferences.present()),
        },
    );
    // Task 7: wire the player bar's queue button to open the Queue sidebar.
    if let Some(ref player) = player {
        let sidebar_for_queue = sidebar.clone();
        player.bar.connect_queue_clicked(move || {
            sidebar_for_queue.refresh_and_select(ViewSource::Queue, "player bar queue button");
        });
    }
    // Cover click → toggle info panel (spec 1.5).
    if let Some(ref player) = player {
        let toggle = info_panel.toggle_button().clone();
        player.connect_cover_clicked(move || {
            toggle.set_active(!toggle.is_active());
        });
    }
    // Artist click → jump to the Artists master/detail view and select the
    // now-playing album artist. Wired above (search for `connect_artist_clicked`)
    // where `artist_view` is in scope; the earlier `set_source`-to-Tracks
    // stopgap is superseded by the dedicated Artists view.
    header.pack_end(&search_entry);
    cover_batch.start();
    app.set_accels_for_action("win.toggle-minimal-view", &["<Control>m"]);
    app.set_accels_for_action("win.preferences", &["<Control>comma"]);
    app.set_accels_for_action("win.keyboard-shortcuts", &["<Control>question"]);
    app.set_accels_for_action("win.help", &[super::help::HELP_ACCELERATOR]);
    super::window_navigation::wire_sidebar_toggle(&sidebar_toggle, &split_view, &sidebar_page);
    let show_content_if_collapsed = super::window_navigation::show_content_callback(&split_view);
    super::library_shell::wire_source_routing(
        &sidebar,
        &track_list,
        stats_view,
        conn,
        &content_stack,
        &library_views,
        &library_title,
        &window_title,
        show_content_if_collapsed,
    );
    {
        // Stage 3 Task 6: the mirror image of `track_list.set_on_playlist_
        // mutated` above — a drag-and-drop drop onto a sidebar playlist row
        // mutates the playlist from the *sidebar* side, so the track list
        // needs the reload here instead (covers the edge case where the
        // playlist just dropped onto is also the one currently on screen).
        // `Weak`, not a strong `Rc`, same reasoning as every other cross-
        // widget callback in this function.
        let track_list_weak = Rc::downgrade(&track_list);
        sidebar.set_on_tracks_added(move || match track_list_weak.upgrade() {
            Some(track_list) => track_list.reload(),
            None => tracing::warn!("track list reload skipped: track list is gone"),
        });
    }
    {
        // Stage 3 Task 6 review finding #1: lets `ui::track_list_dnd`'s
        // `REPRISE_SMOKE_DND=addplaylist:<name>` hook drive the exact same
        // drop-handling sequence a real pointer drag onto a sidebar playlist
        // row runs (DB write, sidebar rebuild + toast, `on_tracks_added` ->
        // the `sidebar.set_on_tracks_added` reload just above) instead of
        // calling `library::playlists::add_tracks` directly — see `Sidebar::
        // handle_playlist_drop`'s doc comment. `Weak`, not a strong `Rc`,
        // same reasoning as every other cross-widget callback in this
        // function.
        let sidebar_weak = Rc::downgrade(&sidebar);
        track_list.set_on_sidebar_playlist_drop(move |playlist_id, playlist_name, ids| {
            match sidebar_weak.upgrade() {
                Some(sidebar) => sidebar.handle_playlist_drop(playlist_id, playlist_name, ids),
                None => {
                    tracing::warn!(
                        "sidebar is gone; cannot dispatch simulated sidebar playlist drop"
                    );
                    false
                }
            }
        });
    }

    let search_restore_guard = super::view_session::new_search_restore_guard();
    super::view_session::wire_search(
        &search_entry,
        track_list.clone(),
        search_restore_guard.clone(),
    );
    super::view_session::arm_smoke(
        &search_entry,
        &track_list,
        &sidebar,
        &window_title,
        &search_restore_guard,
    );
    // Stage 3 Task 9: Space/Ctrl+F/Escape. Wired here, right after `wire_
    // search` — `search_entry` and `track_list` are both fully built and
    // wired to each other by this point, and `player`/`window`/`app` all
    // already exist too, so nothing about shortcut wiring needs to wait for
    // anything still to come. `track_list` is passed by reference (it's
    // still needed further down: `wire_scan_button`, `arm_smoke_rescan`, and
    // eventually a final move into `playlist_io::arm_smoke_m3u`).
    shortcuts::wire(app, &window, &search_entry, &track_list, player.clone());
    // Cloned (not moved) here: `arm_smoke_rescan` below needs its own
    // `db_path`/`track_list`/`sidebar` to call `spawn_scan` with, the same
    // way a real button click would.
    super::scan_flow::wire_scan_button(
        &scan_controls,
        &window,
        &toast_overlay,
        db_path.to_path_buf(),
        track_list.clone(),
        sidebar.clone(),
        watcher_state.clone(),
    );
    super::scan_flow::arm_smoke_rescan(
        &scan_controls,
        &toast_overlay,
        db_path.to_path_buf(),
        track_list.clone(),
        sidebar.clone(),
        watcher_state.clone(),
    );

    // Stage 3 Task 8: if a folder has ever been scanned before (this launch
    // or a previous one — `library_root` is persisted in the `settings`
    // table), start the watcher on it immediately so live updates work from
    // the very first frame, without the user re-scanning just to re-arm it.
    // No persisted root yet (a fresh install) is the ordinary, expected case
    // — logged at debug, not a warning.
    {
        let root = {
            let conn = conn.borrow();
            settings::get_library_root(&conn)
        };
        match root {
            Ok(Some(root)) => super::scan_flow::start_or_restart_watcher(
                &watcher_state,
                &PathBuf::from(root),
                db_path.to_path_buf(),
                Rc::downgrade(&track_list),
                Rc::downgrade(&sidebar),
            ),
            Ok(None) => {
                tracing::debug!("no persisted library root; watcher not started at startup");
            }
            Err(error) => {
                tracing::error!(%error, "failed to read persisted library root at startup");
            }
        }
    }

    // Stage 3 Task 7: the import action lives beside playlist creation in
    // the sidebar and is wired after every widget/callback it needs exists.
    playlist_io::wire_import_action(&window, &toast_overlay, conn.clone(), &sidebar);
    playlist_io::arm_smoke_m3u(conn.clone(), &toast_overlay, sidebar.clone());

    super::window_smoke::arm_bar_position(conn, &library_player_bar);

    super::lyrics_smoke::arm(player.as_ref(), &info_panel, conn);

    super::session_restore::restore_runtime(
        &search_entry,
        &track_list,
        &sidebar,
        &window_title,
        &search_restore_guard,
        player.as_ref(),
        &session_state,
    );
    let restored_source = super::view_session::snapshot(&track_list).source;
    library_title.set_library_navigation_visible(matches!(restored_source, ViewSource::Library));
    super::session_restore::wire_close(
        &window,
        conn,
        &track_list,
        player.as_ref(),
        &session_state,
        &geometry_guard,
    );
    super::session_restore::arm_seed_close(&window);
    super::first_run::run(&window, &scan_button, conn, first_run_decision);
    minimal_view.apply_initial();
    if std::env::var(SMOKE_QUIT_ENV_VAR).is_ok() {
        let delay_secs = std::env::var(SMOKE_QUIT_DELAY_SECS_ENV_VAR)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(SMOKE_QUIT_DELAY_SECS_DEFAULT);
        tracing::info!(
            delay_secs,
            "{} set: arming headless smoke-quit timer",
            SMOKE_QUIT_ENV_VAR
        );
        let smoke_window = window.clone();
        glib::timeout_add_seconds_local(delay_secs, move || {
            tracing::info!("smoke-quit timer fired: closing main window");
            smoke_window.close();
            glib::ControlFlow::Break
        });
    }

    tracing::info!("main window built");
    window.present();
    FileOpenHandler::new(&window, conn.clone(), player, &toast_overlay, sidebar)
}

/// Ordered track ids for `artist`, album-ordered — the natural order for the
/// Artists hero's Play all / Shuffle / Add-to-queue actions.
fn artist_track_ids(
    conn: &Rc<RefCell<Connection>>,
    artist: String,
) -> Result<Vec<i64>, rusqlite::Error> {
    let conn = conn.borrow();
    reprise_core::queries::query_track_ids(
        &conn,
        &ViewSource::Artist(artist),
        "album",
        "asc",
        "",
        &[],
    )
}

/// Fisher–Yates shuffle for the Artists hero "Shuffle" action. `reprise-gnome`
/// carries no direct `rand`/`fastrand` dependency (the crate split kept its dep
/// set minimal), so this seeds a tiny xorshift64 from the wall clock rather
/// than pulling in a new crate. A listen-order shuffle is not security
/// sensitive, so a non-cryptographic PRNG is appropriate here.
fn shuffle_ids(mut ids: Vec<i64>) -> Vec<i64> {
    // `| 1` guards against the degenerate all-zero xorshift state.
    let mut state = (glib::real_time() as u64) | 1;
    for i in (1..ids.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        ids.swap(i, j);
    }
    ids
}

/// Opens the containing folder of the artist's first (album-ordered) track in
/// the desktop file manager, via `gio::AppInfo::launch_default_for_uri` on the
/// parent directory's `file://` URI — the same default-handler path
/// `preference_lastfm.rs` uses for external URLs. Logs and returns on any
/// lookup/launch failure.
fn open_artist_folder(conn: &Rc<RefCell<Connection>>, artist: &str) {
    let path = {
        let conn = conn.borrow();
        let ids = match reprise_core::queries::query_track_ids(
            &conn,
            &ViewSource::Artist(artist.to_string()),
            "album",
            "asc",
            "",
            &[],
        ) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(%error, "artist go-to-folder query failed");
                return;
            }
        };
        let Some(&first) = ids.first() else {
            return;
        };
        match reprise_core::queries::query_track_summary(&conn, first) {
            Ok(Some(summary)) => summary.path,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(%error, "artist go-to-folder path lookup failed");
                return;
            }
        }
    };

    let Some(dir) = Path::new(&path).parent() else {
        tracing::warn!(path, "artist track has no parent directory");
        return;
    };
    let uri = gtk4::gio::File::for_path(dir).uri();
    if let Err(error) =
        gtk4::gio::AppInfo::launch_default_for_uri(&uri, gtk4::gio::AppLaunchContext::NONE)
    {
        tracing::warn!(%error, %uri, "failed to open artist folder");
    }
}
