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
//! own back button automatically). The headerbar also gets an explicit
//! `sidebar-show-symbolic` toggle button, visible only while collapsed, so
//! the sidebar can be brought back without relying solely on that automatic
//! back button. `NavigationSplitView` has no `show-sidebar` property (that's
//! `AdwOverlaySplitView`'s API) — the closest analog is `show-content`
//! (`set_show_content(false)` returns to the sidebar page), which is what
//! the toggle drives (see `wire_sidebar_toggle`).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use reprise_core::library::settings;
use reprise_core::library::watcher::WatcherHandle;

use super::cover_download_worker;
use super::now_playing_wiring;
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
use reprise_core::view_source::ViewSource;

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
pub fn build(app: &adw::Application, conn: &Rc<RefCell<Connection>>, db_path: &Path) {
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

    // Visible only while the split view is collapsed (see `wire_sidebar_
    // toggle`) — at full width both panes already show side by side, so
    // there is nothing to toggle.
    let sidebar_toggle = gtk4::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text(strings::text(strings::SIDEBAR_TOGGLE))
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
    // Module registry: MPRIS is a gated module (`module.mpris.enabled`). Read
    // the flag once here — the one place at startup holding the connection
    // before the controller is built — and thread it into the controller,
    // which owns the `mpris::start` call. A read error must never take the
    // app down: default to on, exactly as a fresh database (no flag row) does.
    let mpris_enabled =
        reprise_core::modules::is_enabled(&conn.borrow(), &reprise_core::modules::MPRIS_MODULE)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not read module.mpris.enabled; defaulting to on");
                true
            });
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
        mpris_enabled,
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
    super::column_header_menu::install(&track_list);
    super::current_track_selection::wire(player.as_ref(), &track_list);
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
    toolbar_view.set_content(Some(&track_content));

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
    let content_nav = library_shell.content_nav;
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
    let minimal_view = super::compact_mode_controls::build_mode(
        &window,
        library_chrome.root.upcast_ref(),
        player.as_ref().map(|player| &player.compact_player),
        conn,
        initial_view,
        &toast_overlay,
    );
    let compact_root = player
        .as_ref()
        .map(|player| player.compact_player.widget().upcast_ref());
    let decorations =
        super::window_decorations::WindowDecorations::new(&window, &header, compact_root);
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
        &split_view,
        &sidebar_page,
        &status_bar,
        &library_player_bar,
        &info_panel,
        &scan_button,
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
        &minimal_view,
        player.as_ref().map(|player| &player.compact_player),
        Rc::new(move || compact_preferences.present()),
    );
    primary_menu::install(
        &header,
        &window,
        &track_list,
        primary_menu::Callbacks {
            on_minimal_view: Rc::new(move || minimal_toggle.toggle()),
            on_preferences: Rc::new(move || preferences.present()),
        },
    );
    header.pack_end(&search_entry);
    cover_batch.start();
    app.set_accels_for_action("win.toggle-minimal-view", &["<Control>m"]);
    super::window_navigation::wire_sidebar_toggle(&sidebar_toggle, &split_view, &sidebar_page);
    // Stage 3 Task 4: sidebar selection drives the track list's source and
    // the headerbar title. Wired here (after `track_list` and `window_title`
    // both exist) rather than at `Sidebar::new` time — see `Sidebar::
    // set_on_select`'s doc comment for why the sidebar's own initial
    // selection doesn't need to round-trip through this callback.
    //
    // Stage 3 Task 4 review finding #1: in collapsed/narrow-window mode,
    // `NavigationSplitView` shows only one of its two pages at a time
    // (`show-content` false = sidebar page, true = content page). Selecting
    // a row used to switch the underlying source but never flip that
    // property, leaving the user staring at the sidebar page after their tap
    // registered. `show_content_if_collapsed` is the fix, shared (via `Rc<dyn
    // Fn()>`) between `on_select` (fires on an actual source change) and
    // `Sidebar::set_on_show_content` (fires on every row activation,
    // including re-activating the already-selected row — see that method's
    // doc comment for why `on_select` alone can't cover a re-tap). A `Weak`
    // upgrade, not a strong capture: `split_view` is about to be handed to
    // `window.set_content` below, and neither callback needs to keep it
    // alive past the window's own lifetime.
    let show_content_if_collapsed = super::window_navigation::show_content_callback(&split_view);
    {
        let track_list = track_list.clone();
        let window_title = window_title.clone();
        let show_content_if_collapsed = show_content_if_collapsed.clone();
        sidebar.set_on_select(move |source, title| {
            track_list.set_source(source);
            window_title.set_title(&title);
            show_content_if_collapsed();
        });
    }
    {
        let show_content_if_collapsed = show_content_if_collapsed.clone();
        sidebar.set_on_show_content(move || show_content_if_collapsed());
    }
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

    window.set_content(Some(library_player_bar.widget()));

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

    // Task 8: wired after `player`/`content_nav` both exist — see
    // `now_playing_wiring.rs`'s doc comments for what each call does.
    now_playing_wiring::wire_bar_expand(player.as_ref(), &content_nav);
    now_playing_wiring::arm_smoke_nowplaying(player.as_ref(), &content_nav);
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
}
