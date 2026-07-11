//! Builds the main application window: an `adw::NavigationSplitView` (Stage
//! 3 Task 4) whose sidebar page holds `ui::sidebar::Sidebar` and whose
//! content page holds the pre-existing libadwaita `ToolbarView` — a header
//! bar (search entry + scan button) over the track list, a status line + the
//! player bar as stacked bottom bars, and an `adw::ToastOverlay` wrapping
//! everything so scan errors can surface a toast (see `wire_scan_button`).
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
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use crate::library;
use crate::library::scanner::{ScanError, ScanReport};

use super::player_controller::PlayerController;
use super::sidebar::Sidebar;
use super::status_bar::StatusBar;
use super::strings;
use super::track_list::{OnActivate, TrackList};
use crate::view_source::ViewSource;

/// Debounce delay between the last keystroke in the search entry and the
/// track-list reload it triggers, so fast typing doesn't fire a query per
/// keystroke.
const SEARCH_DEBOUNCE_MS: u32 = 200;

const DEFAULT_WIDTH: i32 = 1280;
const DEFAULT_HEIGHT: i32 = 800;
const MIN_WIDTH: i32 = 900;
const MIN_HEIGHT: i32 = 600;

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

/// Dev/verification hook (permanent, like the others in this module and in
/// `track_list.rs`): when set to a directory, arms a one-shot idle callback
/// that calls `spawn_scan` directly — the exact function a real "Scan
/// folder…" click hands off to — once the main loop is up, skipping the
/// portal `gtk::FileDialog` folder picker (not headlessly drivable). Added
/// for Stage 3 Task 4 review finding #2's verification: `main.rs`'s
/// `REPRISE_SCAN_DIR` runs its scan *before* the window/sidebar even exist,
/// so it can never appear as its own "sidebar refresh #N (scan completed)"
/// log line — this hook fires after everything is built and wired, so it
/// does, giving headless E2E a real, attributable post-launch scan to grep
/// for.
///
/// Usage: `REPRISE_SCAN_DIR=<fixtures> REPRISE_SMOKE_RESCAN=<dir2>
/// REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.
const SMOKE_RESCAN_ENV_VAR: &str = "REPRISE_SMOKE_RESCAN";

