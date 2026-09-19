//! Tests for the artist-portrait FFI surface.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use super::*;
use crate::log_capture::CapturedLogs;

struct CapturingProgress(Arc<Mutex<Vec<ArtistPortraitProgressUpdate>>>);

impl ArtistPortraitProgressListener for CapturingProgress {
    fn on_progress(&self, update: ArtistPortraitProgressUpdate) {
        self.0.lock().unwrap().push(update);
    }
}

const TINY_IMAGE: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x1c, 0x0c,
    0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64, 0xf8, 0x0f, 0x00,
    0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

fn store_portrait_fixture_in(dir: &Path, name: &str) -> PathBuf {
    reprise_core::artist_portrait::store_fixture_image(dir, name, TINY_IMAGE, "png")
        .expect("portrait fixture must be stored through the production cache path")
}

fn open_gate(library: &MusicLibrary) {
    let writer = library.writer().unwrap();
    reprise_core::online_sources::set_enabled(&writer, true).unwrap();
    reprise_core::modules::set_enabled(&writer, &reprise_core::modules::ARTWORK_MODULE, true)
        .unwrap();
}

/// `MusicLibrary::open` opens the artwork gate itself
/// (`the-phone-always-downloads-its-artwork.md`), so a closed-gate test needs
/// to close it back explicitly rather than rely on a fresh library's default.
fn close_gate(library: &MusicLibrary) {
    let writer = library.writer().unwrap();
    reprise_core::online_sources::set_enabled(&writer, false).unwrap();
    reprise_core::modules::set_enabled(&writer, &reprise_core::modules::ARTWORK_MODULE, false)
        .unwrap();
}

fn scan_artists(directory: &Path, library: &MusicLibrary, names: &[&str]) {
    let music = directory.join("music");
    std::fs::create_dir(&music).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    for (index, name) in names.iter().enumerate() {
        let track_number = u32::try_from(index).unwrap() + 1;
        let path = music.join(format!("artist-{track_number}.flac"));
        std::fs::copy(&fixture, &path).unwrap();
        reprise_core::library::tag_edit::apply_patch_to_file(
            &path,
            &reprise_core::library::tag_edit::TagPatch {
                title: Some(format!("Track {track_number}")),
                artist: Some((*name).to_owned()),
                album: Some(format!("Album {track_number}")),
                album_artist: Some((*name).to_owned()),
                year: None,
                track_no: Some(Some(track_number)),
                genre: None,
            },
        )
        .unwrap();
    }
    let writer = library.writer().unwrap();
    reprise_core::library::scanner::scan_folder(&writer, &music).unwrap();
}

#[test]
fn android_artwork_sizes_map_to_the_three_measured_rungs() {
    assert_eq!(
        crate::AndroidArtworkSize::List.thumbnail_size().pixels(),
        168
    );
    assert_eq!(
        crate::AndroidArtworkSize::NowPlaying
            .thumbnail_size()
            .pixels(),
        1092
    );
    assert_eq!(
        crate::AndroidArtworkSize::ArtistDetail
            .thumbnail_size()
            .pixels(),
        640
    );
}

#[test]
fn a_portrait_is_never_requested_at_the_now_playing_rung() {
    let directory = tempfile::tempdir().unwrap();
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        move |name, dir| {
            Ok(reprise_core::artist_portrait::PortraitOutcome::Found(
                store_portrait_fixture_in(dir, name),
            ))
        },
    )
    .unwrap();
    open_gate(&library);

    let artist_detail_fetched = library
        .artist_portrait_fetch("Band", crate::AndroidArtworkSize::ArtistDetail)
        .unwrap()
        .unwrap();
    let artist_detail_cached = library
        .artist_portrait_cached("Band", crate::AndroidArtworkSize::ArtistDetail)
        .unwrap();
    let now_playing_cached = library
        .artist_portrait_cached("Band", crate::AndroidArtworkSize::NowPlaying)
        .unwrap();

    assert!(
        artist_detail_fetched.ends_with("-640.png"),
        "got {artist_detail_fetched}"
    );
    assert!(
        artist_detail_cached.ends_with("-640.png"),
        "got {artist_detail_cached}"
    );
    assert!(
        now_playing_cached.ends_with("-1092.png"),
        "got {now_playing_cached}"
    );
}

