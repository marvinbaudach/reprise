mod db;
mod format;
mod library;
mod models;
mod mpris;
mod player;
mod queries;
mod queue;
mod ui;
mod view_source;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use tracing_subscriber::EnvFilter;

/// GNOME application ID; must match the `.desktop` file and D-Bus name used
/// for GNOME integration. Shared with `mpris` module for MPRIS `DesktopEntry`.
pub(crate) const APP_ID: &str = "org.reprise.Reprise";

/// Dev/verification hook (not a user-facing feature): when set, a folder is
/// scanned into the database synchronously at startup, before the window is
/// shown, so headless tests (`xvfb-run`) can populate the library without a
/// human driving the folder-picker dialog (`ui::window`'s "Scan folder…"
/// button) — a `gtk::FileDialog` portal prompt can't be driven headlessly,
/// so this hook remains the permanent E2E path for scanning. Mirrors the
/// `REPRISE_SMOKE_QUIT` pattern in `ui::window`.
///
/// Usage: `REPRISE_SCAN_DIR=/path/to/music cargo run`.
const SCAN_DIR_ENV_VAR: &str = "REPRISE_SCAN_DIR";

/// The on-disk database path (honors `XDG_DATA_HOME` via `dirs::data_dir`,
/// which is how headless E2E runs point the app at a scratch database
/// without touching `~/.local/share/reprise`). `pub` so `ui::window` can
/// hand the same path to scan-worker threads (Task 10): each worker opens
/// its own `rusqlite::Connection` over this path rather than sharing the
/// UI's `Rc<RefCell<Connection>>` across threads.
pub fn db_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("reprise/reprise.db")
}

/// Initializes tracing to stderr. Level defaults to `info` and can be
/// overridden via the `REPRISE_LOG` environment variable (e.g.
/// `REPRISE_LOG=debug`). This must run before any other startup code so that
/// failures during database setup are visible on the console.
fn init_logging() {
    let filter = EnvFilter::try_from_env("REPRISE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

fn main() -> glib::ExitCode {
    init_logging();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting Reprise");

    let path = db_path();
    tracing::info!(db_path = %path.display(), "opening database");
    let conn = db::open(Some(&path)).expect("failed to open database");
    db::migrate(&conn).expect("database migration failed");
    tracing::info!("database ready");

    // Single-threaded UI: the connection is shared via Rc<RefCell<_>>, not
    // Arc/Mutex. Scans (Task 10) open their own connection over the same
    // path instead of sharing this one across threads.
    let conn = Rc::new(RefCell::new(conn));

    if let Ok(dir) = std::env::var(SCAN_DIR_ENV_VAR) {
        tracing::info!(
            dir = %dir,
            "{SCAN_DIR_ENV_VAR} set: running headless dev scan before window shows"
        );
        let mut conn = conn.borrow_mut();
        match library::scanner::scan_folder(&mut conn, std::path::Path::new(&dir)) {
            Ok(report) => tracing::info!(?report, "dev scan complete"),
            Err(error) => tracing::error!(%error, "dev scan failed"),
        }
    }

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(move |app| {
        // GApplication is single-instance: a second `reprise` launch forwards
        // `activate` to this (the primary) process instead of spawning a new
        // one. Without this guard, a second launch would build a second
        // window, PlayerController, playbin, and ticker thread all sharing
        // the same database connection.
        if let Some(window) = app.active_window() {
            tracing::debug!("presenting existing window");
            window.present();
            return;
        }
        ui::window::build(app, &conn, path.clone());
    });

    // No custom CLI arguments exist yet, so `run()` (which reads
    // `std::env::args()`) is used as-is rather than `run_with_args`.
    app.run()
}