/// Builds and presents the main window for `app`. `conn` is the shared,
/// already-migrated database connection; the UI layer owns it single-threaded
/// (via `Rc<RefCell<_>>`) and reads through it via `track_list::TrackList`.
/// `db_path` is the same connection's on-disk path, handed to each
/// scan-worker thread so it can open its own `Connection` rather than
/// sharing this one across threads (`rusqlite::Connection` isn't `Send`).
pub fn build(app: &adw::Application, conn: &Rc<RefCell<Connection>>, db_path: PathBuf) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(strings::APP_NAME)
        .default_width(DEFAULT_WIDTH)
        .default_height(DEFAULT_HEIGHT)
        .width_request(MIN_WIDTH)
        .height_request(MIN_HEIGHT)
        .build();

    // Headerbar title follows the currently selected `ViewSource` (Stage 3
    // Task 4); `Library` (`ViewSource::default()`) is both `TrackList`'s and
    // `Sidebar`'s own default initial source, so this is set directly here
    // rather than through a round trip via `Sidebar::set_on_select` (not
    // wired until after `TrackList` exists — see that method's doc comment).
    let window_title = adw::WindowTitle::new(strings::SIDEBAR_MUSIC, "");

    let search_entry = gtk4::SearchEntry::builder()
        .placeholder_text(strings::SEARCH_PLACEHOLDER)
        .build();

    let scan_button = gtk4::Button::with_label(strings::SCAN_FOLDER);

    // Visible only while the split view is collapsed (see `wire_sidebar_
    // toggle`) — at full width both panes already show side by side, so
    // there is nothing to toggle.
    let sidebar_toggle = gtk4::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text(strings::SIDEBAR_TOGGLE)
        .visible(false)
        .build();

    let header = adw::HeaderBar::new();
    header.pack_start(&sidebar_toggle);
    header.set_title_widget(Some(&window_title));
    header.pack_start(&search_entry);
    header.pack_end(&scan_button);

    // The player is created eagerly at window build (not lazily on first
    // activation): construction is cheap (one playbin, no I/O), the
    // `REPRISE_AUDIO_SINK` override keeps headless environments working, and
    // eager creation means the bottom bar exists — greyed out — from the
    // first frame. If GStreamer is unavailable the app degrades to a library
    // browser: error logged, no player bar, activations warn (fault
    // tolerance: never crash over a missing subsystem).
    let player = match PlayerController::new(conn.clone()) {
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
            Some(controller) => controller.queue_ids_snapshot().len(),
            None => 0,
        }
    }));

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
            move |source, count, filter| {
                if matches!(source, ViewSource::Library) {
                    status_bar.refresh(&conn_for_status, filter);
                } else {
                    status_bar.refresh_for_source_count(count as i64);
                }
            },
            queue_ids_provider,
        ))
    };

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(track_list.widget()));

    // Status line stacked directly above the player bar (design mockup 7a):
    // one bottom bar containing both, in this order, rather than relying on
    // `ToolbarView::add_bottom_bar`'s multi-call stacking order.
    let bottom_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    bottom_box.append(status_bar.widget());
    if let Some(player) = &player {
        bottom_box.append(player.bar_widget());
    }
    toolbar_view.add_bottom_bar(&bottom_box);

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

    // Built here — before the sidebar-selection wiring just below, rather
    // than after it — specifically so that wiring can see `split_view`: see
    // this function's doc comment note on Stage 3 Task 4 review finding #1.
    // Both `sidebar_page` and `content_page`'s children (`sidebar.widget()`,
    // `toast_overlay`) already exist by this point, so this reorder has no
    // other dependency to satisfy.
    let sidebar_page = adw::NavigationPage::builder()
        .title(strings::APP_NAME)
        .child(sidebar.widget())
        .build();
    let content_page = adw::NavigationPage::builder()
        .title(strings::APP_NAME)
        .child(&toast_overlay)
        .build();

    let split_view = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .build();
    wire_sidebar_toggle(&sidebar_toggle, &split_view);

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
    let show_content_if_collapsed: Rc<dyn Fn()> = {
        let split_view_weak = split_view.downgrade();
        Rc::new(move || match split_view_weak.upgrade() {
            Some(split_view) => {
                if split_view.is_collapsed() {
                    split_view.set_show_content(true);
                }
            }
            None => tracing::warn!(
                "split view is gone; cannot show content pane after sidebar navigation"
            ),
        })
    };
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

    window.set_content(Some(&split_view));

    wire_search(&search_entry, track_list.clone());
    // Cloned (not moved) here: `arm_smoke_rescan` below needs its own
    // `db_path`/`track_list`/`sidebar` to call `spawn_scan` with, the same
    // way a real button click would.
    wire_scan_button(
        &scan_button,
        &window,
        &toast_overlay,
        db_path.clone(),
        track_list.clone(),
        sidebar.clone(),
    );
    arm_smoke_rescan(&scan_button, &toast_overlay, db_path, track_list, sidebar);

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

/// Wires the headerbar's `sidebar-show-symbolic` toggle to `split_view` (see
/// the module doc's `## Sidebar toggle` section): visible only while
/// collapsed, and its `clicked` state drives `set_show_content` — the
/// closest `AdwNavigationSplitView` analog to "show the sidebar pane" (it
/// has no `show-sidebar` property, unlike `AdwOverlaySplitView`). Every
/// collapse-state flip also resets the toggle to inactive/content-showing,
/// so it starts predictable rather than inheriting whatever a previous wide-
/// layout selection happened to leave.
fn wire_sidebar_toggle(sidebar_toggle: &gtk4::ToggleButton, split_view: &adw::NavigationSplitView) {
    sidebar_toggle.set_visible(split_view.is_collapsed());

    {
        let split_view = split_view.clone();
        sidebar_toggle.connect_toggled(move |button| {
            split_view.set_show_content(!button.is_active());
        });
    }
    {
        let sidebar_toggle = sidebar_toggle.clone();
        split_view.connect_collapsed_notify(move |split_view| {
            sidebar_toggle.set_visible(split_view.is_collapsed());
            sidebar_toggle.set_active(false);
        });
    }
}

