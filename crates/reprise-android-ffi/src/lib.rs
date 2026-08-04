//! Minimal Android library surface over `reprise-core`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reprise_core::db::Db;
use reprise_core::library::scanner::{
    scan_folder_with_source_and_progress, ScanOutcome, ScanProgress,
};
use reprise_core::library::settings;
use reprise_core::queries;

use source::{BridgedSource, SafSource};

#[cfg(test)]
mod artwork_tests;
mod browse;
pub mod playback;
mod playback_session;
pub mod source;
mod source_error;
mod source_names;

#[cfg(test)]
mod playback_tests;

pub use browse::{
    AlbumRow, AlbumWindow, ArtistRow, ArtistWindow, TrackRow, TrackWindow, WindowRange,
};
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
    source: Arc<BridgedSource>,
}

struct LibraryState {
    db: Db,
    tree: Option<ConfiguredTree>,
}

#[derive(uniffi::Object)]
pub struct MusicLibrary {
    state: Mutex<LibraryState>,
    cache_root: PathBuf,
}

#[uniffi::export]
impl MusicLibrary {
    /// Opens the library database inside the app's private directory.
    #[uniffi::constructor]
    pub fn open(
        app_private_directory: &str,
        app_cache_directory: &str,
    ) -> Result<Self, LibraryError> {
        let db_path = Path::new(app_private_directory).join(DATABASE_FILE_NAME);
        let db = Db::open_migrated(Some(&db_path)).map_err(|error| LibraryError::Database {
            detail: error.to_string(),
        })?;
        Ok(Self {
            state: Mutex::new(LibraryState { db, tree: None }),
            cache_root: PathBuf::from(app_cache_directory),
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
            source: Arc::new(BridgedSource::new(source)),
        });
        Ok(())
    }

