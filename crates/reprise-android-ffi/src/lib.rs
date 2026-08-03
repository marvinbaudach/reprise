//! Minimal Android library surface over `reprise-core`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use reprise_core::db::Db;
use reprise_core::library::scanner::{
    scan_folder_with_source_and_progress, ScanOutcome, ScanProgress,
};
use reprise_core::library::settings;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

use source::{BridgedSource, SafSource};

pub mod playback;
mod playback_session;
pub mod source;
mod source_error;
mod source_names;

#[cfg(test)]
mod playback_tests;

pub use playback_session::{
    AndroidPlaybackListener, AndroidPlaybackSession, AndroidPlaybackSnapshot,
};

uniffi::setup_scaffolding!();

const DATABASE_FILE_NAME: &str = "reprise.db";

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
}

/// One row as the UI needs it — deliberately not the full `Track`, so the
/// binding surface stays a decision rather than an accident.
#[derive(uniffi::Record)]
pub struct TrackRow {
    pub uri: String,
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

struct ConfiguredTree {
    uri: PathBuf,
    source: BridgedSource,
}

struct LibraryState {
    db: Db,
    tree: Option<ConfiguredTree>,
}

#[derive(uniffi::Object)]
pub struct MusicLibrary {
    state: Mutex<LibraryState>,
}

#[uniffi::export]
impl MusicLibrary {
    /// Opens the library database inside the app's private directory.
    #[uniffi::constructor]
    pub fn open(app_private_directory: &str) -> Result<Self, LibraryError> {
        let db_path = Path::new(app_private_directory).join(DATABASE_FILE_NAME);
        let db = Db::open_migrated(Some(&db_path)).map_err(|error| LibraryError::Database {
            detail: error.to_string(),
        })?;
        Ok(Self {
            state: Mutex::new(LibraryState { db, tree: None }),
        })
    }

    pub fn set_tree_uri(
        &self,
        tree_uri: String,
        source: Box<dyn SafSource>,
    ) -> Result<(), LibraryError> {
        let mut state = self.lock()?;
        settings::set_library_root(&state.db, &tree_uri).map_err(|error| {
            LibraryError::Database {
                detail: error.to_string(),
            }
        })?;
        state.tree = Some(ConfiguredTree {
            uri: tree_uri.into(),
            source: BridgedSource::new(source),
        });
        Ok(())
    }