#[test]
fn portraits_live_under_the_app_cache_root_not_the_xdg_cache() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("cache");
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        cache.to_str().unwrap(),
        |_, _| panic!("locating the portrait directory must not fetch"),
    )
    .unwrap();

    let portrait_dir = library.portrait_dir();

    assert!(portrait_dir.starts_with(&cache));
    assert!(portrait_dir.ends_with("artist-portraits"));
}

#[test]
fn cached_portraits_never_call_the_fetcher_even_with_the_gate_open() {
    let directory = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        move |_, _| {
            counted.fetch_add(1, Ordering::Relaxed);
            Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound)
        },
    )
    .unwrap();
    open_gate(&library);

    assert_eq!(
        library.artist_portrait_cached("Missing", crate::AndroidArtworkSize::List),
        None,
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn cached_portrait_returns_the_reduced_file_not_the_original() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("cache");
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        cache.to_str().unwrap(),
        |_, _| panic!("cached portrait lookup must not fetch"),
    )
    .unwrap();
    let original = store_portrait_fixture_in(&library.portrait_dir(), "Band");

    let reduced = library
        .artist_portrait_cached("Band", crate::AndroidArtworkSize::List)
        .unwrap();

    let reduced = PathBuf::from(reduced);
    assert!(reduced.starts_with(cache.join("reprise/covers")));
    assert!(reduced.to_string_lossy().ends_with("-168.png"));
    assert_ne!(reduced, original);
}

#[test]
fn portrait_cache_io_is_debug_and_does_not_log_the_artist_name() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("cache");
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        cache.to_str().unwrap(),
        |_, _| panic!("cached portrait lookup must not fetch"),
    )
    .unwrap();
    let artist = "Private Artist Io 91f";
    store_portrait_fixture_in(&library.portrait_dir(), artist);
    std::fs::write(cache.join("reprise"), b"not a directory").unwrap();
    let logs = CapturedLogs::default();

    let portrait =
        logs.capture(|| library.artist_portrait_cached(artist, crate::AndroidArtworkSize::List));

    assert_eq!(portrait, None);
    let logged = logs.joined();
    assert!(logged.contains("DEBUG"), "expected debug, got {logged}");
    assert!(!logged.contains("WARN"), "unexpected warning: {logged}");
    assert!(
        logged.contains("cover cache unusable"),
        "expected the failure classification, got {logged}"
    );
    assert!(!logged.contains(artist), "artist leaked into log: {logged}");
}

#[test]
fn portrait_decode_failures_do_not_log_artist_names() {
    let decode_directory = tempfile::tempdir().unwrap();
    let decode_library = MusicLibrary::open_with_portrait_fetch(
        decode_directory.path().to_str().unwrap(),
        decode_directory.path().join("cache").to_str().unwrap(),
        |_, _| panic!("cached portrait lookup must not fetch"),
    )
    .unwrap();
    let decode_artist = "Private Artist Decode 5c2";
    reprise_core::artist_portrait::store_fixture_image(
        &decode_library.portrait_dir(),
        decode_artist,
        b"not an image",
        "png",
    )
    .unwrap();
    let decode_logs = CapturedLogs::default();

    assert_eq!(
        decode_logs.capture(|| decode_library
            .artist_portrait_cached(decode_artist, crate::AndroidArtworkSize::List)),
        None,
    );
    let decode_logged = decode_logs.joined();
    assert!(decode_logged.contains("DEBUG"));
    assert!(decode_logged.contains("image did not decode"));
    assert!(!decode_logged.contains(decode_artist));
}

#[test]
fn portrait_fetch_failures_do_not_log_artist_names() {
    let fetch_directory = tempfile::tempdir().unwrap();
    let fetch_library = MusicLibrary::open_with_portrait_fetch(
        fetch_directory.path().to_str().unwrap(),
        fetch_directory.path().join("cache").to_str().unwrap(),
        |_, _| Err(reprise_core::artist_portrait::PortraitError::InvalidResponse),
    )
    .unwrap();
    open_gate(&fetch_library);
    let fetch_artist = "Private Artist Fetch 8a4";
    let fetch_logs = CapturedLogs::default();

    assert!(matches!(
        fetch_logs
            .capture(|| fetch_library
                .artist_portrait_fetch(fetch_artist, crate::AndroidArtworkSize::List)),
        Ok(None),
    ));
    let fetch_logged = fetch_logs.joined();
    assert!(fetch_logged.contains("DEBUG"));
    assert!(fetch_logged.contains("artist portrait request failed"));
    assert!(!fetch_logged.contains(fetch_artist));
}

