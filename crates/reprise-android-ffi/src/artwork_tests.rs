use std::fs::File;
use std::os::fd::IntoRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::log_capture::CapturedLogs;
use super::source::{SafSource, SafSourceError, SourceChild, SourceFacts};
use super::{AndroidArtworkSize, LibraryError, MusicLibrary, WindowRange};

const TINY_IMAGE: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x1c, 0x0c,
    0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64, 0xf8, 0x0f, 0x00,
    0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

struct CountingAlbumSource {
    album_directory: PathBuf,
    cover_path: PathBuf,
    open_calls: Arc<AtomicUsize>,
}

impl SafSource for CountingAlbumSource {
    fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
        Ok(Some(41))
    }

    fn probe(
        &self,
        uri: String,
        _follow_links: bool,
    ) -> Result<Option<SourceFacts>, SafSourceError> {
        let path = PathBuf::from(uri);
        Ok(path.exists().then(|| SourceFacts {
            display_name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            is_file: path.is_file(),
            is_directory: path.is_dir(),
            size_bytes: path.metadata().ok().map(|metadata| metadata.len()),
            modified_unix_ms: None,
            document_id: path.to_string_lossy().into_owned(),
        }))
    }

    fn list_children(&self, uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
        if Path::new(&uri) != self.album_directory {
            return Ok(Vec::new());
        }
        Ok(vec![SourceChild {
            uri: self.cover_path.to_string_lossy().into_owned(),
            display_name: Some("cover.bmp".to_owned()),
            is_file: true,
            is_directory: false,
            size_bytes: Some(TINY_IMAGE.len() as u64),
            modified_unix_ms: None,
            document_id: self.cover_path.to_string_lossy().into_owned(),
        }])
    }

    fn open_read_fd(&self, uri: String) -> Result<i32, SafSourceError> {
        self.open_calls.fetch_add(1, Ordering::Relaxed);
        File::open(uri)
            .map(IntoRawFd::into_raw_fd)
            .map_err(|error| SafSourceError::Io {
                detail: error.to_string(),
            })
    }
}

/// One scanned album whose folder holds a readable `cover.bmp`, with the cover
/// cache pointed at `<directory>/cache`. The caller owns that cache path and
/// may sabotage it before calling.
fn library_with_one_album_cover(directory: &Path) -> (MusicLibrary, Arc<AtomicUsize>) {
    let album_directory = directory.join("music");
    std::fs::create_dir(&album_directory).unwrap();
    let track_path = album_directory.join("sine.flac");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(fixture, &track_path).unwrap();
    let db_path = directory.join("reprise.db");
    let db = reprise_core::db::Db::open_migrated(Some(&db_path)).unwrap();
    reprise_core::library::scanner::scan_folder(&db, &album_directory).unwrap();
    drop(db);

    let library = MusicLibrary::open(
        directory.to_str().unwrap(),
        directory.join("cache").to_str().unwrap(),
    )
    .unwrap();
    let cover_path = album_directory.join("cover.bmp");
    std::fs::write(&cover_path, TINY_IMAGE).unwrap();
    let open_calls = Arc::new(AtomicUsize::new(0));
    library
        .set_tree_uri(
            album_directory.to_string_lossy().into_owned(),
            Box::new(CountingAlbumSource {
                album_directory,
                cover_path,
                open_calls: Arc::clone(&open_calls),
            }),
        )
        .unwrap();
    (library, open_calls)
}

fn full_window() -> WindowRange {
    WindowRange {
        offset: 0,
        limit: 500,
    }
}

#[test]
fn artwork_is_resolved_only_by_the_lazy_track_call() {
    let directory = tempfile::tempdir().unwrap();
    let (library, open_calls) = library_with_one_album_cover(directory.path());

    let tracks = library.list_tracks(full_window()).unwrap().rows;

    assert_eq!(
        open_calls.load(Ordering::Relaxed),
        0,
        "list_tracks must remain a metadata-only paged query",
    );
    let artwork = library
        .track_artwork(&tracks[0].uri, AndroidArtworkSize::List)
        .unwrap();
    assert_eq!(open_calls.load(Ordering::Relaxed), 2);
    assert!(artwork.is_some());
    assert!(Path::new(&artwork.unwrap()).starts_with(directory.path().join("cache/reprise/covers")));
}

