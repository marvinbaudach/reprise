use std::fs::File;
use std::os::fd::IntoRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::source::{SafSource, SafSourceError, SourceChild, SourceFacts};
use super::{MusicLibrary, WindowRange};

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

#[test]
fn artwork_is_resolved_only_by_the_lazy_track_call() {
    let directory = tempfile::tempdir().unwrap();
    let album_directory = directory.path().join("music");
    std::fs::create_dir(&album_directory).unwrap();
    let track_path = album_directory.join("sine.flac");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(fixture, &track_path).unwrap();
    let db_path = directory.path().join("reprise.db");
    let db = reprise_core::db::Db::open_migrated(Some(&db_path)).unwrap();
    reprise_core::library::scanner::scan_folder(&db, &album_directory).unwrap();
    drop(db);

    let library = MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
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

    let tracks = library
        .list_tracks(WindowRange {
            offset: 0,
            limit: 500,
        })
        .unwrap()
        .rows;

    assert_eq!(
        open_calls.load(Ordering::Relaxed),
        0,
        "list_tracks must remain a metadata-only paged query",
    );
    let artwork = library.track_artwork(&tracks[0].uri);
    assert_eq!(open_calls.load(Ordering::Relaxed), 2);
    assert!(artwork.is_some());
    assert!(Path::new(&artwork.unwrap()).starts_with(directory.path().join("cache/reprise/covers")));
}
