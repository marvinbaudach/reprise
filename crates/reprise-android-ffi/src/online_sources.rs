use reprise_core::{modules, online_sources};

use crate::{LibraryError, MusicLibrary};

#[uniffi::export]
impl MusicLibrary {
    pub fn online_sources_enabled(&self) -> Result<bool, LibraryError> {
        let state = self.lock()?;
        online_sources::network_allowed(&state.db, &modules::ARTWORK_MODULE).map_err(|error| {
            LibraryError::Database {
                detail: error.to_string(),
            }
        })
    }

    pub fn set_online_sources_enabled(&self, value: bool) -> Result<(), LibraryError> {
        let state = self.lock()?;
        online_sources::set_enabled(&state.db, value).map_err(|error| LibraryError::Database {
            detail: error.to_string(),
        })?;
        modules::set_enabled(&state.db, &modules::ARTWORK_MODULE, value).map_err(|error| {
            LibraryError::Database {
                detail: error.to_string(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    #[test]
    fn the_switch_is_off_on_a_fresh_database() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();

        assert!(!library.online_sources_enabled().unwrap());
    }

    #[test]
    fn switching_on_survives_the_first_enable_seed() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();

        library.set_online_sources_enabled(true).unwrap();

        assert!(library.online_sources_enabled().unwrap());
        let state = library.lock().unwrap();
        assert!(reprise_core::online_sources::is_enabled(&state.db).unwrap());
        assert!(reprise_core::modules::is_enabled(
            &state.db,
            &reprise_core::modules::ARTWORK_MODULE,
        )
        .unwrap());
    }

    #[test]
    fn switching_off_closes_the_gate_for_fetches() {
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

        library.set_online_sources_enabled(true).unwrap();
        library
            .artist_portrait_fetch("Band", crate::AndroidArtworkSize::List)
            .unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        library.set_online_sources_enabled(false).unwrap();
        library
            .artist_portrait_fetch("Band", crate::AndroidArtworkSize::List)
            .unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn an_off_and_on_cycle_leaves_the_switch_on() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();

        library.set_online_sources_enabled(true).unwrap();
        library.set_online_sources_enabled(false).unwrap();
        library.set_online_sources_enabled(true).unwrap();

        assert!(library.online_sources_enabled().unwrap());
    }
}