#[test]
fn a_portrait_that_was_never_downloaded_is_none() {
    let directory = tempfile::tempdir().unwrap();
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        |_, _| panic!("cached portrait lookup must not fetch"),
    )
    .unwrap();

    assert_eq!(
        library.artist_portrait_cached("Missing", crate::AndroidArtworkSize::List),
        None,
    );
}

#[test]
fn net_1a_a_closed_gate_never_calls_the_fetcher_and_writes_no_file() {
    let directory = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        move |_, _| {
            counted.fetch_add(1, Ordering::Relaxed);
            Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound)
        },
    )
    .unwrap();
    close_gate(&library);

    assert!(matches!(
        library.artist_portrait_fetch("Band", crate::AndroidArtworkSize::List),
        Ok(None),
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(!library.portrait_dir().exists());
}

#[test]
fn net_1a_an_open_gate_calls_the_fetcher_once_and_returns_the_reduced_file() {
    let directory = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        move |name, dir| {
            counted.fetch_add(1, Ordering::Relaxed);
            Ok(reprise_core::artist_portrait::PortraitOutcome::Found(
                store_portrait_fixture_in(dir, name),
            ))
        },
    )
    .unwrap();
    open_gate(&library);

    let path = library
        .artist_portrait_fetch("Band", crate::AndroidArtworkSize::List)
        .unwrap()
        .unwrap();

    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(path.ends_with("-168.png"), "got {path}");
}

#[test]
fn the_query_lock_is_free_while_a_portrait_is_being_fetched() {
    let directory = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release_rx = Arc::new(std::sync::Mutex::new(release_rx));
    let waiting_release = Arc::clone(&release_rx);
    let library = Arc::new(
        MusicLibrary::open_with_portrait_fetch(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
            move |_, _| {
                started_tx.send(()).unwrap();
                waiting_release.lock().unwrap().recv().unwrap();
                Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound)
            },
        )
        .unwrap(),
    );
    open_gate(&library);

    std::thread::scope(|scope| {
        let fetching = Arc::clone(&library);
        let fetch = scope
            .spawn(move || fetching.artist_portrait_fetch("Band", crate::AndroidArtworkSize::List));
        started_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();

        let querying = Arc::clone(&library);
        let (query_tx, query_rx) = std::sync::mpsc::channel();
        scope.spawn(move || query_tx.send(querying.appearance_settings()).unwrap());

        assert!(query_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
            .is_ok());
        release_tx.send(()).unwrap();
        assert!(matches!(fetch.join().unwrap(), Ok(None)));
    });
}

#[test]
fn artists_missing_portraits_skips_those_already_cached() {
    let directory = tempfile::tempdir().unwrap();
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        |_, _| panic!("listing missing portraits must not fetch"),
    )
    .unwrap();
    scan_artists(
        directory.path(),
        &library,
        &["Already Cached", "Needs Portrait"],
    );
    store_portrait_fixture_in(&library.portrait_dir(), "Already Cached");
    open_gate(&library);

    assert_eq!(
        library.artists_missing_portraits(10).unwrap(),
        vec!["Needs Portrait"]
    );
}

#[test]
fn artists_missing_portraits_returns_nothing_when_the_gate_is_closed() {
    let directory = tempfile::tempdir().unwrap();
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        |_, _| panic!("a closed gate must not fetch"),
    )
    .unwrap();
    close_gate(&library);
    scan_artists(directory.path(), &library, &["First", "Second"]);

    assert!(library.artists_missing_portraits(10).unwrap().is_empty());
}

