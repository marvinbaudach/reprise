//! Minimal Android library surface over `reprise-core`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reprise_core::db::Db;
use reprise_core::library::scanner::{scan_folder_with_source_and_progress, ScanOutcome};
use reprise_core::library::settings;
use reprise_core::queries;

use source::{BridgedSource, SafSource};

mod appearance;
#[cfg(test)]
mod artwork_tests;
mod browse;
mod library_types;
mod logging;
mod play_journal;
mod play_recorder;
pub mod playback;
mod playback_session;
mod playback_settings;
pub mod source;
mod source_error;
mod source_names;

#[cfg(test)]
mod log_capture;
pub use appearance::*;
pub use browse::{
    AlbumRow, AlbumWindow, ArtistRow, ArtistWindow, TrackRow, TrackWindow, WindowRange,
};
pub use library_types::{
    AndroidArtworkSize, LibraryError, MusicLibrary, ScanProgressListener, ScanProgressUpdate,
    ScanSummary,
};
use library_types::{ConfiguredTree, LibraryState, DATABASE_FILE_NAME};
pub use logging::init_logging;
pub use playback_session::{
    AndroidPlaybackListener, AndroidPlaybackSession, AndroidPlaybackSnapshot, AndroidRepeatMode,
};
pub use playback_settings::*;

uniffi::setup_scaffolding!();

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

    /// Persists one row's rating and refuses to report success if the row was
    /// removed after it crossed the boundary.
    pub fn set_track_rating(&self, track_id: i64, rating: i32) -> Result<(), LibraryError> {
        let state = self.lock()?;
        let changed =
            reprise_core::library::stats::set_rating_if_present(&state.db, track_id, rating)
                .map_err(|error| LibraryError::Database {
                    detail: error.to_string(),
                })?;
        if !changed {
            return Err(LibraryError::TrackNotFound { track_id });
        }
        Ok(())
    }

    /// Resolves local artwork lazily for one track and returns its cached
    /// measured-size thumbnail path.
    ///
    /// The two answers are kept apart, the way every sibling method on this
    /// object keeps them apart:
    ///
    /// * `Ok(None)` — **this track has no artwork**. A missing picture, or one
    ///   that does not decode, is an ordinary answer the UI renders as the
    ///   no-artwork symbol. It is not an error and Kotlin must not treat it as
    ///   one.
    /// * `Err(_)` — the library itself could not answer: a handle poisoned by
    ///   an earlier panic, or no configured tree to read covers through. Those
    ///   are the same conditions `set_track_rating`, `scan`, `list_*` and
    ///   `search_tracks` all report as a typed [`LibraryError`], and folding
    ///   them into the `None` that means "no cover" is what made a broken
    ///   library indistinguishable from a picture-less one.
    ///
    /// A full disk still answers `Ok(None)` — the cover cache being unwritable
    /// affects the *thumbnail*, not the library, and every following track will
    /// fail the same way — but it says so in the log rather than passing
    /// silently.
    pub fn track_artwork(
        &self,
        track_uri: &str,
        size: AndroidArtworkSize,
    ) -> Result<Option<String>, LibraryError> {
        let source = {
            let state = self.lock()?;
            let tree = state.tree.as_ref().ok_or(LibraryError::TreeNotConfigured)?;
            Arc::clone(&tree.source)
        };
        let Some(cover) = reprise_core::cover::resolve_source_with_source(
            source.as_ref(),
            Path::new(&track_uri),
            &self.cache_root,
        ) else {
            return Ok(None);
        };
        match reprise_core::cover::thumbnail_with_source(
            source.as_ref(),
            &cover,
            size.thumbnail_size(),
            &self.cache_root,
        ) {
            Ok(path) => Ok(Some(path.to_string_lossy().into_owned())),
            // The cache is unusable: every following track will fail the same
            // way, so this is the one worth waking someone up for.
            Err(error @ reprise_core::cover::CoverError::Io(_)) => {
                tracing::warn!(%error, track = track_uri, "no artwork: cover cache unusable");
                Ok(None)
            }
            // One unrenderable image among many. Expected in the wild, so it
            // stays at debug.
            Err(error) => {
                tracing::debug!(%error, track = track_uri, "no artwork: cover did not decode");
                Ok(None)
            }
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
        AlbumRow, ArtistRow, LibraryError, MusicLibrary, ScanProgressListener, ScanProgressUpdate,
        WindowRange,
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
                    id: 2,
                    uri: first_uri,
                    title: "All I Want".into(),
                    artist: "Joni Mitchell".into(),
                    album: "Blue".into(),
                    duration_ms: 1_160,
                    play_count: 0,
                    rating: 0,
                },
                super::TrackRow {
                    id: 3,
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
                    id: 3,
                    uri: case_uri,
                    title: "A Case of You".into(),
                    artist: "Joni Mitchell".into(),
                    album: "Blue".into(),
                    duration_ms: 1_160,
                    play_count: 27,
                    rating: 4,
                },
                super::TrackRow {
                    id: 2,
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

    #[test]
    fn track_identity_drives_rating_writes_and_a_missing_id_is_an_error() {
        let (_directory, library) = browse_library();
        let track = library
            .search_tracks("A Case of You".into(), full_window())
            .unwrap()
            .rows
            .remove(0);

        assert!(track.id > 0);
        library.set_track_rating(track.id, 5).unwrap();
        let updated = library
            .search_tracks("A Case of You".into(), full_window())
            .unwrap()
            .rows
            .remove(0);
        assert_eq!(updated.id, track.id);
        assert_eq!(updated.rating, 5);

        assert!(matches!(
            library.set_track_rating(i64::MAX, 4),
            Err(LibraryError::TrackNotFound { track_id }) if track_id == i64::MAX
        ));
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
