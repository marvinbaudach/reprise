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

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::FromRawFd;
use std::path::Path;
use std::sync::Mutex;

use reprise_core::db::Db;
use reprise_core::library::scanner::{scan_folder, ScanOutcome};
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

uniffi::setup_scaffolding!();

const PROBE_INITIAL_READ_BYTES: usize = 64;
const PROBE_SEEK_OFFSET: usize = 16;
const PROBE_COMPARISON_BYTES: usize = 32;

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

/// Evidence from probing whether an adopted Android file descriptor supports
/// the `Read + Seek` contract required by `LibraryReadHandle`.
#[derive(Debug, uniffi::Record)]
pub struct FileDescriptorProbeResult {
    pub bytes_read: u64,
    pub read_error: Option<String>,
    pub seek_succeeded: bool,
    pub seek_error: Option<String>,
    pub bytes_read_after_seek: u64,
    pub read_after_seek_error: Option<String>,
    pub bytes_match: Option<bool>,
}

#[uniffi::export]
pub fn probe_file_descriptor(raw_fd: i32) -> FileDescriptorProbeResult {
    if raw_fd < 0 {
        let detail = format!("invalid file descriptor: {raw_fd}");
        return FileDescriptorProbeResult {
            bytes_read: 0,
            read_error: Some(detail.clone()),
            seek_succeeded: false,
            seek_error: Some(detail),
            bytes_read_after_seek: 0,
            read_after_seek_error: None,
            bytes_match: None,
        };
    }

    // ParcelFileDescriptor.detachFd() transfers ownership to Rust. Adopting it
    // here makes File close it on every return path; leaking one descriptor per
    // track would eventually exhaust the Android process descriptor table.
    let mut file = unsafe { File::from_raw_fd(raw_fd) };
    let mut initial = [0; PROBE_INITIAL_READ_BYTES];
    let (bytes_read, read_error) = match file.read(&mut initial) {
        Ok(count) => (count, None),
        Err(error) => (0, Some(error.to_string())),
    };

    let seek_error = match file.seek(SeekFrom::Start(PROBE_SEEK_OFFSET as u64)) {
        Ok(_) => None,
        Err(error) => Some(error.to_string()),
    };
    let seek_succeeded = seek_error.is_none();

    let mut bytes_read_after_seek = 0;
    let mut read_after_seek_error = None;
    let mut bytes_match = None;
    if seek_succeeded {
        let mut after_seek = [0; PROBE_COMPARISON_BYTES];
        match file.read(&mut after_seek) {
            Ok(count) => {
                bytes_read_after_seek = count;
                if read_error.is_none() && PROBE_SEEK_OFFSET + count <= bytes_read && count > 0 {
                    bytes_match = Some(
                        after_seek[..count]
                            == initial[PROBE_SEEK_OFFSET..PROBE_SEEK_OFFSET + count],
                    );
                }
            }
            Err(error) => read_after_seek_error = Some(error.to_string()),
        }
    }

    FileDescriptorProbeResult {
        bytes_read: bytes_read as u64,
        read_error,
        seek_succeeded,
        seek_error,
        bytes_read_after_seek: bytes_read_after_seek as u64,
        read_after_seek_error,
        bytes_match,
    }
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

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::fd::IntoRawFd;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    use super::probe_file_descriptor;

    #[test]
    fn fd_probe_reports_seekable_and_non_seekable_descriptors() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/assets/sine.flac");
        let seekable = probe_file_descriptor(File::open(fixture).unwrap().into_raw_fd());

        assert_eq!(seekable.bytes_read, 64);
        assert_eq!(seekable.read_error, None);
        assert!(seekable.seek_succeeded);
        assert_eq!(seekable.seek_error, None);
        assert_eq!(seekable.bytes_read_after_seek, 32);
        assert_eq!(seekable.read_after_seek_error, None);
        assert_eq!(seekable.bytes_match, Some(true));

        let mut child = Command::new("printf")
            .arg("ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let non_seekable = probe_file_descriptor(stdout.into_raw_fd());
        assert!(child.wait().unwrap().success());

        assert_eq!(non_seekable.bytes_read, 64);
        assert_eq!(non_seekable.read_error, None);
        assert!(!non_seekable.seek_succeeded);
        assert!(non_seekable.seek_error.is_some());
        assert_eq!(non_seekable.bytes_read_after_seek, 0);
        assert_eq!(non_seekable.read_after_seek_error, None);
        assert_eq!(non_seekable.bytes_match, None);
    }
}