#[test]
fn now_playing_artwork_uses_the_1092_pixel_cache_rung() {
    let directory = tempfile::tempdir().unwrap();
    let (library, _) = library_with_one_album_cover(directory.path());
    let track_uri = library.list_tracks(full_window()).unwrap().rows[0]
        .uri
        .clone();

    let artwork = library
        .track_artwork(&track_uri, AndroidArtworkSize::NowPlaying)
        .unwrap()
        .unwrap();

    assert!(artwork.ends_with("-1092.png"), "got {artwork}");
}

/// The two answers `track_artwork` has to keep apart, in one test.
///
/// A poisoned handle is the same condition every sibling method reports as a
/// typed `LibraryError`; folding it into the `None` that means "this track has
/// no picture" is what made a broken library look like a picture-less one. A
/// track whose folder holds no image stays `Ok(None)` — ordinary, not an error.
#[test]
fn a_broken_library_is_an_error_while_a_track_without_a_picture_is_not() {
    let directory = tempfile::tempdir().unwrap();
    let (library, _) = library_with_one_album_cover(directory.path());
    let track_uri = library.list_tracks(full_window()).unwrap().rows[0]
        .uri
        .clone();
    std::fs::remove_file(directory.path().join("music/cover.bmp")).unwrap();

    assert_eq!(
        library
            .track_artwork(&track_uri, AndroidArtworkSize::List)
            .unwrap(),
        None,
        "a track with no picture is an ordinary answer, not a failure",
    );

    // Poisons the handle the only way a mutex can be poisoned: a panic while
    // it is held. The panic message below is expected test output.
    let library = Arc::new(library);
    let poisoner = Arc::clone(&library);
    let panicked = std::thread::spawn(move || {
        let _guard = poisoner.writer.lock().unwrap();
        panic!("poisoning the library handle on purpose");
    })
    .join();
    assert!(panicked.is_err());

    assert!(
        matches!(
            library.track_artwork(&track_uri, AndroidArtworkSize::List),
            Err(LibraryError::Database { .. }),
        ),
        "a poisoned handle must surface the way every sibling method surfaces it",
    );
}

/// A tree that was never configured is the other condition the siblings report
/// rather than swallow: `scan` answers `TreeNotConfigured`, and so does this.
#[test]
fn artwork_without_a_configured_tree_is_the_same_error_a_scan_reports() {
    let directory = tempfile::tempdir().unwrap();
    let library = MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();

    assert!(matches!(
        library.track_artwork(
            "content://provider/document/x.flac",
            AndroidArtworkSize::List
        ),
        Err(LibraryError::TreeNotConfigured),
    ));
}

/// An environmental failure must not look like "this track has no artwork".
/// The FFI keeps answering `Ok(None)` — the cover *cache* is what broke, not
/// the library — but it has to leave a trail, or a full disk suppresses every
/// cover in the library with nothing to find afterwards.
#[test]
fn an_unusable_cover_cache_is_logged_rather_than_passing_as_no_artwork() {
    let directory = tempfile::tempdir().unwrap();
    let (library, open_calls) = library_with_one_album_cover(directory.path());
    let track_uri = library.list_tracks(full_window()).unwrap().rows[0]
        .uri
        .clone();

    assert!(
        library
            .track_artwork(&track_uri, AndroidArtworkSize::NowPlaying)
            .unwrap()
            .is_some(),
        "the fixture cover must resolve before the cache is made unusable",
    );
    assert_eq!(open_calls.load(Ordering::Relaxed), 2);

    // A plain file where the cover cache needs a directory: `create_dir_all`
    // fails, the same shape a full disk or a revoked cache dir produces.
    std::fs::remove_dir_all(directory.path().join("cache/reprise")).unwrap();
    std::fs::write(directory.path().join("cache/reprise"), b"not a directory").unwrap();

    let logs = CapturedLogs::default();
    let artwork = logs.capture(|| library.track_artwork(&track_uri, AndroidArtworkSize::List));

    assert_eq!(artwork.unwrap(), None);
    assert_eq!(
        open_calls.load(Ordering::Relaxed),
        4,
        "the fixture cover must resolve and reach the unusable cache",
    );
    let logged = logs.joined();
    assert!(logged.contains("WARN"), "expected a warning, got {logged}");
    assert!(
        logged.contains("cover cache unusable"),
        "expected the cache failure to be named, got {logged}"
    );
    assert!(
        logged.contains(&track_uri),
        "expected the affected track to be named, got {logged}"
    );
}