/// Wires the header's `SearchEntry` to `track_list`: every `search-changed`
/// emission (GTK already coalesces pure text-composition events for us, but
/// not typing speed) restarts a 200 ms debounce timer, canceling any timer
/// still pending, before reloading the track list with the current text as
/// the filter. `track_list` is moved in and lives for as long as the timer
/// closure — the window itself owns no other reference to it beyond
/// `wire_scan_button`'s copy (both hold an `Rc`), so this is also what keeps
/// it alive for the lifetime of the widget tree.
fn wire_search(search_entry: &gtk4::SearchEntry, track_list: Rc<TrackList>) {
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    search_entry.connect_search_changed(move |entry| {
        if let Some(previous) = pending.borrow_mut().take() {
            previous.remove();
        }
        let text = entry.text().to_string();
        let track_list = track_list.clone();
        let pending_for_timeout = pending.clone();
        let source_id = glib::timeout_add_local(
            std::time::Duration::from_millis(u64::from(SEARCH_DEBOUNCE_MS)),
            move || {
                track_list.set_filter(&text);
                // The timer fired: nothing left to cancel next time.
                pending_for_timeout.borrow_mut().take();
                glib::ControlFlow::Break
            },
        );
        *pending.borrow_mut() = Some(source_id);
    });
}

/// Arms the `REPRISE_SMOKE_RESCAN` hook (see `SMOKE_RESCAN_ENV_VAR`'s doc
/// comment): one idle callback, deferred so it runs once the main loop is up
/// (matching `track_list.rs`'s `arm_smoke_*` hooks), that calls `spawn_scan`
/// with the given directory — exactly what `wire_scan_button`'s click
/// handler does after a folder is chosen, minus the dialog.
fn arm_smoke_rescan(
    scan_button: &gtk4::Button,
    toast_overlay: &adw::ToastOverlay,
    db_path: PathBuf,
    track_list: Rc<TrackList>,
    sidebar: Rc<Sidebar>,
) {
    let Ok(dir) = std::env::var(SMOKE_RESCAN_ENV_VAR) else {
        return;
    };
    tracing::info!(dir = %dir, "{SMOKE_RESCAN_ENV_VAR} set: arming headless post-launch rescan");
    let scan_button = scan_button.clone();
    let toast_overlay = toast_overlay.clone();
    glib::idle_add_local_once(move || {
        spawn_scan(
            PathBuf::from(dir),
            db_path,
            scan_button,
            toast_overlay,
            track_list,
            sidebar,
        );
    });
}

/// Wires the header's "Scan folder…" button: a click opens a portal-friendly
/// `gtk::FileDialog` folder picker; a chosen folder starts a background scan
/// (see `spawn_scan`). Dismissing the dialog without choosing a folder is a
/// normal, expected outcome (not an error) — logged at debug and otherwise
/// ignored.
fn wire_scan_button(
    scan_button: &gtk4::Button,
    window: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    db_path: PathBuf,
    track_list: Rc<TrackList>,
    sidebar: Rc<Sidebar>,
) {
    let window = window.clone();
    let toast_overlay = toast_overlay.clone();
    let scan_button_handle = scan_button.clone();

    scan_button.connect_clicked(move |_| {
        // Disable synchronously, before the async dialog even opens: a
        // second click landing while the first dialog is still up must not
        // be able to spawn a second dialog (and thus a second concurrent
        // scan worker against the same DB). Every exit path below that does
        // *not* hand off to `spawn_scan` must re-enable the button; the
        // `spawn_scan` path re-enables it itself once the scan finishes.
        scan_button_handle.set_sensitive(false);

        let dialog = gtk4::FileDialog::builder()
            .title(strings::SCAN_DIALOG_TITLE)
            .modal(true)
            .build();

        let window = window.clone();
        let toast_overlay = toast_overlay.clone();
        let db_path = db_path.clone();
        let track_list = track_list.clone();
        let sidebar = sidebar.clone();
        let scan_button = scan_button_handle.clone();

        glib::spawn_future_local(async move {
            let folder = match dialog.select_folder_future(Some(&window)).await {
                Ok(folder) => folder,
                Err(error) => {
                    // Dismissed (Escape/Cancel) or Cancelled: the user simply
                    // changed their mind — not a failure worth a toast.
                    if error.matches(gtk4::DialogError::Dismissed)
                        || error.matches(gtk4::DialogError::Cancelled)
                    {
                        tracing::debug!("scan folder dialog dismissed");
                    } else {
                        tracing::error!(%error, "scan folder dialog failed");
                    }
                    scan_button.set_sensitive(true);
                    return;
                }
            };
            let Some(path) = folder.path() else {
                tracing::warn!("selected folder has no local filesystem path; cannot scan");
                scan_button.set_sensitive(true);
                return;
            };

            spawn_scan(
                path,
                db_path,
                scan_button,
                toast_overlay,
                track_list,
                sidebar,
            );
        });
    });
}

