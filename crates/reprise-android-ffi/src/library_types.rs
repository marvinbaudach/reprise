//! Owned library records and errors that form the Android FFI contract.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reprise_core::artist_portrait::{PortraitBackfill, PortraitError, PortraitOutcome};
use reprise_core::db::Db;
use reprise_core::library::scanner::ScanProgress;

use crate::source::BridgedSource;

pub(crate) const DATABASE_FILE_NAME: &str = "reprise.db";

pub(crate) type PortraitFetch =
    dyn Fn(&str, &Path) -> Result<PortraitOutcome, PortraitError> + Send + Sync;

pub(crate) struct ConfiguredTree {
    pub(crate) uri: PathBuf,
    pub(crate) source: Arc<BridgedSource>,
}

// Lock order for any operation that needs both is `writer` before `tree`.
// The reader is never held together with either of them.
#[derive(uniffi::Object)]
pub struct MusicLibrary {
    pub(crate) writer: Arc<Mutex<Db>>,
    pub(crate) reader: Arc<Mutex<Db>>,
    pub(crate) tree: Mutex<Option<ConfiguredTree>>,
    pub(crate) cache_root: PathBuf,
    pub(crate) database_path: PathBuf,
    pub(crate) portrait_fetch: Arc<PortraitFetch>,
    pub(crate) portrait_backfill: PortraitBackfill,
}

impl MusicLibrary {
    /// A poisoned mutex means another call panicked while holding the
    /// connection. Reporting that as an error beats propagating the panic
    /// across the FFI boundary, where it would abort the app process.
    pub(crate) fn writer(&self) -> Result<std::sync::MutexGuard<'_, Db>, LibraryError> {
        self.writer.lock().map_err(|_| LibraryError::Database {
            detail: "library handle poisoned by an earlier panic".to_owned(),
        })
    }

    pub(crate) fn reader(&self) -> Result<std::sync::MutexGuard<'_, Db>, LibraryError> {
        self.reader.lock().map_err(|_| LibraryError::Database {
            detail: "library handle poisoned by an earlier panic".to_owned(),
        })
    }

    pub(crate) fn writer_handle(&self) -> Arc<Mutex<Db>> {
        Arc::clone(&self.writer)
    }

    pub(crate) fn reader_handle(&self) -> Arc<Mutex<Db>> {
        Arc::clone(&self.reader)
    }

    pub(crate) fn configured_tree(&self) -> Result<(PathBuf, Arc<BridgedSource>), LibraryError> {
        let tree = self.tree.lock().map_err(|_| LibraryError::Database {
            detail: "library handle poisoned by an earlier panic".to_owned(),
        })?;
        let tree = tree.as_ref().ok_or(LibraryError::TreeNotConfigured)?;
        Ok((tree.uri.clone(), Arc::clone(&tree.source)))
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

/// The three measured Android artwork slots; all remain lazy per item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidArtworkSize {
    List,
    NowPlaying,
    ArtistDetail,
}

impl AndroidArtworkSize {
    pub(crate) fn thumbnail_size(self) -> reprise_core::cover::ThumbnailSize {
        match self {
            Self::List => reprise_core::cover::ThumbnailSize::MobileList,
            Self::NowPlaying => reprise_core::cover::ThumbnailSize::MobileFull,
            Self::ArtistDetail => reprise_core::cover::ThumbnailSize::MobilePortrait,
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
