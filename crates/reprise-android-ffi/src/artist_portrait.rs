use std::path::{Path, PathBuf};

use reprise_core::artist_portrait::{load_cached_from, PortraitOutcome};
use reprise_core::cover::{self, CoverSource};
use reprise_core::library::source::UnixLibrarySource;

use crate::{AndroidArtworkSize, MusicLibrary};

impl MusicLibrary {
    pub(crate) fn portrait_dir(&self) -> PathBuf {
        self.cache_root.join("artist-portraits")
    }

    fn reduced_portrait_path(
        &self,
        artist: &str,
        path: &Path,
        size: AndroidArtworkSize,
    ) -> Option<String> {
        match cover::thumbnail_with_source(
            &UnixLibrarySource,
            &CoverSource::FolderImage(path.to_owned()),
            size.thumbnail_size(),
            &self.cache_root,
        ) {
            Ok(path) => Some(path.to_string_lossy().into_owned()),
            Err(error @ cover::CoverError::Io(_)) => {
                tracing::warn!(%error, %artist, "no artist portrait: cover cache unusable");
                None
            }
            Err(error) => {
                tracing::debug!(%error, %artist, "no artist portrait: image did not decode");
                None
            }
        }
    }
}

#[uniffi::export]
impl MusicLibrary {
    pub fn artist_portrait_cached(&self, name: &str, size: AndroidArtworkSize) -> Option<String> {
        match load_cached_from(name, &self.portrait_dir()) {
            PortraitOutcome::Found(path) => self.reduced_portrait_path(name, &path, size),
            PortraitOutcome::NotFound => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    const TINY_IMAGE: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5,
        0x1c, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64,
        0xf8, 0x0f, 0x00, 0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xe3, 0x66, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn store_portrait_fixture(library: &MusicLibrary, name: &str) -> PathBuf {
        let normalized = name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        normalized.as_bytes().hash(&mut hasher);
        let path = library
            .portrait_dir()
            .join(format!("{:016x}.png", hasher.finish()));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, TINY_IMAGE).unwrap();
        path
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
        {
            let state = library.lock().unwrap();
            reprise_core::online_sources::set_enabled(&state.db, true).unwrap();
            reprise_core::modules::set_enabled(
                &state.db,
                &reprise_core::modules::ARTWORK_MODULE,
                true,
            )
            .unwrap();
        }

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
        let original = store_portrait_fixture(&library, "Band");

        let reduced = library
            .artist_portrait_cached("Band", crate::AndroidArtworkSize::List)
            .unwrap();

        let reduced = PathBuf::from(reduced);
        assert!(reduced.starts_with(cache.join("reprise/covers")));
        assert!(reduced.to_string_lossy().ends_with("-168.png"));
        assert_ne!(reduced, original);
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
}
