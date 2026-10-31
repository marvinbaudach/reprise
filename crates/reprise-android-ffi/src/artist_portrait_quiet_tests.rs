use std::collections::HashMap;
use std::fs::File;
use std::os::fd::IntoRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;

const FIXTURE_IMAGE: &[u8] = b"fixture";

#[derive(Default)]
struct SafCalls(Mutex<HashMap<String, usize>>);

impl SafCalls {
    fn record(&self, uri: &str) {
        *self.0.lock().unwrap().entry(uri.to_owned()).or_default() += 1;
    }

    fn count_for(&self, uri: &str) -> usize {
        *self.0.lock().unwrap().get(uri).unwrap_or(&0)
    }
}

struct CountingTree(Arc<SafCalls>);

impl crate::source::SafSource for CountingTree {
    fn residence_token(&self, uri: String) -> Result<Option<i64>, crate::source::SafSourceError> {
        self.0.record(&uri);
        Ok(Some(1))
    }

    fn probe(
        &self,
        uri: String,
        _follow_links: bool,
    ) -> Result<Option<crate::source::SourceFacts>, crate::source::SafSourceError> {
        self.0.record(&uri);
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
        self.0.record(&uri);
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
        self.0.record(&uri);
        File::open(uri)
            .map(IntoRawFd::into_raw_fd)
            .map_err(|error| crate::source::SafSourceError::Io {
                detail: error.to_string(),
            })
    }
}

struct Updates(Arc<Mutex<Vec<ArtistPortraitProgressUpdate>>>);

impl ArtistPortraitProgressListener for Updates {
    fn on_progress(&self, update: ArtistPortraitProgressUpdate) {
        self.0.lock().unwrap().push(update);
    }
}

fn visible(update: ArtistPortraitProgressUpdate) -> bool {
    update.run_id != 0
        && !(update.state == ArtistPortraitProgressState::Complete && update.total == 0)
}

