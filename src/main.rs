// The UI (src/ui) is built out incrementally: search wiring lands in Task 9,
// scanning in Task 10. Until then `queries`, `player`, and `library::scanner`
// are not yet called from the UI. Silence dead-code warnings for that
// not-yet-wired-up surface rather than weakening it.
#![allow(dead_code)]

mod db;
mod library;
mod models;
mod player;
mod queries;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use tracing_subscriber::EnvFilter;

/// GNOME application ID; must match the `.desktop` file and D-Bus name used
/// for GNOME integration.
const APP_ID: &str = "org.reprise.Reprise";

fn db_path() -> std::path::PathBuf {
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

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(move |app| {
        ui::window::build(app, conn.clone());
    });

    // No custom CLI arguments exist yet, so `run()` (which reads
    // `std::env::args()`) is used as-is rather than `run_with_args`.
    app.run()
}
