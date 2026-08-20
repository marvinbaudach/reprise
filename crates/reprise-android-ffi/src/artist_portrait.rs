use std::path::{Path, PathBuf};

use reprise_core::artist_portrait::{load_cached_from, verdict, PortraitOutcome};
use reprise_core::cover::{self, CoverSource};
use reprise_core::library::source::UnixLibrarySource;

use crate::{AndroidArtworkSize, MusicLibrary};

impl MusicLibrary {
    pub(crate) fn portrait_dir(&self) -> PathBuf {
        self.cache_root.join("artist-portraits")
    }

    fn reduced_portrait_path(&self, path: &Path, size: AndroidArtworkSize) -> Option<String> {
        match cover::thumbnail_with_source(
            &UnixLibrarySource,
            &CoverSource::FolderImage(path.to_owned()),
            size.thumbnail_size(),
            &self.cache_root,
        ) {
            Ok(path) => Some(path.to_string_lossy().into_owned()),
            Err(error @ cover::CoverError::Io(_)) => {
                tracing::debug!(%error, "no artist portrait: cover cache unusable");
                None
            }
            Err(error) => {
                tracing::debug!(%error, "no artist portrait: image did not decode");
                None
            }
        }
    }
}

#[uniffi::export]
impl MusicLibrary {
    pub fn artists_missing_portraits(
        &self,
        limit: u32,
    ) -> Result<Vec<String>, crate::LibraryError> {
        let allowed = {
            let reader = self.reader()?;
            reprise_core::online_sources::network_allowed_or_off(
                &reader,
                &reprise_core::modules::ARTWORK_MODULE,
            )
        };
        if !allowed {
            return Ok(Vec::new());
        }

        let wanted = limit as usize;
        if wanted == 0 {
            return Ok(Vec::new());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() as i64);
        let portrait_dir = self.portrait_dir();
        let mut missing = Vec::new();
        let mut offset = 0;
        loop {
            let window = {
                let reader = self.reader()?;
                reprise_core::queries::query_artists(
                    &reader,
                    "",
                    reprise_core::queries::WindowRange {
                        offset,
                        limit: i64::MAX,
                    },
                )
            }
            .map_err(|error| crate::LibraryError::Query {
                detail: error.to_string(),
            })?;
            let returned = window.rows.len();
            for artist in window.rows {
                if verdict(&portrait_dir, &artist.artist, now).needs_fetch() {
                    missing.push(artist.artist);
                    if missing.len() == wanted {
                        return Ok(missing);
                    }
                }
            }
            if !window.has_more || returned == 0 {
                return Ok(missing);
            }
            offset = offset.saturating_add(i64::try_from(returned).unwrap_or(i64::MAX));
        }
    }
}

#[uniffi::export]
impl MusicLibrary {
    pub fn artist_portrait_cached(&self, name: &str, size: AndroidArtworkSize) -> Option<String> {
        match load_cached_from(name, &self.portrait_dir()) {
            PortraitOutcome::Found(path) => self.reduced_portrait_path(&path, size),
            PortraitOutcome::NotFound => None,
        }
    }

    pub fn artist_portrait_fetch(
        &self,
        name: &str,
        size: AndroidArtworkSize,
    ) -> Result<Option<String>, crate::LibraryError> {
        let allowed = {
            let reader = self.reader()?;
            reprise_core::online_sources::network_allowed_or_off(
                &reader,
                &reprise_core::modules::ARTWORK_MODULE,
            )
        };
        if !allowed {
            return Ok(None);
        }

        match (self.portrait_fetch)(name, &self.portrait_dir()) {
            Ok(PortraitOutcome::Found(path)) => Ok(self.reduced_portrait_path(&path, size)),
            Ok(PortraitOutcome::NotFound) => Ok(None),
            Err(error) => {
                tracing::debug!(%error, "artist portrait request failed");
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;
    use crate::log_capture::CapturedLogs;

    const TINY_IMAGE: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5,
        0x1c, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64,
        0xf8, 0x0f, 0x00, 0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
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

        let portrait = logs
            .capture(|| library.artist_portrait_cached(artist, crate::AndroidArtworkSize::List));

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
            fetch_logs.capture(|| fetch_library
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
            let fetch = scope.spawn(move || {
                fetching.artist_portrait_fetch("Band", crate::AndroidArtworkSize::List)
            });
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
    fn artists_missing_portraits_returns_nothing_when_the_switch_is_off() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open_with_portrait_fetch(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
            |_, _| panic!("a closed gate must not fetch"),
        )
        .unwrap();
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
}
