use reprise_core::db::Db;
use reprise_core::{modules, online_sources};

/// Opens the artwork network gate on every `MusicLibrary::open`: on Android
/// the artist-portrait and album-cover download has no switch, unlike the
/// desktop's consent wizard (`the-phone-always-downloads-its-artwork.md`).
/// Core's own default stays off — this is a platform decision the shared
/// code does not have, so it lives at this FFI boundary instead.
///
/// Read before write: `open` runs on every process start, and a database
/// that already has the gate on must not be written again.
pub(crate) fn open_artwork_gate(writer: &Db) -> Result<(), rusqlite::Error> {
    if online_sources::is_enabled(writer)? && modules::is_enabled(writer, &modules::ARTWORK_MODULE)?
    {
        return Ok(());
    }
    online_sources::set_enabled(writer, true)?;
    modules::set_enabled(writer, &modules::ARTWORK_MODULE, true)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use reprise_core::library::settings;

    use super::*;
    use crate::MusicLibrary;

    #[test]
    fn a_fresh_database_opens_with_the_artwork_gate_on() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();

        let reader = library.reader().unwrap();
        assert!(online_sources::network_allowed(&reader, &modules::ARTWORK_MODULE).unwrap());
        assert!(settings::get_setting(
            &reader,
            settings::ONLINE_SOURCES_FIRST_ENABLE_COMPLETED_KEY
        )
        .unwrap()
        .is_some());
    }

    #[test]
    fn a_stored_off_is_overridden_on_the_next_open() {
        let directory = tempfile::tempdir().unwrap();
        let private = directory.path().to_str().unwrap().to_owned();
        let cache = directory.path().join("cache").to_str().unwrap().to_owned();

        {
            let library = MusicLibrary::open(&private, &cache).unwrap();
            let writer = library.writer().unwrap();
            online_sources::set_enabled(&writer, false).unwrap();
            modules::set_enabled(&writer, &modules::ARTWORK_MODULE, false).unwrap();
        }

        let library = MusicLibrary::open(&private, &cache).unwrap();

        let reader = library.reader().unwrap();
        assert!(online_sources::network_allowed(&reader, &modules::ARTWORK_MODULE).unwrap());
    }

    #[test]
    fn the_gate_is_open_for_fetches() {
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

        library
            .artist_portrait_fetch("Band", crate::AndroidArtworkSize::List)
            .unwrap();

        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}