    pub fn scan(
        &self,
        progress: Box<dyn ScanProgressListener>,
    ) -> Result<ScanSummary, LibraryError> {
        let state = self.lock()?;
        let tree = state.tree.as_ref().ok_or(LibraryError::TreeNotConfigured)?;
        let outcome =
            scan_folder_with_source_and_progress(&tree.source, &state.db, &tree.uri, |event| {
                progress.on_progress(event.into());
            });
        drop(progress);
        let outcome = outcome.map_err(|error| LibraryError::Scan {
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

    pub fn list_tracks(&self) -> Result<Vec<TrackRow>, LibraryError> {
        let state = self.lock()?;
        let tracks = queries::query_track_window(
            &state.db,
            &ViewSource::Library,
            "title",
            "asc",
            "",
            0,
            i64::MAX,
            &[],
        )
        .map_err(|error| LibraryError::Query {
            detail: error.to_string(),
        })?;
        Ok(tracks
            .into_iter()
            .map(|track| TrackRow {
                uri: track.path,
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
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, LibraryState>, LibraryError> {
        self.state.lock().map_err(|_| LibraryError::Database {
            detail: "library handle poisoned by an earlier panic".to_owned(),
        })
    }
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

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::fd::IntoRawFd;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::{MusicLibrary, ScanProgressListener, ScanProgressUpdate};
    use crate::source::{SafSource, SafSourceError, SourceChild, SourceFacts};

    const TREE_URI: &str = "content://com.android.externalstorage.documents/tree/primary%3AMusic";
    const TRACK_URI: &str = "content://com.android.externalstorage.documents/tree/primary%3AMusic/document/primary%3AMusic%2Fsine.flac";
    const ALBUM_URI: &str = "content://com.android.externalstorage.documents/tree/primary%3AMusic/document/primary%3AMusic%2FSome%20Album";
    const BROKEN_TAGS_URI: &str = "content://com.android.externalstorage.documents/tree/primary%3AMusic/document/primary%3AMusic%2FSome%20Album%2Fbroken-tags.mp3";

    struct OneTrackSource {
        probe_calls: Arc<AtomicUsize>,
    }

    impl SafSource for OneTrackSource {
        fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
            Ok(Some(41))
        }

        fn probe(
            &self,
            uri: String,
            _follow_links: bool,
        ) -> Result<Option<SourceFacts>, SafSourceError> {
            self.probe_calls.fetch_add(1, Ordering::Relaxed);
            Ok(match uri.as_str() {
                TREE_URI => Some(SourceFacts {
                    display_name: Some("Music".to_owned()),
                    is_file: false,
                    is_directory: true,
                    size_bytes: None,
                    modified_unix_ms: Some(1_775_000_000_000),
                    document_id: "primary:Music".to_owned(),
                }),
                TRACK_URI => Some(SourceFacts {
                    display_name: Some("sine.flac".to_owned()),
                    is_file: true,
                    is_directory: false,
                    size_bytes: Some(12_066),
                    modified_unix_ms: Some(1_775_000_123_456),
                    document_id: "primary:Music/sine.flac".to_owned(),
                }),
                _ => None,
            })
        }

        fn list_children(&self, uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
            Ok(if uri == TREE_URI {
                vec![SourceChild {
                    uri: TRACK_URI.to_owned(),
                    display_name: Some("sine.flac".to_owned()),
                    is_file: true,
                    is_directory: false,
                    size_bytes: Some(12_066),
                    modified_unix_ms: Some(1_775_000_123_456),
                    document_id: "primary:Music/sine.flac".to_owned(),
                }]
            } else {
                Vec::new()
            })
        }

        fn open_read_fd(&self, uri: String) -> Result<i32, SafSourceError> {
            if uri != TRACK_URI {
                return Err(SafSourceError::Io {
                    detail: format!("unexpected document: {uri}"),
                });
            }
            let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../android/app/src/main/assets/sine.flac");
            File::open(fixture)
                .map(IntoRawFd::into_raw_fd)
                .map_err(|error| SafSourceError::Io {
                    detail: error.to_string(),
                })
        }
    }

    #[derive(Default)]
    struct RecordingProgress {
        events: Arc<Mutex<Vec<ScanProgressUpdate>>>,
    }

    impl ScanProgressListener for RecordingProgress {
        fn on_progress(&self, progress: ScanProgressUpdate) {
            self.events.lock().unwrap().push(progress);
        }
    }

    struct UntaggedAlbumSource;

    impl SafSource for UntaggedAlbumSource {
        fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
            Ok(Some(41))
        }

        fn probe(
            &self,
            uri: String,
            _follow_links: bool,
        ) -> Result<Option<SourceFacts>, SafSourceError> {
            Ok((uri == TREE_URI).then(|| SourceFacts {
                display_name: Some("Music".to_owned()),
                is_file: false,
                is_directory: true,
                size_bytes: None,
                modified_unix_ms: Some(1_775_000_000_000),
                document_id: "primary:Music".to_owned(),
            }))
        }

        fn list_children(&self, uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
            let children = match uri.as_str() {
                TREE_URI => vec![SourceChild {
                    uri: ALBUM_URI.to_owned(),
                    display_name: Some("Some Album".to_owned()),
                    is_file: false,
                    is_directory: true,
                    size_bytes: None,
                    modified_unix_ms: Some(1_775_000_000_000),
                    document_id: "primary:Music/Some Album".to_owned(),
                }],
                ALBUM_URI => vec![SourceChild {
                    uri: BROKEN_TAGS_URI.to_owned(),
                    display_name: Some("broken-tags.mp3".to_owned()),
                    is_file: true,
                    is_directory: false,
                    size_bytes: Some(1),
                    modified_unix_ms: Some(1_775_000_123_456),
                    document_id: "primary:Music/Some Album/broken-tags.mp3".to_owned(),
                }],
                _ => Vec::new(),
            };
            Ok(children)
        }

        fn open_read_fd(&self, uri: String) -> Result<i32, SafSourceError> {
            if uri != BROKEN_TAGS_URI {
                return Err(SafSourceError::Io {
                    detail: format!("unexpected document: {uri}"),
                });
            }
            let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../reprise-core/tests/fixtures/broken-tags.mp3");
            File::open(fixture)
                .map(IntoRawFd::into_raw_fd)
                .map_err(|error| SafSourceError::Io {
                    detail: error.to_string(),
                })
        }
    }

    #[test]
    fn configured_saf_tree_scans_with_indeterminate_first_progress_and_lists_tracks() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(directory.path().to_str().unwrap()).unwrap();
        let probe_calls = Arc::new(AtomicUsize::new(0));
        library
            .set_tree_uri(
                TREE_URI.to_owned(),
                Box::new(OneTrackSource {
                    probe_calls: Arc::clone(&probe_calls),
                }),
            )
            .unwrap();
        let progress = RecordingProgress::default();
        let events = Arc::clone(&progress.events);

        let summary = library.scan(Box::new(progress)).unwrap();
        let tracks = library.list_tracks().unwrap();

        assert_eq!(summary.added, 1);
        assert_eq!(summary.updated, 0);
        assert_eq!(summary.errors, 0);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].uri, TRACK_URI);
        assert_eq!(tracks[0].title, "sine.flac");
        assert_eq!(
            probe_calls.load(Ordering::Relaxed),
            2,
            "the child cursor's display name must not cost a per-file probe",
        );
        assert!(directory.path().join("reprise.db").is_file());
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                ScanProgressUpdate::Discovering,
                ScanProgressUpdate::Scanning {
                    processed: 1,
                    total: None,
                    current_uri: TRACK_URI.to_owned(),
                },
            ]
        );
    }

    #[test]
    fn untagged_saf_track_uses_the_provider_parent_name_as_its_album() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(directory.path().to_str().unwrap()).unwrap();
        library
            .set_tree_uri(TREE_URI.to_owned(), Box::new(UntaggedAlbumSource))
            .unwrap();

        let summary = library
            .scan(Box::new(RecordingProgress::default()))
            .unwrap();
        let tracks = library.list_tracks().unwrap();

        assert_eq!(summary.added, 1);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].uri, BROKEN_TAGS_URI);
        assert_eq!(tracks[0].title, "broken-tags.mp3");
        assert_eq!(tracks[0].album, "Some Album");
    }
}
