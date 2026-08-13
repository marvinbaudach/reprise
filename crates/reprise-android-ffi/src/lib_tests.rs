use std::fs::File;
use std::os::fd::IntoRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::{LibraryError, MusicLibrary, ScanProgressListener, ScanProgressUpdate, WindowRange};
use crate::source::{SafSource, SafSourceError, SourceChild, SourceFacts};
use reprise_core::device_sync::listen_report::ListenReport;

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
fn browse_surface_lists_core_artist_summaries_in_core_order() {
    let (directory, library) = browse_library();
    let joni_representatives = [
        directory
            .path()
            .join("music/blue-1.flac")
            .to_string_lossy()
            .into_owned(),
        directory
            .path()
            .join("music/blue-2.flac")
            .to_string_lossy()
            .into_owned(),
    ];
    let other_uri = directory
        .path()
        .join("music/blue-live.flac")
        .to_string_lossy()
        .into_owned();
    let rows = library.list_artists(full_window()).unwrap().rows;

    assert_eq!(
        rows.iter()
            .map(|row| (row.artist.as_str(), row.track_count, row.album_count))
            .collect::<Vec<_>>(),
        vec![("Joni Mitchell", 2, 1), ("Other Artist", 1, 1),]
    );
    assert!(joni_representatives.contains(&rows[0].representative_uri));
    assert_eq!(rows[1].representative_uri, other_uri);
}

#[test]
fn browse_surface_searches_album_titles_and_artists_with_exact_windows() {
    let (_directory, library) = browse_library();

    let first = library
        .search_albums(
            "BLUE",
            WindowRange {
                offset: 0,
                limit: 1,
            },
        )
        .unwrap();
    let second = library
        .search_albums(
            "blue",
            WindowRange {
                offset: 1,
                limit: 1,
            },
        )
        .unwrap();
    let artist_match = library
        .search_albums("joni mitchell", full_window())
        .unwrap();

    assert_eq!(first.total, 2);
    assert_eq!(first.rows.len(), 1);
    assert!(first.has_more);
    assert_eq!(second.total, 2);
    assert_eq!(second.rows.len(), 1);
    assert!(!second.has_more);
    assert_eq!(artist_match.total, 1);
    assert_eq!(artist_match.rows[0].album_artist, "Joni Mitchell");
}

#[test]
fn browse_surface_searches_effective_artists_with_exact_counts() {
    let (_directory, library) = browse_library();

    let artists = library.search_artists("JONI", full_window()).unwrap();

    assert_eq!(artists.total, 1);
    assert_eq!(artists.rows.len(), 1);
    assert!(!artists.has_more);
    assert_eq!(artists.rows[0].artist, "Joni Mitchell");
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

    let rows = library
        .list_album_tracks(" blue ".into(), "joni mitchell".into(), full_window())
        .unwrap()
        .rows;

    assert_eq!(
        rows.into_iter()
            .map(|row| {
                (
                    row.uri,
                    row.title,
                    row.artist,
                    row.album,
                    row.duration_ms,
                    row.play_count,
                    row.rating,
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                first_uri,
                "All I Want".into(),
                "Joni Mitchell".into(),
                "Blue".into(),
                1_160,
                0,
                0,
            ),
            (
                second_uri,
                "A Case of You".into(),
                "Joni Mitchell".into(),
                "Blue".into(),
                1_160,
                27,
                4,
            ),
        ]
    );
}

#[test]
fn browse_surface_search_matches_genre_metadata_in_core_title_order() {
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

    let rows = library.search_tracks(" folk ", full_window()).unwrap().rows;

    assert_eq!(
        rows.into_iter()
            .map(|row| {
                (
                    row.uri,
                    row.title,
                    row.artist,
                    row.album,
                    row.duration_ms,
                    row.play_count,
                    row.rating,
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                case_uri,
                "A Case of You".into(),
                "Joni Mitchell".into(),
                "Blue".into(),
                1_160,
                27,
                4,
            ),
            (
                want_uri,
                "All I Want".into(),
                "Joni Mitchell".into(),
                "Blue".into(),
                1_160,
                0,
                0,
            ),
        ]
    );
}

#[test]
fn track_window_carries_real_rating_and_play_count_without_changing_paging() {
    let (_directory, library) = browse_library();

    let window = library
        .search_tracks(
            "joni mitchell",
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
            "joni mitchell",
            WindowRange {
                offset: 0,
                limit: 1,
            },
        )
        .unwrap();
    let second = library
        .search_tracks(
            "joni mitchell",
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
        .search_tracks("A Case of You", full_window())
        .unwrap()
        .rows
        .remove(0);

    assert!(track.id > 0);
    library.set_track_rating(track.id, 5).unwrap();
    let updated = library
        .search_tracks("A Case of You", full_window())
        .unwrap()
        .rows
        .remove(0);
    assert_eq!(updated.id, track.id);
    assert_eq!(updated.rating, 5);
    let report = ListenReport::decode(&library.prepare_listen_report(None).unwrap()).unwrap();
    assert!(report.listens.is_empty());
    assert_eq!(report.ratings.len(), 1);
    assert_eq!(report.ratings[0].sequence, 1);
    assert_eq!(report.ratings[0].device_path, "blue-2.flac");
    assert_eq!(report.ratings[0].rating, 5);
    assert!(report.ratings[0].rated_at > 0);

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

/// The cut is a character one. A byte-wise slice through a multi-byte
/// character would not merely shorten the query, it would panic — and it would
/// do it on the one input class a search field sees constantly.
#[test]
fn a_search_text_is_clipped_on_a_character_boundary_and_shorter_ones_are_untouched() {
    use super::{bounded_search_text, MAX_SEARCH_TEXT_CHARS};

    assert_eq!(bounded_search_text("slowdive"), "slowdive");
    assert_eq!(bounded_search_text(""), "");

    let long_ascii = "a".repeat(MAX_SEARCH_TEXT_CHARS + 50);
    assert_eq!(
        bounded_search_text(&long_ascii).chars().count(),
        MAX_SEARCH_TEXT_CHARS
    );

    // Four bytes per character: a byte-wise cut would land inside one of them.
    let long_multibyte = "🎵".repeat(MAX_SEARCH_TEXT_CHARS + 50);
    let clipped = bounded_search_text(&long_multibyte);
    assert_eq!(clipped.chars().count(), MAX_SEARCH_TEXT_CHARS);
    assert!(long_multibyte.starts_with(clipped));
}
