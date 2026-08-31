mod i18n;
#[cfg(test)]
mod test_db;
mod ui;

use std::cell::RefCell;
use std::fmt;
use std::path::Path;
use std::rc::Rc;
use std::sync::OnceLock;

use gtk4::gio;
use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::{db, library};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// The canonical runtime application identity. Compile-time resources and
/// packaging mirror this value, with focused tests guarding critical mirrors.
pub const APP_ID: &str = "io.github.marvinbaudach.Reprise";

static APP_RESOURCES_REGISTERED: OnceLock<()> = OnceLock::new();
const APP_ICON_RESOURCE_PATH: &str = "/io/github/marvinbaudach/Reprise/icons";

/// Registers app-private icons once for both the production application and
/// display-backed tests. The resource prefix follows `APP_ID`, allowing
/// `GtkApplication` to expose its `icons` subtree through the active theme.
pub(crate) fn register_app_resources() {
    APP_RESOURCES_REGISTERED.get_or_init(|| {
        gio::resources_register_include!("reprise.gresource")
            .expect("the compiled Reprise resources must register");
    });
}

/// Adds the registered private icon subtree to the theme for the active
/// display. `GtkApplication` also derives this path from `APP_ID`; doing it
/// explicitly keeps the contract testable and independent of startup order.
pub(crate) fn install_app_icon_resource_path() {
    let display = gtk4::gdk::Display::default()
        .expect("the icon resource path requires an initialized GTK display");
    gtk4::IconTheme::for_display(&display).add_resource_path(APP_ICON_RESOURCE_PATH);
}

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
pub(crate) const SMOKE_MPRIS_BUS_ENV_VAR: &str = "REPRISE_SMOKE_MPRIS_BUS_NAME";

/// Initializes tracing to stderr. Level defaults to `info` and can be
/// overridden via the `REPRISE_LOG` environment variable (e.g.
/// `REPRISE_LOG=debug`). This must run before any other startup code so that
/// failures during database setup are visible on the console.
fn init_logging() {
    let filter = EnvFilter::try_from_env("REPRISE_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,lofty=error"));
    let formatting = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(filter);
    tracing_subscriber::registry()
        .with(formatting)
        .with(ui::diagnostics::session_layer().with_filter(ui::diagnostics::session_filter()))
        .init();
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| {
                panic_info
                    .payload()
                    .downcast_ref::<String>()
                    .map(String::as_str)
            })
            .unwrap_or("non-string panic payload");
        let location = panic_info
            .location()
            .map_or_else(|| "unknown location".to_owned(), ToString::to_string);
        tracing::error!(panic_payload = payload, panic_location = location, "panic");
        previous(panic_info);
    }));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DatabaseOpenFailure {
    path: String,
    error: String,
}

impl DatabaseOpenFailure {
    const HEADING: &'static str = "Reprise could not open the database";
    const BODY: &'static str = "Database: {path}\n\nError: {error}";

    fn body(&self, template: &str) -> String {
        i18n::format_message(template, &[("path", &self.path), ("error", &self.error)])
    }
}

fn database_open_result<T, E>(
    path: &Path,
    open: impl FnOnce() -> Result<T, E>,
) -> Result<T, DatabaseOpenFailure>
where
    E: fmt::Display,
{
    open().map_err(|error| DatabaseOpenFailure {
        path: path.display().to_string(),
        error: error.to_string(),
    })
}

fn report_database_open_failure(
    app: &adw::Application,
    failure: DatabaseOpenFailure,
) -> glib::ExitCode {
    app.connect_activate(move |app| {
        let heading = i18n::gettext(DatabaseOpenFailure::HEADING);
        let body = failure.body(&i18n::gettext(DatabaseOpenFailure::BODY));
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Reprise")
            .default_width(480)
            .default_height(240)
            .build();
        let dialog = adw::AlertDialog::builder()
            .heading(heading)
            .body(body)
            .close_response("close")
            .build();
        dialog.add_response("close", &i18n::gettext("Close"));
        let weak_app = app.downgrade();
        window.present();
        dialog.choose(Some(&window), gio::Cancellable::NONE, move |_| {
            if let Some(app) = weak_app.upgrade() {
                app.quit();
            }
        });
    });
    let _ = app.run();
    glib::ExitCode::FAILURE
}

