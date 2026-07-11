pub mod db;
pub mod ipc;
pub mod library;
pub mod models;
pub mod player;

use ipc::AppState;
use tracing_subscriber::EnvFilter;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting Reprise");

    let path = db_path();
    tracing::info!(db_path = %path.display(), "opening database");
    let conn = db::open(Some(&path)).expect("failed to open database");
    db::migrate(&conn).expect("database migration failed");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            db: std::sync::Mutex::new(conn),
        })
        .manage(player::PlayerState(std::sync::Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            ipc::get_track_window,
            ipc::scan_music_folder,
            ipc::get_library_stats,
            player::play_track,
            player::toggle_pause,
            player::seek_to,
            player::set_volume,
            player::stop,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Reprise");
}
