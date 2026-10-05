use {rusqlite::Connection, std::path::Path};

#[cfg(test)]
use {
    crate::db_grandfather::grandfather_network_features,
    crate::db_schema_baseline::{
        SCHEMA_V1, SCHEMA_V10, SCHEMA_V11, SCHEMA_V12, SCHEMA_V13, SCHEMA_V14, SCHEMA_V15,
        SCHEMA_V17, SCHEMA_V18, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6, SCHEMA_V7,
        SCHEMA_V8, SCHEMA_V9,
    },
};

#[path = "db_connection.rs"]
mod connection;
#[path = "db_handle.rs"]
mod handle;
pub use crate::db_spectrogram::{
    complete_render_data_track_ids, get_track_spectrogram, get_waveform_peaks,
    pending_render_data_tracks, pending_segment_render_data_files, set_track_render_data,
    set_track_spectrogram, set_waveform_peaks, track_source_fingerprint, PendingRenderDataTrack,
    PendingSegmentFile, PendingSegmentTrack, SpectrogramStoreOutcome,
};
#[cfg(test)]
pub(crate) use connection::open_with_options;
pub(crate) use connection::{main_path_connection, open};
pub use handle::Db;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database schema {found} is newer than supported schema {supported}")]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("database schema {found} is not ready; expected schema {supported}")]
    SchemaNotReady { found: i64, supported: i64 },
}

impl From<crate::CoreError> for DbError {
    fn from(error: crate::CoreError) -> Self {
        Self::Sqlite(error.into())
    }
}

pub use crate::db_migrations::SUPPORTED_SCHEMA_VERSION;

/// Default SQLite `busy_timeout` (milliseconds) for every connection opened
/// through [`Db`]: wait up to this long for a write lock instead of failing
/// immediately with `SQLITE_BUSY` — cheap insurance for a concurrent writer
/// (e.g. a scan worker thread's own `Connection` writing while the UI thread
/// reads). Exposed as a named constant so a caller that temporarily overrides
/// the timeout (the change-log prune's non-blocking probe during
/// [`Db::open_migrated`]) can restore exactly this value afterwards.
pub const DEFAULT_BUSY_TIMEOUT_MS: i64 = 5000;

/// Opens Core's internal connection and applies every pending schema migration.
///
/// Public callers construct [`Db`] instead of duplicating these
/// schema-readiness details.
pub(crate) fn open_migrated(path: Option<&Path>) -> Result<Connection, DbError> {
    let conn = open(path)?;
    migrate_connection(&conn)?;
    // Non-blocking, skip-when-idle: this must never stall or fail because a
    // concurrent writer (a running app's long scan transaction) holds the lock —
    // see `events::prune_on_open`. The ~30 GTK `open_migrated(...).unwrap()`
    // call sites and pure-CLI reads both depend on that guarantee.
    crate::events::prune_on_open(&conn)?;
    Ok(conn)
}

/// The on-disk database path (honors `XDG_DATA_HOME` via `dirs::data_dir`,
/// which is how headless E2E runs point the app at a scratch database
/// without touching `~/.local/share/reprise`). Lives in `reprise-core` so
/// every frontend — GNOME today, a future KDE/Qt or macOS client — resolves
/// the *same* library database. Frontends also hand this path to scan-worker
/// threads: each worker opens its own [`Db`] over it rather than sharing the
/// UI's handle across threads.
pub fn default_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("reprise/reprise.db")
}

/// Applies pending schema migrations in order, tracked via `PRAGMA user_version`.
pub(crate) fn migrate_connection(conn: &Connection) -> Result<(), DbError> {
    let cover_cache = crate::cover_download::downloaded_dir();
    let portrait_cache = crate::artist_portrait::cache_dir();
    migrate_with_cache_dirs(conn, &cover_cache, &portrait_cache)
}

pub(crate) fn migrate_with_cache_dirs(
    conn: &Connection,
    cover_cache: &Path,
    portrait_cache: &Path,
) -> Result<(), DbError> {
    let initial_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if initial_version > SUPPORTED_SCHEMA_VERSION {
        return Err(DbError::SchemaTooNew {
            found: initial_version,
            supported: SUPPORTED_SCHEMA_VERSION,
        });
    }
    crate::db_schema_baseline::migrate_baseline(
        conn,
        initial_version > 0,
        cover_cache,
        portrait_cache,
    )?;
    crate::db_migrations::run_migrations(conn, initial_version > 0, cover_cache, portrait_cache)?;
    Ok(())
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "db_recent_migration_tests.rs"]
mod recent_migration_tests;

#[cfg(test)]
#[path = "db_network_migration_tests.rs"]
mod network_migration_tests;

#[cfg(test)]
#[path = "db_stats_migration_tests.rs"]
mod stats_migration_tests;

#[cfg(test)]
#[path = "db_migration_repair_tests.rs"]
mod migration_repair_tests;

#[cfg(test)]
#[path = "db_change_log_migration_tests.rs"]
mod change_log_migration_tests;

#[cfg(test)]
#[path = "db_ai_jobs_migration_tests.rs"]
mod ai_jobs_migration_tests;

#[cfg(test)]
#[path = "library/settings_geometry_migration_tests.rs"]
mod settings_geometry_migration_tests;

#[cfg(test)]
mod migration_registry_tests {
    use super::SUPPORTED_SCHEMA_VERSION;

    #[test]
    fn migration_registry_is_contiguous_without_duplicates() {
        let versions = crate::db_migrations::migration_versions();
        let expected: Vec<i64> = (19..=SUPPORTED_SCHEMA_VERSION).collect();

        assert_eq!(versions, expected);
        // The supported version is the last registry entry by construction. An unregistered
        // `pub(crate) fn migrate_vN` is never called, so `dead_code` under `-D warnings` guards
        // against forgetting the table line; no literal version is pinned here.
    }
}