/// Starts a background scan of `folder`: disables `scan_button` and swaps
/// its label to "Scanning…", runs `library::scanner::scan_folder` on a
/// `std::thread` against a *separate* `rusqlite::Connection` opened from
/// `db_path` (a `Connection` cannot cross threads), then marshals the result
/// back onto the GTK main thread over an `async_channel` — the same bridge
/// pattern `player_controller.rs` uses for `PlayerEvent`s, except the
/// receive side here is a single one-shot `recv().await` rather than
/// `player_controller.rs`'s long-lived drain loop: this channel is
/// `bounded(1)` and carries exactly one message (the scan's final result),
/// not a stream of events. On success: re-enable the button, reload the
/// track list (`TrackList::reload`'s `on_reload` hook keeps the status line
/// in sync too — see its doc comment — so this doesn't refresh it a second
/// time itself), and refresh the sidebar (trigger #1 from `Sidebar::
/// refresh`'s doc comment — a scan can add tracks/playlists and clear
/// import-error/missing counts, none of which the narrowed `on_reload` hook
/// covers any more). On failure: re-enable the button, log at `error!`, and
/// surface an `adw::Toast` — the app stays fully usable either way (fault
/// tolerance: a scan failure must never wedge the UI or crash the app).
fn spawn_scan(
    folder: PathBuf,
    db_path: PathBuf,
    scan_button: gtk4::Button,
    toast_overlay: adw::ToastOverlay,
    track_list: Rc<TrackList>,
    sidebar: Rc<Sidebar>,
) {
    scan_button.set_sensitive(false);
    scan_button.set_label(strings::SCANNING);
    scan_button.set_tooltip_text(Some(strings::SCANNING));

    let (sender, receiver) = async_channel::bounded::<Result<ScanReport, ScanError>>(1);

    std::thread::spawn(move || {
        let result = run_scan(&db_path, &folder);
        if let Err(error) = sender.send_blocking(result) {
            // The only way `send_blocking` fails on a bounded(1) channel
            // whose one send is happening right here is a closed receiver —
            // i.e. the window (and this whole future) is already gone.
            tracing::warn!(%error, "scan result dropped: UI receiver is gone");
        }
    });

    glib::spawn_future_local(async move {
        let outcome = receiver.recv().await;

        scan_button.set_sensitive(true);
        scan_button.set_label(strings::SCAN_FOLDER);
        scan_button.set_tooltip_text(None);

        match outcome {
            Ok(Ok(report)) => {
                tracing::info!(?report, "scan complete");
                track_list.reload();
                sidebar.refresh("scan completed");
            }
            Ok(Err(error)) => {
                tracing::error!(%error, "scan failed");
                toast_overlay.add_toast(adw::Toast::new(&format!(
                    "{}{error}",
                    strings::SCAN_FAILED_PREFIX
                )));
            }
            Err(error) => {
                // The sender was dropped without sending — the worker thread
                // must have panicked before reaching `send_blocking`.
                tracing::error!(%error, "scan worker channel closed unexpectedly");
                toast_overlay.add_toast(adw::Toast::new(&format!(
                    "{}{error}",
                    strings::SCAN_FAILED_PREFIX
                )));
            }
        }
    });
}

/// Runs on the scan worker thread: opens and migrates its own `Connection`
/// over `db_path` (never the UI's `Rc<RefCell<Connection>>` — see the
/// module doc comment on `spawn_scan`), then scans `folder` through it.
fn run_scan(db_path: &std::path::Path, folder: &std::path::Path) -> Result<ScanReport, ScanError> {
    let mut worker_conn = crate::db::open(Some(db_path))?;
    crate::db::migrate(&worker_conn)?;
    library::scanner::scan_folder(&mut worker_conn, folder)
}
