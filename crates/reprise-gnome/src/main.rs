mod i18n;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::{db, library};
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

/// Dev/verification hook (permanent, like `REPRISE_SCAN_DIR`, which this
/// runs immediately after): when set to a playlist name, creates a manual
/// playlist with that name and seeds it with every track the scan above just
/// found — via `library::playlists::create_with_tracks`, the same
/// transactional primitive the "New playlist…" context-menu action and the
/// M3U importer both use — so a headless E2E can exercise a real,
/// non-trivial playlist (source switch, forced `playlist_order` sort,
/// playback order) without a human driving the "New playlist" dialog (an
/// `AdwAlertDialog`, not headlessly drivable any more than a
/// `gtk::FileDialog` is).
///
/// Seeded in **descending title order** (`ORDER BY title DESC`), not
/// insertion/id order: the library's own default view sorts ascending, so
/// this guarantees the playlist's order is the *reverse* of what the
/// library view would show — a headless run that activates row 0 of each
/// and gets two different tracks is discriminating proof that a `Playlist`
/// source's forced `"playlist_order"` sort (`ui::track_list`'s `default_
/// sort_for_source`) is actually driving playback, not a coincidence of
/// both views agreeing on some other order.
///
/// Requires `REPRISE_SCAN_DIR` (or a prior run's already-scanned database)
/// so there is a library to seed from; a request against an empty library is
/// harmless (`create_with_tracks` with an empty id slice still creates the
/// playlist, just with no tracks — logged, not treated as an error).
///
/// Usage: `REPRISE_SCAN_DIR=/path/to/music REPRISE_SMOKE_SEED_PLAYLIST="My
/// Mix" cargo run`.
const SEED_PLAYLIST_ENV_VAR: &str = "REPRISE_SMOKE_SEED_PLAYLIST";

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

/// Backs the `REPRISE_SMOKE_SEED_PLAYLIST` hook (see `SEED_PLAYLIST_ENV_VAR`'s
/// doc comment for why descending title order): reads every track id
/// currently in the library, then creates `name` with them via `library::
/// playlists::create_with_tracks`. Returns `(playlist_id, track_count)`.
fn seed_playlist_from_library(
    conn: &mut rusqlite::Connection,
    name: &str,
) -> rusqlite::Result<(i64, usize)> {
    let mut statement = conn.prepare("SELECT id FROM tracks ORDER BY title DESC")?;
    let ids: Vec<i64> = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    // Dropped explicitly, before the `&mut Connection` borrow below: `conn.
    // prepare` above borrowed `conn` immutably for `statement`'s lifetime,
    // and `create_with_tracks` needs a `&mut Connection` — the two borrows
    // can't overlap.
    drop(statement);
    let count = ids.len();
    let playlist_id = library::playlists::create_with_tracks(conn, name, &ids)?;
    Ok((playlist_id, count))
}

fn application_flags() -> gio::ApplicationFlags {
    gio::ApplicationFlags::HANDLES_OPEN
}

type SharedFileOpenHandler = Rc<RefCell<Option<ui::file_open::FileOpenHandler>>>;

fn ensure_window(
    app: &adw::Application,
    conn: &Rc<RefCell<rusqlite::Connection>>,
    db_path: &std::path::Path,
    shared: &SharedFileOpenHandler,
) -> ui::file_open::FileOpenHandler {
    let existing = shared.borrow().clone();
    if let Some(existing) = existing {
        return existing;
    }

    let handler = ui::window::build(app, conn, db_path);
    *shared.borrow_mut() = Some(handler.clone());
    handler
}

fn main() -> glib::ExitCode {
    init_logging();
    i18n::init();
    i18n::smoke_report();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting Reprise");

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(application_flags())
        .build();

    // Primary-vs-secondary is decided *before* the database is touched
    // (field finding, Stage 3): GApplication is single-instance, and a
    // second `reprise` launch only forwards `activate` to the primary
    // process — it has no business opening (and taking sqlite locks on)
    // the database it will never use, and previously it also exited with
    // zero user-visible feedback. `register()` is the explicit,
    // documented way to settle instance uniqueness ahead of `run()`
    // (which registers idempotently again later); after it,
    // `is_remote()` says which side this process is. This seam is chosen
    // over moving the DB open into a `connect_startup` closure because it
    // keeps the existing synchronous `Rc<RefCell<Connection>>` plumbing
    // into `connect_activate` untouched — and it gives the secondary a
    // natural place to say goodbye out loud.
    if let Err(error) = app.register(gio::Cancellable::NONE) {
        // No session bus (or another registration failure): uniqueness
        // can't be established, so behave as a standalone primary — the
        // same degraded-but-working mode GApplication itself falls back
        // to when `run()`'s own registration fails.
        tracing::warn!(%error, "could not register with the session bus; continuing standalone");
    }
    if app.is_remote() {
        tracing::info!("Reprise is already running — presenting the existing window");
        // Forwards `activate` to the primary instance and returns once
        // that's done — the primary's activate handler (below, but running
        // in the *other* process) presents its window.
        return app.run();
    }

    let path = db::default_path();
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
        // Stage 3 Task 8: persist this as the library root, exactly like a
        // real "Scan folder…" click does (`ui::window::run_scan`), so
        // `ui::window::build`'s startup check finds it and arms the watcher
        // on it — the dev hook otherwise has no way to reach that behavior,
        // since it runs before any window (and thus any watcher) exists.
        if let Err(error) = library::settings::set_library_root(&conn, &dir) {
            tracing::error!(%error, "failed to persist library root for dev scan hook");
        }
    }

    if let Ok(name) = std::env::var(SEED_PLAYLIST_ENV_VAR) {
        tracing::info!(
            name = %name,
            "{SEED_PLAYLIST_ENV_VAR} set: seeding a playlist from the current library"
        );
        let mut conn = conn.borrow_mut();
        match seed_playlist_from_library(&mut conn, &name) {
            Ok((playlist_id, count)) => tracing::info!(
                playlist_id,
                count,
                "{SEED_PLAYLIST_ENV_VAR}: playlist seeded"
            ),
            Err(error) => {
                tracing::error!(%error, "{SEED_PLAYLIST_ENV_VAR}: failed to seed playlist");
            }
        }
    }

    let file_open_handler: SharedFileOpenHandler = Rc::new(RefCell::new(None));
    let activate_conn = conn.clone();
    let activate_path = path.clone();
    let activate_handler = file_open_handler.clone();
    app.connect_activate(move |app| {
        // A second `reprise` launch forwards `activate` here (see the
        // `is_remote()` check above for the secondary's side of this).
        // Without this guard, a forwarded activate would build a second
        // window, PlayerController, playbin, and ticker thread all sharing
        // the same database connection.
        let handler = ensure_window(app, &activate_conn, &activate_path, &activate_handler);
        tracing::debug!("presenting existing window");
        handler.present();
    });

    let open_conn = conn;
    let open_path = path;
    app.connect_open(move |app, files, _hint| {
        let handler = ensure_window(app, &open_conn, &open_path, &file_open_handler);
        handler.open(files);
    });

    app.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_accepts_forwarded_file_open_requests() {
        assert!(application_flags().contains(gio::ApplicationFlags::HANDLES_OPEN));
    }
}