#[test]
fn artists_missing_portraits_respects_its_limit() {
    let directory = tempfile::tempdir().unwrap();
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        |_, _| panic!("listing missing portraits must not fetch"),
    )
    .unwrap();
    scan_artists(directory.path(), &library, &["Alpha", "Beta", "Gamma"]);
    open_gate(&library);

    assert_eq!(
        library.artists_missing_portraits(2).unwrap(),
        vec!["Alpha", "Beta"]
    );
}

#[test]
fn artists_missing_portraits_never_touches_the_network() {
    let directory = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        move |_, _| {
            counted.fetch_add(1, Ordering::Relaxed);
            Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound)
        },
    )
    .unwrap();
    scan_artists(directory.path(), &library, &["Alpha", "Beta"]);
    open_gate(&library);

    assert_eq!(
        library.artists_missing_portraits(10).unwrap(),
        vec!["Alpha", "Beta"]
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn backfill_receives_transport_errors_without_counting_or_marking_them() {
    // Guards the process-global `album_cover` statics this test's own
    // trailing `cancel_artist_portrait_backfill()` reaches, against a
    // concurrent test doing the same under cargo's default parallel test
    // threads (B3 review finding 3).
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        move |_, _| {
            counted.fetch_add(1, Ordering::Relaxed);
            Err(reprise_core::artist_portrait::PortraitError::Fetch(
                reprise_core::musicbrainz::FetchError::Transport,
            ))
        },
    )
    .unwrap();
    scan_artists(directory.path(), &library, &["Offline band"]);
    open_gate(&library);
    let updates = Arc::new(Mutex::new(Vec::new()));

    library.start_artist_portrait_backfill(Box::new(CapturingProgress(Arc::clone(&updates))));
    for _ in 0..2_000 {
        if library.artist_portrait_backfill_progress().state == ArtistPortraitProgressState::Paused
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let progress = library.artist_portrait_backfill_progress();

    assert_eq!(progress.state, ArtistPortraitProgressState::Paused);
    assert_eq!((progress.done, progress.failed, progress.total), (0, 0, 1));
    assert!(calls.load(Ordering::Relaxed) >= 3);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert_ne!(
        reprise_core::artist_portrait::verdict(&library.portrait_dir(), "Offline band", now,),
        reprise_core::artist_portrait::CacheVerdict::FreshNegative,
    );
    assert_eq!(
        updates.lock().unwrap().first().map(|update| update.state),
        Some(ArtistPortraitProgressState::Preparing),
    );
    library.cancel_artist_portrait_backfill();
}

#[test]
fn revoking_artwork_consent_stops_the_worker_before_the_next_artist() {
    // This test asserts `covers_done`/`covers_total` are `0` at the end —
    // a concurrently running cover pass from another test shares the same
    // process-global handle and would contaminate that read (B3 review
    // finding 3).
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let fetch_entered = Arc::clone(&entered);
    let fetch_release = Arc::clone(&release);
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        move |name, dir| {
            counted.fetch_add(1, Ordering::Relaxed);
            let (entered_lock, entered_wake) = &*fetch_entered;
            *entered_lock.lock().unwrap() = true;
            entered_wake.notify_all();
            let (release_lock, release_wake) = &*fetch_release;
            let mut open = release_lock.lock().unwrap();
            while !*open {
                open = release_wake.wait(open).unwrap();
            }
            Ok(reprise_core::artist_portrait::PortraitOutcome::Found(
                store_portrait_fixture_in(dir, name),
            ))
        },
    )
    .unwrap();
    scan_artists(directory.path(), &library, &["First", "Second"]);
    open_gate(&library);
    let updates = Arc::new(Mutex::new(Vec::new()));
    library.start_artist_portrait_backfill(Box::new(CapturingProgress(updates)));
    let (entered_lock, entered_wake) = &*entered;
    let mut did_enter = entered_lock.lock().unwrap();
    while !*did_enter {
        did_enter = entered_wake.wait(did_enter).unwrap();
    }
    drop(did_enter);

    {
        let writer = library.writer().unwrap();
        reprise_core::modules::set_enabled(&writer, &reprise_core::modules::ARTWORK_MODULE, false)
            .unwrap();
    }
    let (release_lock, release_wake) = &*release;
    *release_lock.lock().unwrap() = true;
    release_wake.notify_all();
    for _ in 0..5_000 {
        if library.artist_portrait_backfill_progress().run_id == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        library.artist_portrait_backfill_progress(),
        ArtistPortraitProgressUpdate {
            run_id: 0,
            state: ArtistPortraitProgressState::Complete,
            done: 0,
            failed: 0,
            total: 0,
            covers_done: 0,
            covers_total: 0,
        }
    );
}

