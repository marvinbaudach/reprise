//! Builds the main application window: a libadwaita `ToolbarView` with a
//! header bar (search entry + scan button) over the track list, a status
//! line + the player bar as stacked bottom bars, and an `adw::ToastOverlay`
//! wrapping everything so scan errors can surface a toast (see
//! `wire_scan_button`).

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
use super::status_bar::StatusBar;
use super::strings;
use super::track_list::{OnActivate, TrackList};

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
const SMOKE_QUIT_DELAY_SECS: u32 = 3;

/// Builds and presents the main window for `app`. `conn` is the shared,
/// already-migrated database connection; the UI layer owns it single-threaded
/// (via `Rc<RefCell<_>>`) and reads through it via `track_list::TrackList`.
/// `db_path` is the same connection's on-disk path, handed to each
/// scan-worker thread so it can open its own `Connection` rather than
/// sharing this one across threads (`rusqlite::Connection` isn't `Send`).
pub fn build(app: &adw::Application, conn: Rc<RefCell<Connection>>, db_path: PathBuf) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(strings::APP_NAME)
        .default_width(DEFAULT_WIDTH)
        .default_height(DEFAULT_HEIGHT)
        .width_request(MIN_WIDTH)
        .height_request(MIN_HEIGHT)
        .build();

    let window_title = adw::WindowTitle::new(strings::APP_NAME, "");

    let search_entry = gtk4::SearchEntry::builder()
        .placeholder_text(strings::SEARCH_PLACEHOLDER)
        .build();

    let scan_button = gtk4::Button::with_label(strings::SCAN_FOLDER);

    let header = adw::HeaderBar::new();
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
    let player = match PlayerController::new() {
        Ok(controller) => Some(controller),
        Err(error) => {
            tracing::error!(%error, "player unavailable: playback disabled");
            None
        }
    };

    let on_activate: OnActivate = {
        let player = player.clone();
        Box::new(move |track| match &player {
            Some(player) => player.play_track(track),
            None => {
                tracing::warn!(path = %track.path, "player unavailable; ignoring activation");
            }
        })
    };

    let status_bar = StatusBar::new();

    let track_list = {
        let status_bar = status_bar.clone();
        let conn_for_status = conn.clone();
        Rc::new(TrackList::new(conn.clone(), on_activate, move || {
            status_bar.refresh(&conn_for_status);
        }))
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

    window.set_content(Some(&toast_overlay));

    wire_search(&search_entry, track_list.clone());
    wire_scan_button(
        &scan_button,
        &window,
        &toast_overlay,
        conn,
        db_path,
        track_list,
        status_bar,
    );

    if std::env::var(SMOKE_QUIT_ENV_VAR).is_ok() {
        tracing::info!(
            delay_secs = SMOKE_QUIT_DELAY_SECS,
            "{} set: arming headless smoke-quit timer",
            SMOKE_QUIT_ENV_VAR
        );
        let smoke_window = window.clone();
        glib::timeout_add_seconds_local(SMOKE_QUIT_DELAY_SECS, move || {
            tracing::info!("smoke-quit timer fired: closing main window");
            smoke_window.close();
            glib::ControlFlow::Break
        });
    }

    tracing::info!("main window built");
    window.present();
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

/// Wires the header's "Scan folder…" button: a click opens a portal-friendly
/// `gtk::FileDialog` folder picker; a chosen folder starts a background scan
/// (see `spawn_scan`). Dismissing the dialog without choosing a folder is a
/// normal, expected outcome (not an error) — logged at debug and otherwise
/// ignored.
fn wire_scan_button(
    scan_button: &gtk4::Button,
    window: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    conn: Rc<RefCell<Connection>>,
    db_path: PathBuf,
    track_list: Rc<TrackList>,
    status_bar: StatusBar,
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
        let status_bar = status_bar.clone();
        let conn = conn.clone();
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
                tracing::warn!(
                    "selected folder has no local filesystem path; cannot scan"
                );
                scan_button.set_sensitive(true);
                return;
            };

            spawn_scan(
                path,
                db_path,
                scan_button,
                toast_overlay,
                track_list,
                status_bar,
                conn,
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
/// track list and status line. On
/// failure: re-enable the button, log at `error!`, and surface an
/// `adw::Toast` — the app stays fully usable either way (fault tolerance: a
/// scan failure must never wedge the UI or crash the app).
fn spawn_scan(
    folder: PathBuf,
    db_path: PathBuf,
    scan_button: gtk4::Button,
    toast_overlay: adw::ToastOverlay,
    track_list: Rc<TrackList>,
    status_bar: StatusBar,
    conn: Rc<RefCell<Connection>>,
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
                status_bar.refresh(&conn);
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
