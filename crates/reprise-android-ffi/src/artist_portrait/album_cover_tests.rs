//! Tests for the on-demand album-cover FFI surface (B2).
//!
//! `cargo test` runs the whole crate's tests in one process, and the memo
//! this module tests is a process-global `static`, so every test here starts
//! with `let _guard = ` [`reset_album_cover_state_for_tests`]`()`, held for
//! the whole test body, and uses its own album name — belt and braces
//! against order-dependence, and the guard against a concurrent test's
//! reset or cancel landing mid-test under cargo's default parallel test
//! threads (B3 review finding 3).

use std::fs::File;
use std::os::fd::IntoRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::source::{SafSource, SafSourceError, SourceChild, SourceFacts};

const TINY_IMAGE: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x1c, 0x0c,
    0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64, 0xf8, 0x0f, 0x00,
    0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

/// A process-unique suffix so tests sharing the crate's one test binary (and
/// this module's process-global memo) never collide on the same album key.
fn unique_album(case: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("{case} {:016x}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn open_gate(library: &MusicLibrary) {
    let writer = library.writer().unwrap();
    reprise_core::online_sources::set_enabled(&writer, true).unwrap();
    reprise_core::modules::set_enabled(&writer, &reprise_core::modules::ARTWORK_MODULE, true)
        .unwrap();
}

/// A `SafSource` that reads straight off the real filesystem — the uri
/// passed around in these tests is a plain absolute path.
struct RealFsSource;

impl SafSource for RealFsSource {
    fn residence_token(&self, _uri: String) -> Result<Option<i64>, SafSourceError> {
        Ok(Some(1))
    }

    fn probe(
        &self,
        uri: String,
        _follow_links: bool,
    ) -> Result<Option<SourceFacts>, SafSourceError> {
        let path = PathBuf::from(&uri);
        Ok(path.exists().then(|| SourceFacts {
            display_name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            is_file: path.is_file(),
            is_directory: path.is_dir(),
            size_bytes: path.metadata().ok().map(|metadata| metadata.len()),
            modified_unix_ms: None,
            document_id: uri,
        }))
    }

    fn list_children(&self, uri: String) -> Result<Vec<SourceChild>, SafSourceError> {
        let Ok(entries) = std::fs::read_dir(&uri) else {
            return Ok(Vec::new());
        };
        Ok(entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                Some(SourceChild {
                    uri: path.to_string_lossy().into_owned(),
                    display_name: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned()),
                    is_file: path.is_file(),
                    is_directory: path.is_dir(),
                    size_bytes: path.metadata().ok().map(|metadata| metadata.len()),
                    modified_unix_ms: None,
                    document_id: path.to_string_lossy().into_owned(),
                })
            })
            .collect())
    }

    fn open_read_fd(&self, uri: String) -> Result<i32, SafSourceError> {
        File::open(uri)
            .map(IntoRawFd::into_raw_fd)
            .map_err(|error| SafSourceError::Io {
                detail: error.to_string(),
            })
    }
}

/// One scanned track credited to `album_artist`/`album`, its tree configured
/// against the real filesystem. No cover anywhere yet.
fn library_with_one_album(
    directory: &Path,
    album_artist: &str,
    album: &str,
) -> (MusicLibrary, String) {
    let music = directory.join("music");
    std::fs::create_dir(&music).unwrap();
    let track_path = music.join("track.flac");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(fixture, &track_path).unwrap();
    reprise_core::library::tag_edit::apply_patch_to_file(
        &track_path,
        &reprise_core::library::tag_edit::TagPatch {
            title: Some("Track One".to_owned()),
            artist: Some(album_artist.to_owned()),
            album: Some(album.to_owned()),
            album_artist: Some(album_artist.to_owned()),
            year: None,
            track_no: Some(Some(1)),
            genre: None,
        },
    )
    .unwrap();

    let library = MusicLibrary::open(
        directory.to_str().unwrap(),
        directory.join("cache").to_str().unwrap(),
    )
    .unwrap();
    library
        .set_tree_uri(music.to_string_lossy().into_owned(), Box::new(RealFsSource))
        .unwrap();
    let writer = library.writer().unwrap();
    reprise_core::library::scanner::scan_folder(&writer, &music).unwrap();
    drop(writer);

    let track_uri = track_path.to_string_lossy().into_owned();
    (library, track_uri)
}

