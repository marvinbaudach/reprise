//! Throwaway Android feasibility spike (spike branch only — do not merge).
//!
//! The narrowest UniFFI surface that still proves the real thing: open the
//! real database, run the real scanner, and read through the real windowed
//! query layer — all from Kotlin, with `reprise-core` loaded as a `.so` inside
//! an app sandbox rather than executed as a shell binary.
//!
//! `Db` owns a `rusqlite::Connection` and is deliberately not `Sync` (see
//! `db_handle.rs`), so the exported object holds it behind a `Mutex`. That is
//! the same rule the desktop already follows — one handle per thread — and it
//! is worth knowing now that it survives the FFI boundary unchanged.

use std::path::Path;
use std::sync::Mutex;

use reprise_core::db::Db;
use reprise_core::library::scanner::{scan_folder, ScanOutcome};
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum LibraryError {
    #[error("database error: {detail}")]
    Database { detail: String },
    #[error("scan error: {detail}")]
    Scan { detail: String },
    #[error("query error: {detail}")]
    Query { detail: String },
}

/// One row as the UI needs it — deliberately not the full `Track`, so the
/// binding surface stays a decision rather than an accident.
#[derive(uniffi::Record)]
pub struct TrackRow {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
}

#[derive(uniffi::Record)]
pub struct ScanSummary {
    pub added: u32,
    pub updated: u32,
    pub errors: u32,
}

#[derive(uniffi::Object)]
pub struct MusicLibrary {
    db: Mutex<Db>,
}

#[uniffi::export]
impl MusicLibrary {
    /// Opens (creating and migrating if needed) the library at `db_path`.
    #[uniffi::constructor]
    pub fn open(db_path: &str) -> Result<Self, LibraryError> {
        let db = Db::open_migrated(Some(Path::new(&db_path))).map_err(|error| {
            LibraryError::Database {
                detail: error.to_string(),
            }
        })?;
        Ok(Self { db: Mutex::new(db) })
    }

    pub fn scan(&self, folder: &str) -> Result<ScanSummary, LibraryError> {
        let db = self.lock()?;
        let outcome = scan_folder(&db, Path::new(&folder)).map_err(|error| LibraryError::Scan {
            detail: error.to_string(),
        })?;
        match outcome {
            ScanOutcome::Completed(report) => Ok(ScanSummary {
                added: report.added,
                updated: report.updated,
                errors: report.errors,
            }),
            ScanOutcome::RootUnavailable { root } => Err(LibraryError::Scan {
                detail: format!("root unavailable: {}", root.display()),
            }),
        }
    }

    pub fn track_count(&self) -> Result<i64, LibraryError> {
        let db = self.lock()?;
        queries::query_track_count(&db, &ViewSource::Library, "", &[]).map_err(|error| {
            LibraryError::Query {
                detail: error.to_string(),
            }
        })
    }

    pub fn window(&self, offset: i64, limit: i64) -> Result<Vec<TrackRow>, LibraryError> {
        let db = self.lock()?;
        let tracks = queries::query_track_window(
            &db,
            &ViewSource::Library,
            "title",
            "asc",
            "",
            offset,
            limit,
            &[],
        )
        .map_err(|error| LibraryError::Query {
            detail: error.to_string(),
        })?;
        Ok(tracks
            .into_iter()
            .map(|track| TrackRow {
                title: track.title,
                artist: track.artist,
                album: track.album,
                duration_ms: track.duration_ms,
            })
            .collect())
    }
}

impl MusicLibrary {
    /// A poisoned mutex means another call panicked while holding the
    /// connection. Reporting that as an error beats propagating the panic
    /// across the FFI boundary, where it would abort the app process.
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Db>, LibraryError> {
        self.db.lock().map_err(|_| LibraryError::Database {
            detail: "library handle poisoned by an earlier panic".to_owned(),
        })
    }
}