#[test]
fn a_second_start_after_a_settled_run_shows_nothing() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let root = tempfile::tempdir().unwrap();
    let music = root.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    let albums = [
        ("Found artist", "Cached album"),
        ("Missing artist", "Marked album"),
        ("Dropped artist", "Local album"),
        ("Other artist", "Needs album"),
    ];
    let mut track_paths = HashMap::new();
    for (index, (artist, album)) in albums.iter().enumerate() {
        let directory = music.join(format!("album-{index}"));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("track.flac");
        std::fs::copy(&fixture, &path).unwrap();
        reprise_core::library::tag_edit::apply_patch_to_file(
            &path,
            &reprise_core::library::tag_edit::TagPatch {
                title: Some(format!("Track {index}")),
                artist: Some((*artist).to_owned()),
                album: Some((*album).to_owned()),
                album_artist: Some((*artist).to_owned()),
                year: None,
                track_no: Some(Some(1)),
                genre: None,
            },
        )
        .unwrap();
        track_paths.insert(*artist, path);
    }
    std::fs::write(music.join("album-2/cover.png"), FIXTURE_IMAGE).unwrap();

    let cache_root = root.path().join("cache");
    let portrait_fetches = Arc::new(AtomicUsize::new(0));
    let counted_portraits = Arc::clone(&portrait_fetches);
    let library = MusicLibrary::open_with_portrait_fetch(
        root.path().to_str().unwrap(),
        cache_root.to_str().unwrap(),
        move |artist, cache_dir| {
            counted_portraits.fetch_add(1, Ordering::Relaxed);
            match artist {
                "Found artist" | "Other artist" => {
                    let path = reprise_core::artist_portrait::store_fixture_image(
                        cache_dir,
                        artist,
                        FIXTURE_IMAGE,
                        "png",
                    );
                    path.map(PortraitOutcome::Found)
                        .ok_or(reprise_core::artist_portrait::PortraitError::InvalidResponse)
                }
                "Missing artist" => Ok(PortraitOutcome::NotFound),
                "Dropped artist" => {
                    Err(reprise_core::artist_portrait::PortraitError::InvalidResponse)
                }
                _ => Ok(PortraitOutcome::NotFound),
            }
        },
    )
    .unwrap();
    let writer = library.writer().unwrap();
    reprise_core::online_sources::set_enabled(&writer, true).unwrap();
    reprise_core::modules::set_enabled(&writer, &reprise_core::modules::ARTWORK_MODULE, true)
        .unwrap();
    reprise_core::library::scanner::scan_folder(&writer, &music).unwrap();
    drop(writer);

    let portrait_dir = library.portrait_dir();
    reprise_core::artist_portrait::store_fixture_image(
        &portrait_dir,
        "Found artist",
        FIXTURE_IMAGE,
        "png",
    )
    .unwrap();
    let downloaded = reprise_core::cover_download::downloaded_dir_in(&cache_root);
    std::fs::create_dir_all(&downloaded).unwrap();
    let cached_key = reprise_core::cover_download::album_key("Found artist", "Cached album");
    std::fs::write(downloaded.join(format!("{cached_key}.png")), FIXTURE_IMAGE).unwrap();
    let missing_key = reprise_core::cover_download::album_key("Missing artist", "Marked album");
    std::fs::write(downloaded.join(format!("{missing_key}.notfound3")), b"").unwrap();
    let local_key = reprise_core::cover_download::album_key("Dropped artist", "Local album");
    std::fs::write(downloaded.join(format!("{local_key}.local")), b"").unwrap();

    let saf_calls = Arc::new(SafCalls::default());
    library
        .set_tree_uri(
            music.to_string_lossy().into_owned(),
            Box::new(CountingTree(Arc::clone(&saf_calls))),
        )
        .unwrap();
    let updates = Arc::new(Mutex::new(Vec::new()));
    let network_fetches = Arc::new(AtomicUsize::new(0));
    let counted_network = Arc::clone(&network_fetches);
    let network_fetch: Arc<NetworkCoverFetch> = Arc::new(move |_, _, _| {
        counted_network.fetch_add(1, Ordering::Relaxed);
        CoverFetchOutcome::NotFound
    });
    let start = || {
        library.start_artist_portrait_backfill_with(
            Box::new(Updates(Arc::clone(&updates))),
            Arc::clone(&network_fetch),
        );
    };
    start();
    wait_for_update(&updates, 0, |update| {
        update.state == ArtistPortraitProgressState::Complete
            && update.covers_total == 1
            && update.covers_done == 1
    });
    std::thread::sleep(Duration::from_millis(20));

    // The injected network seam stands in for fetch_and_cache_in's persistent miss write.
    let needs_key = reprise_core::cover_download::album_key("Other artist", "Needs album");
    std::fs::write(downloaded.join(format!("{needs_key}.notfound3")), b"").unwrap();

    assert_eq!(
        saf_calls.count_for(&track_paths["Dropped artist"].to_string_lossy()),
        0
    );
    let first_saf_calls = saf_calls.0.lock().unwrap().clone();
    let portrait_count = portrait_fetches.load(Ordering::Relaxed);
    let network_count = network_fetches.load(Ordering::Relaxed);
    let before_second = updates.lock().unwrap().len();
    start();
    assert!(!visible(library.artist_portrait_backfill_progress()));
    wait_for_update(&updates, before_second, |update| {
        update.state == ArtistPortraitProgressState::Complete
    });

    let captured = updates.lock().unwrap();
    let second_updates = &captured[before_second..];
    assert!(
        second_updates
            .iter()
            .copied()
            .all(|update| !visible(update)),
        "second run published visible updates: {second_updates:?}"
    );
    assert_eq!(portrait_fetches.load(Ordering::Relaxed), portrait_count);
    assert_eq!(network_fetches.load(Ordering::Relaxed), network_count);
    assert_eq!(*saf_calls.0.lock().unwrap(), first_saf_calls);
    assert!(!visible(library.artist_portrait_backfill_progress()));
}

fn wait_for_update(
    updates: &Arc<Mutex<Vec<ArtistPortraitProgressUpdate>>>,
    start: usize,
    matches: impl Fn(ArtistPortraitProgressUpdate) -> bool,
) {
    for _ in 0..5_000 {
        if updates
            .lock()
            .unwrap()
            .get(start..)
            .is_some_and(|updates| updates.iter().copied().any(&matches))
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!(
        "portrait and cover backfill did not settle: {:?}",
        updates.lock().unwrap()
    );
}
