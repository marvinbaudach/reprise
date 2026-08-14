//! Owned library records and errors that form the Android FFI contract.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use reprise_core::db::Db;
use reprise_core::library::scanner::ScanProgress;

use crate::source::BridgedSource;

pub(crate) const DATABASE_FILE_NAME: &str = "reprise.db";

pub(crate) struct ConfiguredTree {
    pub(crate) uri: PathBuf,
    pub(crate) source: Arc<BridgedSource>,
}

pub(crate) struct LibraryState {
    pub(crate) db: Db,
    pub(crate) tree: Option<ConfiguredTree>,
}

#[derive(uniffi::Object)]
pub struct MusicLibrary {
    pub(crate) writer: Mutex<LibraryState>,
    pub(crate) reader: Mutex<Db>,
    pub(crate) cache_root: PathBuf,
    pub(crate) database_path: PathBuf,
}

impl MusicLibrary {
    /// A poisoned mutex means another call panicked while holding the
    /// connection. Reporting that as an error beats propagating the panic
    /// across the FFI boundary, where it would abort the app process.
    pub(crate) fn writer(&self) -> Result<std::sync::MutexGuard<'_, LibraryState>, LibraryError> {
        self.writer.lock().map_err(|_| LibraryError::Database {
            detail: "library handle poisoned by an earlier panic".to_owned(),
        })
    }

    pub(crate) fn reader(&self) -> Result<std::sync::MutexGuard<'_, Db>, LibraryError> {
        self.reader.lock().map_err(|_| LibraryError::Database {
            detail: "library handle poisoned by an earlier panic".to_owned(),
        })
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum LibraryError {
    #[error("database error: {detail}")]
    Database { detail: String },
    #[error("scan error: {detail}")]
    Scan { detail: String },
    #[error("query error: {detail}")]
    Query { detail: String },
    #[error("no library tree is configured")]
    TreeNotConfigured,
    #[error("track {track_id} is no longer in the library")]
    TrackNotFound { track_id: i64 },
    #[error("invalid playback setting: {detail}")]
    InvalidPlaybackSetting { detail: String },
    #[error("listen-report journal error: {detail}")]
    ListenReport { detail: String },
}

/// The two measured Android artwork slots; both remain lazy per track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidArtworkSize {
    List,
    NowPlaying,
}

impl AndroidArtworkSize {
    pub(crate) fn thumbnail_size(self) -> reprise_core::cover::ThumbnailSize {
        match self {
            Self::List => reprise_core::cover::ThumbnailSize::MobileList,
            Self::NowPlaying => reprise_core::cover::ThumbnailSize::MobileFull,
        }
    }
}

#[derive(uniffi::Record)]
pub struct ScanSummary {
    pub added: u32,
    pub updated: u32,
    pub errors: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ScanProgressUpdate {
    Discovering,
    Scanning {
        processed: u64,
        total: Option<u64>,
        current_uri: String,
    },
    Fetching {
        done: u64,
        total: u64,
    },
}

#[uniffi::export(callback_interface)]
pub trait ScanProgressListener: Send + Sync {
    fn on_progress(&self, progress: ScanProgressUpdate);
}

impl From<ScanProgress> for ScanProgressUpdate {
    fn from(progress: ScanProgress) -> Self {
        match progress {
            ScanProgress::Discovering => Self::Discovering,
            ScanProgress::Scanning {
                processed,
                total,
                current_path,
            } => Self::Scanning {
                processed,
                total,
                current_uri: current_path.to_string_lossy().into_owned(),
            },
            ScanProgress::Fetching { done, total } => Self::Fetching { done, total },
        }
    }
}