#[test]
fn the_gate_off_fetches_nothing() {
    let _guard = reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let album = unique_album("Gate Off");
    let (library, track_uri) = library_with_one_album(directory.path(), "Gate Off Band", &album);
    // The gate is left off: the default for a freshly opened library.

    let resolved = library
        .album_cover_fetch_with(&track_uri, AndroidArtworkSize::NowPlaying, &|_, _, _| {
            panic!("the gate is off: a fetch must not run")
        })
        .unwrap();

    assert_eq!(resolved, None);
}

#[test]
fn local_art_is_never_replaced_by_a_download() {
    let _guard = reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let album = unique_album("Local Art");
    let (library, track_uri) = library_with_one_album(directory.path(), "Local Art Band", &album);
    open_gate(&library);
    std::fs::write(directory.path().join("music/cover.png"), TINY_IMAGE).unwrap();

    let resolved = library
        .album_cover_fetch_with(&track_uri, AndroidArtworkSize::NowPlaying, &|_, _, _| {
            panic!("local art already resolves: a fetch must not run")
        })
        .unwrap();

    assert!(resolved.is_some(), "the folder cover must resolve locally");
}

#[test]
fn a_fetched_cover_is_found_by_the_resolver() {
    let _guard = reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let album = unique_album("Fetched Cover");
    let (library, track_uri) = library_with_one_album(directory.path(), "Fetched Band", &album);
    open_gate(&library);
    let cache_root = directory.path().join("cache");

    let resolved = library
        .album_cover_fetch_with(
            &track_uri,
            AndroidArtworkSize::NowPlaying,
            &move |album_artist, album, _mbid| {
                let key = reprise_core::cover_download::album_key(album_artist, album);
                let dir = reprise_core::cover_download::downloaded_dir_in(&cache_root);
                std::fs::create_dir_all(&dir).unwrap();
                let path = dir.join(format!("{key}.png"));
                std::fs::write(&path, TINY_IMAGE).unwrap();
                CoverFetchOutcome::Downloaded(path)
            },
        )
        .unwrap();

    assert!(
        resolved.is_some(),
        "the resolver must find the cover the fetch just wrote",
    );
}

#[test]
fn a_miss_is_remembered_for_the_process() {
    let _guard = reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let album = unique_album("Remembered Miss");
    let (library, track_uri) =
        library_with_one_album(directory.path(), "Remembered Miss Band", &album);
    open_gate(&library);
    let fetch_calls = std::cell::Cell::new(0_u32);

    let first = library
        .album_cover_fetch_with(&track_uri, AndroidArtworkSize::NowPlaying, &|_, _, _| {
            fetch_calls.set(fetch_calls.get() + 1);
            CoverFetchOutcome::NotFound
        })
        .unwrap();
    let second = library
        .album_cover_fetch_with(&track_uri, AndroidArtworkSize::NowPlaying, &|_, _, _| {
            panic!("a memorised miss must not fetch again")
        })
        .unwrap();

    assert_eq!(first, None);
    assert_eq!(second, None);
    assert_eq!(fetch_calls.get(), 1);
}

#[test]
fn a_transient_failure_is_retried() {
    let _guard = reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let album = unique_album("Transient Retry");
    let (library, track_uri) =
        library_with_one_album(directory.path(), "Transient Retry Band", &album);
    open_gate(&library);
    let fetch_calls = std::cell::Cell::new(0_u32);

    for _ in 0..2 {
        let resolved = library
            .album_cover_fetch_with(&track_uri, AndroidArtworkSize::NowPlaying, &|_, _, _| {
                fetch_calls.set(fetch_calls.get() + 1);
                CoverFetchOutcome::TransientFailure
            })
            .unwrap();
        assert_eq!(resolved, None);
    }

    assert_eq!(
        fetch_calls.get(),
        2,
        "a transient failure must never be memorised",
    );
}

#[test]
fn the_reader_is_released_before_the_fetch() {
    let _guard = reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let album = unique_album("Reader Released");
    let (library, track_uri) =
        library_with_one_album(directory.path(), "Reader Released Band", &album);
    open_gate(&library);
    let reader_handle = library.reader_handle();

    let resolved = library
        .album_cover_fetch_with(
            &track_uri,
            AndroidArtworkSize::NowPlaying,
            &move |_, _, _| {
                assert!(
                    reader_handle.try_lock().is_ok(),
                    "the reader must be released before the fetch runs",
                );
                CoverFetchOutcome::NotFound
            },
        )
        .unwrap();

    assert_eq!(resolved, None);
}