/// Backs the `REPRISE_SMOKE_SEED_PLAYLIST` hook (see `SEED_PLAYLIST_ENV_VAR`'s
/// doc comment for why descending title order): reads every track id
/// currently in the library, then creates `name` with them via `library::
/// playlists::create_with_tracks`. Returns `(playlist_id, track_count)`.
fn seed_playlist_from_library(db: &db::Db, name: &str) -> rusqlite::Result<(i64, usize)> {
    let ids = reprise_core::queries::query_track_ids_by_title_desc(db)?;
    let count = ids.len();
    let playlist_id = library::playlists::create_with_tracks(db, name, &ids)?;
    Ok((playlist_id, count))
}

fn application_flags() -> gio::ApplicationFlags {
    application_flags_for_smoke_bus(std::env::var_os(SMOKE_MPRIS_BUS_ENV_VAR).is_some())
}

fn application_flags_for_smoke_bus(has_smoke_bus: bool) -> gio::ApplicationFlags {
    let mut flags = gio::ApplicationFlags::HANDLES_OPEN;
    if has_smoke_bus {
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    flags
}

type SharedFileOpenHandler = Rc<RefCell<Option<ui::file_open::FileOpenHandler>>>;

fn ensure_window(
    app: &adw::Application,
    conn: &Rc<Db>,
    db_path: &std::path::Path,
    shared: &SharedFileOpenHandler,
    startup_intent: ui::file_open::StartupOpenIntent,
) -> ui::file_open::FileOpenHandler {
    let existing = shared.borrow().clone();
    if let Some(existing) = existing {
        return existing;
    }

    let handler = ui::window::build(app, conn, db_path, startup_intent);
    *shared.borrow_mut() = Some(handler.clone());
    handler
}

fn main() -> glib::ExitCode {
    register_app_resources();
    ui::track_list::diagnostic_trail::mark_process_start();
    init_logging();
    install_panic_hook();
    ui::startup_report::mark("logging initialised");
    i18n::init();
    ui::startup_report::mark("i18n initialised");
    crate::ui::date_format::init();
    i18n::smoke_report();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting Reprise");

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(application_flags())
        .build();
    app.connect_startup(|_| install_app_icon_resource_path());
    ui::startup_report::mark("adw::Application built");

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
    // keeps the existing synchronous `Rc<Db>` plumbing
    // into `connect_activate` untouched — and it gives the secondary a
    // natural place to say goodbye out loud.
    if let Err(error) = app.register(gio::Cancellable::NONE) {
        // No session bus (or another registration failure): uniqueness
        // can't be established, so behave as a standalone primary — the
        // same degraded-but-working mode GApplication itself falls back
        // to when `run()`'s own registration fails.
        tracing::warn!(%error, "could not register with the session bus; continuing standalone");
    }
    ui::startup_report::mark("app.register() returned");
    if app.is_remote() {
        tracing::info!("Reprise is already running — presenting the existing window");
        // Forwards `activate` to the primary instance and returns once
        // that's done — the primary's activate handler (below, but running
        // in the *other* process) presents its window.
        return app.run();
    }

    let path = db::default_path();
    tracing::info!(db_path = %path.display(), "opening database");
    let conn = match database_open_result(&path, || db::Db::open_migrated(Some(&path))) {
        Ok(conn) => conn,
        Err(failure) => {
            tracing::error!(
                db_path = %path.display(),
                error = %failure.error,
                "could not open or migrate database"
            );
            return report_database_open_failure(&app, failure);
        }
    };
    ui::startup_report::mark("database opened");
    tracing::info!("database ready");
    ui::startup_report::mark("database migrated");

    match path.parent() {
        Some(db_dir) => match library::TagWriteLock::acquire(db_dir) {
            Ok(lock_attempt) => {
                if let Err(error) = library::library_doctor::LibraryDoctor::new(&conn)
                    .finalize_incomplete_writes(lock_attempt)
                {
                    tracing::warn!(%error, "could not recover interrupted tag writes at startup");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not acquire the tag-write recovery lock at startup");
            }
        },
        None => tracing::warn!(
            "could not recover interrupted tag writes: database has no parent directory"
        ),
    }

    // Single-threaded UI: the database handle is shared via Rc, not Arc/Mutex.
    // Core owns the connection and exposes named operations; scans open their
    // own handle over the same path instead of sharing this one across threads.
    let conn = Rc::new(conn);

    if let Ok(dir) = std::env::var(SCAN_DIR_ENV_VAR) {
        tracing::info!(
            dir = %dir,
            "{SCAN_DIR_ENV_VAR} set: running headless dev scan before window shows"
        );
        match library::scanner::scan_folder(&conn, std::path::Path::new(&dir)) {
            Ok(library::scanner::ScanOutcome::Completed(report)) => {
                tracing::info!(?report, "dev scan complete");
            }
            // Mapped onto the same log-error display path as a real scan
            // failure (Task 1.5's interim contract — see `ui::scan::
            // scan_worker`'s `reconcile_outcome` for the GUI-toast
            // equivalent; there is no toast to show here, this hook runs
            // headless before any window exists).
            Ok(library::scanner::ScanOutcome::RootUnavailable { root }) => {
                tracing::error!(
                    root = %root.display(),
                    "dev scan failed: library folder unavailable: {}",
                    root.display()
                );
            }
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
        match seed_playlist_from_library(&conn, &name) {
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
        ui::startup_report::mark("activate");
        // A second `reprise` launch forwards `activate` here (see the
        // `is_remote()` check above for the secondary's side of this).
        // Without this guard, a forwarded activate would build a second
        // window, PlayerController, playbin, and ticker thread all sharing
        // the same database connection.
        let handler = ensure_window(
            app,
            &activate_conn,
            &activate_path,
            &activate_handler,
            ui::file_open::StartupOpenIntent::Library,
        );
        tracing::debug!("presenting existing window");
        handler.present();
    });

    let action_app = app.downgrade();
    let action_conn = conn.clone();
    let action_path = path.clone();
    let action_handler = file_open_handler.clone();
    ui::notifications::install_update_actions(
        app.upcast_ref::<gio::Application>(),
        move |target| {
            let Some(app) = action_app.upgrade() else {
                return;
            };
            let handler = ensure_window(
                &app,
                &action_conn,
                &action_path,
                &action_handler,
                ui::file_open::StartupOpenIntent::Library,
            );
            handler.open_updates_view(target);
        },
    );
    ui::notifications::arm_update_notifications(&app, &conn);
    let open_conn = conn;
    let open_path = path;
    app.connect_open(move |app, files, _hint| {
        let request = ui::file_open::resolve_open_request(&open_conn, files);
        let startup_intent = request.startup_intent();
        let handler = ensure_window(
            app,
            &open_conn,
            &open_path,
            &file_open_handler,
            startup_intent,
        );
        handler.open_request(request);
    });

    app.run()
}

#[cfg(test)]
mod app_identity_tests {
    use super::{APP_ICON_RESOURCE_PATH, APP_ID};

    #[test]
    fn app_id_is_the_flathub_reverse_dns_form() {
        assert_eq!(APP_ID, "io.github.marvinbaudach.Reprise");
    }

    #[test]
    fn app_id_has_between_three_and_five_components() {
        let parts: Vec<&str> = APP_ID.split('.').collect();
        assert!(
            (3..=5).contains(&parts.len()),
            "Flathub requires 3 to 5 components, got {}",
            parts.len()
        );
        for part in &parts {
            assert!(
                part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "component {part:?} contains a character Flathub rejects"
            );
        }
    }

    #[test]
    fn app_icon_resource_path_follows_app_id() {
        let expected_prefix = format!("/{}", APP_ID.replace('.', "/"));
        assert_eq!(APP_ICON_RESOURCE_PATH, format!("{expected_prefix}/icons"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_open_failure_becomes_a_presentable_message() {
        let path = std::path::Path::new("/unopenable/reprise.db");
        let failure = database_open_result(path, || Err::<(), _>("database is locked"))
            .expect_err("the fixture must exercise the error branch");

        assert_eq!(
            DatabaseOpenFailure::HEADING,
            "Reprise could not open the database"
        );
        assert_eq!(
            failure.body(DatabaseOpenFailure::BODY),
            "Database: /unopenable/reprise.db\n\nError: database is locked"
        );
    }

    #[test]
    fn application_accepts_forwarded_file_open_requests() {
        assert!(
            application_flags_for_smoke_bus(false).contains(gio::ApplicationFlags::HANDLES_OPEN)
        );
    }

    #[test]
    fn an_explicit_smoke_bus_runs_as_an_isolated_non_unique_instance() {
        assert!(!application_flags_for_smoke_bus(false).contains(gio::ApplicationFlags::NON_UNIQUE));
        assert!(application_flags_for_smoke_bus(true).contains(gio::ApplicationFlags::NON_UNIQUE));
    }
}
