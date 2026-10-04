use {rusqlite::Connection, std::path::Path};

#[path = "db_connection.rs"]
mod connection;
#[path = "db_handle.rs"]
mod handle;
pub use crate::db_spectrogram::{
    complete_render_data_track_ids, get_track_spectrogram, get_waveform_peaks,
    pending_render_data_tracks, set_track_render_data, set_track_spectrogram, set_waveform_peaks,
    track_source_fingerprint, PendingRenderDataTrack, SpectrogramStoreOutcome,
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

pub const SUPPORTED_SCHEMA_VERSION: i64 = 87;

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
    crate::db_library_doctor::migrate_v19(conn)?;
    crate::db_tag_write_jobs::migrate_v20(conn)?;
    crate::db_library_doctor_remote::migrate_v21(conn)?;
    crate::db_library_doctor_remote::migrate_v22(conn)?;
    crate::db_mix_planner::migrate_v23(conn)?;
    crate::db_listen_history::migrate_v24(conn)?;
    crate::db_library_exclusions::migrate_v25(conn)?;
    crate::db_new_releases_history::migrate_v26(conn)?;
    crate::db_drop_audio_analysis_mix::migrate_v27(conn)?;
    crate::db_change_log::migrate_v28(conn)?;
    crate::db_ai_jobs::migrate_v29(conn)?;
    crate::db_artist_news_fetch::migrate_v30(conn)?;
    crate::db_concerts::migrate_v31(conn)?;
    crate::db_podcasts_radio::migrate_v32(conn)?;
    crate::db_podcasts_radio::migrate_v33(conn)?;
    crate::db_podcasts_radio::migrate_v34(conn)?;
    crate::db_recently_added::migrate_v35(conn)?;
    crate::db_device_sync::migrate_v36(conn)?;
    crate::db_device_sync::migrate_v37(conn)?;
    crate::db_device_sync::migrate_v38(conn)?;
    crate::db_release_discography::migrate_v39(conn)?;
    crate::db_podcasts_radio::migrate_v40(conn)?;
    crate::db_podcasts_radio::migrate_v41(conn)?;
    crate::db_device_sync::migrate_v42(conn)?;
    crate::db_podcasts_radio::migrate_v43(conn)?;
    crate::db_device_sync::migrate_v44(conn)?;
    crate::db_sync_log::migrate_v45(conn)?;
    crate::db_device_sync::migrate_v46(conn)?;
    crate::db_podcasts_radio::migrate_v47(conn)?;
    crate::db_podcasts_radio::migrate_v48(conn)?;
    crate::db_podcasts_radio::migrate_v49(conn)?;
    crate::db_online_sources::migrate_v50(conn, initial_version > 0, cover_cache, portrait_cache)?;
    crate::db_podcasts_radio::migrate_v51(conn)?;
    crate::db_podcasts_radio::migrate_v52(conn)?;
    crate::db_equalizer::migrate_v53(conn)?;
    crate::db_play_journal::migrate_v54(conn)?;
    crate::db_spectrogram::migrate_v55(conn)?;
    crate::db_new_releases_accent::migrate_v56(conn)?;
    crate::db_drop_sound_features::migrate_v57(conn)?;
    crate::db_library_doctor::migrate_v58(conn)?;
    crate::db_podcasts_radio::migrate_v59(conn)?;
    crate::db_drop_sound_features::migrate_v60(conn)?;
    crate::db_mobile_sync::migrate_v61(conn)?;
    crate::db_releases_view_scope::migrate_v62(conn)?;
    crate::db_listens_back::migrate_v63(conn)?;
    crate::db_mobile_sync::migrate_v64(conn)?;
    crate::db_listens_back::migrate_v65(conn)?;
    crate::db_library_doctor::migrate_v66(conn)?;
    crate::db_library_doctor::migrate_v67(conn)?;
    crate::db_device_sync::migrate_v68(conn)?;
    crate::db_deleted_releases::migrate_v69(conn)?;
    crate::db_deleted_releases::migrate_v70(conn)?;
    crate::db_artwork::migrate_v71(conn)?;
    crate::db_artwork::migrate_v72(conn)?;
    crate::db_concerts::migrate_v73(conn)?;
    crate::db_new_releases_notify::migrate_v74(conn)?;
    crate::db_concerts::migrate_v75(conn)?;
    crate::db_concerts::migrate_v76(conn)?;
    crate::db_podcast_channel_image::migrate_v77(conn)?;
    crate::db_podcast_resume_scope::migrate_v78(conn)?;
    crate::library::settings::migrate_v79(conn)?;
    crate::library::settings::migrate_v80(conn)?;
    crate::db_sync_log::migrate_v81(conn)?;
    crate::db_sort_indexes::migrate_v82(conn)?;
    crate::db_cover_download::migrate_v83(conn)?;
    crate::db_cover_download::migrate_v84(conn)?;
    crate::db_smart_playlist_names::migrate_v85(conn)?;
    crate::db_library_doctor::migrate_v86(conn)?;
    crate::db_device_sync::migrate_v87(conn)?;
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
    fn migration_registry_is_contiguous_and_ends_at_supported_version() {
        let versions = crate::db_migrations::migration_versions();
        let expected: Vec<i64> = (19..=SUPPORTED_SCHEMA_VERSION).collect();

        assert_eq!(versions, expected);
        assert_eq!(versions.last().copied(), Some(SUPPORTED_SCHEMA_VERSION));
    }
}
