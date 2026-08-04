use std::fs::File;
use std::os::fd::IntoRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::source::{SafSource, SafSourceError, SourceChild, SourceFacts};
use super::{AndroidArtworkSize, MusicLibrary, WindowRange};

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

/// A minimal `tracing::Subscriber` that records each event's fields as plain
/// text, so a test can assert on what a real log line carried without pulling
/// in `tracing-subscriber`. Mirrors the capture `reprise-core`'s podcast tests
/// use for the same purpose.
#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<Vec<String>>>);

impl CapturedLogs {
    fn joined(&self) -> String {
        self.0.lock().unwrap().join("\n")
    }
}

struct FieldCollector(String);

impl tracing::field::Visit for FieldCollector {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write as _;
        let _ = write!(self.0, " {}={:?}", field.name(), value);
    }
}

struct LogCapture(CapturedLogs);

impl tracing::Subscriber for LogCapture {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut collector = FieldCollector(event.metadata().level().to_string());
        event.record(&mut collector);
        self.0 .0.lock().unwrap().push(collector.0);
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
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
    let artwork = library.track_artwork(&tracks[0].uri, AndroidArtworkSize::List);
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
        .unwrap();

    assert!(artwork.ends_with("-1092.png"), "got {artwork}");
}

/// An environmental failure must not look like "this track has no artwork".
/// The FFI keeps answering `None` — that is the UI contract — but it has to
/// leave a trail, or a full disk suppresses every cover in the library with
/// nothing to find afterwards.
#[test]
fn an_unusable_cover_cache_is_logged_rather_than_passing_as_no_artwork() {
    let directory = tempfile::tempdir().unwrap();
    // A plain file where the cover cache needs a directory: `create_dir_all`
    // fails, the same shape a full disk or a revoked cache dir produces.
    std::fs::create_dir(directory.path().join("cache")).unwrap();
    std::fs::write(directory.path().join("cache/reprise"), b"not a directory").unwrap();
    let (library, _) = library_with_one_album_cover(directory.path());
    let track_uri = library.list_tracks(full_window()).unwrap().rows[0]
        .uri
        .clone();

    let logs = CapturedLogs::default();
    let artwork = tracing::subscriber::with_default(LogCapture(logs.clone()), || {
        library.track_artwork(&track_uri, AndroidArtworkSize::List)
    });

    assert!(artwork.is_none());
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