/// A `SafSource` reading straight off the real filesystem, for tests that
/// need a configured tree — the cover chain (B3) has no reason to run
/// without one.
struct ChainFsSource;

impl crate::source::SafSource for ChainFsSource {
    fn residence_token(&self, _uri: String) -> Result<Option<i64>, crate::source::SafSourceError> {
        Ok(Some(1))
    }

    fn probe(
        &self,
        uri: String,
        _follow_links: bool,
    ) -> Result<Option<crate::source::SourceFacts>, crate::source::SafSourceError> {
        let path = PathBuf::from(&uri);
        Ok(path.exists().then(|| crate::source::SourceFacts {
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

    fn list_children(
        &self,
        uri: String,
    ) -> Result<Vec<crate::source::SourceChild>, crate::source::SafSourceError> {
        let Ok(entries) = std::fs::read_dir(&uri) else {
            return Ok(Vec::new());
        };
        Ok(entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                Some(crate::source::SourceChild {
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

    fn open_read_fd(&self, uri: String) -> Result<i32, crate::source::SafSourceError> {
        use std::os::fd::IntoRawFd;
        std::fs::File::open(uri)
            .map(IntoRawFd::into_raw_fd)
            .map_err(|error| crate::source::SafSourceError::Io {
                detail: error.to_string(),
            })
    }
}

struct NoopProgressListener;

impl ArtistPortraitProgressListener for NoopProgressListener {
    fn on_progress(&self, _update: ArtistPortraitProgressUpdate) {}
}

/// The chain B3 adds: once the portrait run this rides beside reports
/// `Complete`, the cover pass starts on its own worklist and its progress
/// merges into the same `ArtistPortraitProgressUpdate` Kotlin already reads.
/// The one album here already has local art, so this must never reach the
/// network — proven directly through `start_artist_portrait_backfill_with`'s
/// test seam (B3 review finding 2) rather than assumed from a passing
/// `covers_done == covers_total`, which cannot tell a locally-resolved
/// album from one that silently made a live request.
#[test]
fn the_cover_pass_starts_once_the_portrait_run_completes() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let track_path = music.join("track.flac");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(fixture, &track_path).unwrap();
    reprise_core::library::tag_edit::apply_patch_to_file(
        &track_path,
        &reprise_core::library::tag_edit::TagPatch {
            title: Some("Track One".to_owned()),
            artist: Some("Chain Band".to_owned()),
            album: Some("Chain Album".to_owned()),
            album_artist: Some("Chain Band".to_owned()),
            year: None,
            track_no: Some(Some(1)),
            genre: None,
        },
    )
    .unwrap();
    // Local art: the cover pass must settle this album without a request.
    std::fs::write(music.join("cover.png"), TINY_IMAGE).unwrap();

    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        |_, _| Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound),
    )
    .unwrap();
    open_gate(&library);
    library
        .set_tree_uri(
            music.to_string_lossy().into_owned(),
            Box::new(ChainFsSource),
        )
        .unwrap();
    let writer = library.writer().unwrap();
    reprise_core::library::scanner::scan_folder(&writer, &music).unwrap();
    drop(writer);

    let network_fetch_calls = Arc::new(AtomicUsize::new(0));
    let counted_network_fetch = Arc::clone(&network_fetch_calls);
    library.start_artist_portrait_backfill_with(
        Box::new(NoopProgressListener),
        Arc::new(move |_, _, _| {
            counted_network_fetch.fetch_add(1, Ordering::Relaxed);
            CoverFetchOutcome::NotFound
        }),
    );

    let mut last = library.artist_portrait_backfill_progress();
    for _ in 0..5_000 {
        last = library.artist_portrait_backfill_progress();
        if last.covers_total > 0 && last.covers_done == last.covers_total {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    assert_eq!(last.covers_total, 1, "one album, one representative track");
    assert_eq!(
        last.covers_done, 1,
        "local art settles the album without a request"
    );
    assert_eq!(
        network_fetch_calls.load(Ordering::Relaxed),
        0,
        "a locally-resolved album must never reach the network fetch",
    );
}

/// The live push in `forward` (as opposed to the pure `merged_progress_update`
/// it is built on, exercised separately below) must honour the same
/// invariant: never `Complete` while a chained cover pass has not yet
/// reported any progress of its own (B3 review finding 1). Before the fix
/// this call site pushed the raw portrait `Complete`/`covers=0/0` update
/// unconditionally, ahead of ever starting the cover pass that immediately
/// revokes it.
#[test]
fn the_forward_closure_never_pushes_complete_before_the_chained_cover_pass_reports_progress() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let track_path = music.join("track.flac");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(fixture, &track_path).unwrap();
    reprise_core::library::tag_edit::apply_patch_to_file(
        &track_path,
        &reprise_core::library::tag_edit::TagPatch {
            title: Some("Track One".to_owned()),
            artist: Some("Push Order Band".to_owned()),
            album: Some("Push Order Album".to_owned()),
            album_artist: Some("Push Order Band".to_owned()),
            year: None,
            track_no: Some(Some(1)),
            genre: None,
        },
    )
    .unwrap();
    std::fs::write(music.join("cover.png"), TINY_IMAGE).unwrap();

    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        |_, _| Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound),
    )
    .unwrap();
    open_gate(&library);
    library
        .set_tree_uri(
            music.to_string_lossy().into_owned(),
            Box::new(ChainFsSource),
        )
        .unwrap();
    let writer = library.writer().unwrap();
    reprise_core::library::scanner::scan_folder(&writer, &music).unwrap();
    drop(writer);

    let updates = Arc::new(Mutex::new(Vec::new()));
    library.start_artist_portrait_backfill(Box::new(CapturingProgress(Arc::clone(&updates))));

    // Polled off the captured log itself, not `artist_portrait_backfill_progress()`:
    // that getter reads the shared state directly, which the worker writes
    // *before* it calls the listener, so polling it and then reading
    // `updates` is its own TOCTOU race under load — this test's assertion
    // only cares what the listener actually saw and in what order.
    for _ in 0..5_000 {
        let settled = updates
            .lock()
            .unwrap()
            .iter()
            .any(|update| update.covers_total > 0 && update.covers_done == update.covers_total);
        if settled {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let captured = updates.lock().unwrap().clone();
    let first_cover_update = captured.iter().position(|update| update.covers_total > 0);
    assert!(
        first_cover_update.is_some(),
        "the chain must report cover progress at all: {captured:?}"
    );
    let first_cover_update = first_cover_update.unwrap();
    assert!(
        captured[..first_cover_update]
            .iter()
            .all(|update| update.state != ArtistPortraitProgressState::Complete),
        "a Complete update arrived before the chained cover pass reported \
         any progress, a bar that finished and then ran backwards: {captured:?}"
    );
}

/// A finished portrait run reports `Complete` — until the cover pass that
/// rides beside it still has albums left, at which point the merged update
/// must not look finished: a `Complete` bar with a `total` that keeps
/// growing reads as a counter running backwards.
#[test]
fn a_portrait_completion_does_not_report_complete_while_covers_are_still_running() {
    let portrait = reprise_core::artist_portrait::PortraitBackfillProgress {
        run_id: 1,
        state: PortraitBackfillState::Complete,
        done: 200,
        failed: 0,
        total: 200,
    };

    let mid_run = merged_progress_update(
        portrait,
        CoverBackfillProgress {
            done: 0,
            total: 500,
        },
    );
    assert_eq!(mid_run.state, ArtistPortraitProgressState::Running);

    let covers_done_too = merged_progress_update(
        portrait,
        CoverBackfillProgress {
            done: 500,
            total: 500,
        },
    );
    assert_eq!(covers_done_too.state, ArtistPortraitProgressState::Complete);

    let no_covers_at_all = merged_progress_update(portrait, CoverBackfillProgress::default());
    assert_eq!(
        no_covers_at_all.state,
        ArtistPortraitProgressState::Complete
    );
}