    pub fn scan(
        &self,
        progress: Box<dyn ScanProgressListener>,
    ) -> Result<ScanSummary, LibraryError> {
        let state = self.lock()?;
        let tree = state.tree.as_ref().ok_or(LibraryError::TreeNotConfigured)?;
        let outcome = scan_folder_with_source_and_progress(
            tree.source.as_ref(),
            &state.db,
            &tree.uri,
            |event| {
                progress.on_progress(event.into());
            },
        );
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

    pub fn list_tracks(&self, window: WindowRange) -> Result<TrackWindow, LibraryError> {
        let state = self.lock()?;
        queries::query_library_text_search(&state.db, "", window.into())
            .map(TrackWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    pub fn list_albums(&self, window: WindowRange) -> Result<AlbumWindow, LibraryError> {
        let state = self.lock()?;
        queries::query_albums(&state.db, window.into())
            .map(AlbumWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    pub fn list_artists(&self, window: WindowRange) -> Result<ArtistWindow, LibraryError> {
        let state = self.lock()?;
        queries::query_artists(&state.db, window.into())
            .map(ArtistWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    pub fn list_album_tracks(
        &self,
        album: String,
        album_artist: String,
        window: WindowRange,
    ) -> Result<TrackWindow, LibraryError> {
        let album = album.into_boxed_str();
        let album_artist = album_artist.into_boxed_str();
        let state = self.lock()?;
        queries::query_album_tracks(&state.db, &album, &album_artist, window.into())
            .map(TrackWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    pub fn search_tracks(
        &self,
        text: String,
        window: WindowRange,
    ) -> Result<TrackWindow, LibraryError> {
        let text = text.into_boxed_str();
        let state = self.lock()?;
        queries::query_library_text_search(&state.db, &text, window.into())
            .map(TrackWindow::from)
            .map_err(|error| LibraryError::Query {
                detail: error.to_string(),
            })
    }

    /// Resolves local artwork lazily for one track and returns its cached
    /// 168 px thumbnail path. A missing or unreadable image stays `None`,
    /// because "this track has no artwork" is a legitimate answer the UI must
    /// render. An *environmental* failure — a poisoned handle, a full disk, a
    /// cache directory that cannot be created — looks identical from the
    /// Kotlin side, so every one of those leaves a `tracing` line behind
    /// rather than silently suppressing covers forever.
    pub fn track_artwork(&self, track_uri: &str) -> Option<String> {
        let source = {
            let state = match self.lock() {
                Ok(state) => state,
                Err(error) => {
                    tracing::warn!(%error, track = track_uri, "no artwork: library handle unusable");
                    return None;
                }
            };
            Arc::clone(&state.tree.as_ref()?.source)
        };
        let cover = reprise_core::cover::resolve_source_with_source(
            source.as_ref(),
            Path::new(&track_uri),
            &self.cache_root,
        )?;
        match reprise_core::cover::thumbnail_with_source(
            source.as_ref(),
            &cover,
            reprise_core::cover::ThumbnailSize::MobileList,
            &self.cache_root,
        ) {
            Ok(path) => Some(path.to_string_lossy().into_owned()),
            // The cache is unusable: every following track will fail the same
            // way, so this is the one worth waking someone up for.
            Err(error @ reprise_core::cover::CoverError::Io(_)) => {
                tracing::warn!(%error, track = track_uri, "no artwork: cover cache unusable");
                None
            }
            // One unrenderable image among many. Expected in the wild, so it
            // stays at debug.
            Err(error) => {
                tracing::debug!(%error, track = track_uri, "no artwork: cover did not decode");
                None
            }
        }
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

    use super::{
        AlbumRow, ArtistRow, MusicLibrary, ScanProgressListener, ScanProgressUpdate, WindowRange,
    };
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

    fn browse_library() -> (tempfile::TempDir, MusicLibrary) {
        let directory = tempfile::tempdir().unwrap();
        let music = directory.path().join("music");
        std::fs::create_dir(&music).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/assets/sine.flac");
        for (name, title, artist, album_artist, year, track_no, genre) in [
            (
                "blue-2.flac",
                "A Case of You",
                "Joni Mitchell",
                "Joni Mitchell",
                1971,
                2,
                "Folk",
            ),
            (
                "blue-1.flac",
                "All I Want",
                "Joni Mitchell",
                "Joni Mitchell",
                1971,
                1,
                "Folk",
            ),
            (
                "blue-live.flac",
                "Blue Live",
                "Guest",
                "Other Artist",
                2000,
                1,
                "Rock",
            ),
        ] {
            let path = music.join(name);
            std::fs::copy(&fixture, &path).unwrap();
            reprise_core::library::tag_edit::apply_patch_to_file(
                &path,
                &reprise_core::library::tag_edit::TagPatch {
                    title: Some(title.into()),
                    artist: Some(artist.into()),
                    album: Some("Blue".into()),
                    album_artist: Some(album_artist.into()),
                    year: Some(Some(year)),
                    track_no: Some(Some(track_no)),
                    genre: Some(genre.into()),
                },
            )
            .unwrap();
        }
        let db_path = directory.path().join("reprise.db");
        let db = reprise_core::db::Db::open_migrated(Some(&db_path)).unwrap();
        reprise_core::library::scanner::scan_folder(&db, &music).unwrap();
        let rated_track = reprise_core::queries::query_library_text_search(
            &db,
            "A Case of You",
            reprise_core::queries::WindowRange {
                offset: 0,
                limit: 1,
            },
        )
        .unwrap()
        .rows
        .remove(0);
        reprise_core::library::stats::set_rating(&db, rated_track.id, 4).unwrap();
        for played_at in 0..27 {
            reprise_core::library::stats::record_play(&db, rated_track.id, played_at).unwrap();
        }
        drop(db);
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        (directory, library)
    }

    fn full_window() -> WindowRange {
        WindowRange {
            offset: 0,
            limit: 500,
        }
    }

    #[test]
    fn browse_surface_lists_core_album_summaries_in_core_order() {
        let (directory, library) = browse_library();
        let blue_uri = directory
            .path()
            .join("music/blue-1.flac")
            .to_string_lossy()
            .into_owned();
        let live_uri = directory
            .path()
            .join("music/blue-live.flac")
            .to_string_lossy()
            .into_owned();

        assert_eq!(
            library.list_albums(full_window()).unwrap().rows,
            vec![
                AlbumRow {
                    album: "Blue".into(),
                    album_artist: "Joni Mitchell".into(),
                    representative_uri: blue_uri,
                    track_count: 2,
                    year: Some(1971),
                    total_duration_ms: 2_320,
                },
                AlbumRow {
                    album: "Blue".into(),
                    album_artist: "Other Artist".into(),
                    representative_uri: live_uri,
                    track_count: 1,
                    year: Some(2000),
                    total_duration_ms: 1_160,
                },
            ]
        );
    }

    #[test]
    fn browse_surface_lists_core_artist_summaries_in_core_order() {
        let (directory, library) = browse_library();
        let joni_uri = directory
            .path()
            .join("music/blue-1.flac")
            .to_string_lossy()
            .into_owned();
        let other_uri = directory
            .path()
            .join("music/blue-live.flac")
            .to_string_lossy()
            .into_owned();

        assert_eq!(
            library.list_artists(full_window()).unwrap().rows,
            vec![
                ArtistRow {
                    artist: "Joni Mitchell".into(),
                    track_count: 2,
                    album_count: 1,
                    representative_uri: joni_uri,
                },
                ArtistRow {
                    artist: "Other Artist".into(),
                    track_count: 1,
                    album_count: 1,
                    representative_uri: other_uri,
                },
            ]
        );
    }

    #[test]
    fn browse_surface_gets_one_albums_tracks_in_core_order() {
        let (directory, library) = browse_library();
        let first_uri = directory
            .path()
            .join("music/blue-1.flac")
            .to_string_lossy()
            .into_owned();
        let second_uri = directory
            .path()
            .join("music/blue-2.flac")
            .to_string_lossy()
            .into_owned();

        assert_eq!(
            library
                .list_album_tracks(" blue ".into(), "joni mitchell".into(), full_window())
                .unwrap()
                .rows,
            vec![
                super::TrackRow {
                    uri: first_uri,
                    title: "All I Want".into(),
                    artist: "Joni Mitchell".into(),
                    album: "Blue".into(),
                    duration_ms: 1_160,
                    play_count: 0,
                    rating: 0,
                },
                super::TrackRow {
                    uri: second_uri,
                    title: "A Case of You".into(),
                    artist: "Joni Mitchell".into(),
                    album: "Blue".into(),
                    duration_ms: 1_160,
                    play_count: 27,
                    rating: 4,
                },
            ]
        );
    }

    #[test]
    fn browse_surface_searches_shared_fields_in_core_title_order() {
        let (directory, library) = browse_library();
        let case_uri = directory
            .path()
            .join("music/blue-2.flac")
            .to_string_lossy()
            .into_owned();
        let want_uri = directory
            .path()
            .join("music/blue-1.flac")
            .to_string_lossy()
            .into_owned();

        assert_eq!(
            library
                .search_tracks(" folk ".into(), full_window())
                .unwrap()
                .rows,
            vec![
                super::TrackRow {
                    uri: case_uri,
                    title: "A Case of You".into(),
                    artist: "Joni Mitchell".into(),
                    album: "Blue".into(),
                    duration_ms: 1_160,
                    play_count: 27,
                    rating: 4,
                },
                super::TrackRow {
                    uri: want_uri,
                    title: "All I Want".into(),
                    artist: "Joni Mitchell".into(),
                    album: "Blue".into(),
                    duration_ms: 1_160,
                    play_count: 0,
                    rating: 0,
                },
            ]
        );
    }

    #[test]
    fn track_window_carries_real_rating_and_play_count_without_changing_paging() {
        let (_directory, library) = browse_library();

        let window = library
            .search_tracks(
                "folk".into(),
                WindowRange {
                    offset: 0,
                    limit: 1,
                },
            )
            .unwrap();
        let row = &window.rows[0];

        assert_eq!(window.total, 2);
        assert!(window.has_more);
        assert_eq!(row.rating, 4);
        assert_eq!(row.play_count, 27);
    }

    #[test]
    fn browse_surface_exposes_exact_total_and_continuation() {
        let (_directory, library) = browse_library();

        let first = library
            .search_tracks(
                "folk".into(),
                WindowRange {
                    offset: 0,
                    limit: 1,
                },
            )
            .unwrap();
        let second = library
            .search_tracks(
                "folk".into(),
                WindowRange {
                    offset: 1,
                    limit: 1,
                },
            )
            .unwrap();

        assert_eq!(first.total, 2);
        assert_eq!(first.rows.len(), 1);
        assert!(first.has_more);
        assert_eq!(second.total, 2);
        assert_eq!(second.rows.len(), 1);
        assert!(!second.has_more);
        assert_ne!(first.rows[0].uri, second.rows[0].uri);
    }

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
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
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
        let tracks = library.list_tracks(full_window()).unwrap().rows;

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
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        library
            .set_tree_uri(TREE_URI.to_owned(), Box::new(UntaggedAlbumSource))
            .unwrap();

        let summary = library
            .scan(Box::new(RecordingProgress::default()))
            .unwrap();
        let tracks = library.list_tracks(full_window()).unwrap().rows;

        assert_eq!(summary.added, 1);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].uri, BROKEN_TAGS_URI);
        assert_eq!(tracks[0].title, "broken-tags.mp3");
        assert_eq!(tracks[0].album, "Some Album");
    }
}
