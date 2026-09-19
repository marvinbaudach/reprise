use reprise_core::db::Db;
use reprise_core::{modules, online_sources};

use crate::LibraryError;

/// Opens the artwork network gate on every `MusicLibrary::open`: on Android
/// the artist-portrait and album-cover download has no switch, unlike the
/// desktop's consent wizard (`the-phone-always-downloads-its-artwork.md`).
/// Core's own default stays off — this is a platform decision the shared
/// code does not have, so it lives at this FFI boundary instead.
///
/// This reaches through `online_sources::set_enabled`, the app-wide network
/// gate shared by every module, not an artwork-only switch. On a database
/// where the gate has never been turned on, that call also runs core's
/// one-shot first-enable seed, which writes `module.radio.enabled = true`
/// (`RADIO_MODULE.default_enabled` is `true`) alongside the other modules it
/// decides for. Accepted, not a bug: Android has no per-module onboarding of
/// its own, and forcing Radio off here would be a behaviour change this FFI
/// boundary does not own.
///
/// Read before write: `open` runs on every process start, and a database
/// that already has the gate on must not be written again.
pub(crate) fn open_artwork_gate(writer: &Db) -> Result<(), LibraryError> {
    if online_sources::is_enabled(writer).map_err(database_error)?
        && modules::is_enabled(writer, &modules::ARTWORK_MODULE).map_err(database_error)?
    {
        return Ok(());
    }
    online_sources::set_enabled(writer, true).map_err(database_error)?;
    modules::set_enabled(writer, &modules::ARTWORK_MODULE, true).map_err(database_error)
}

fn database_error(error: impl std::fmt::Display) -> LibraryError {
    LibraryError::Database {
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use reprise_core::library::settings;

    use super::*;
    use crate::MusicLibrary;

    #[test]
    fn net_4c_a_fresh_database_opens_with_the_artwork_gate_on() {
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
    fn a_fresh_database_pins_every_module_state_the_seed_leaves() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();

        let reader = library.reader().unwrap();

        // The gate itself, and the one module this FFI boundary decides for
        // explicitly.
        assert!(online_sources::is_enabled(&reader).unwrap());
        assert!(modules::is_enabled(&reader, &modules::ARTWORK_MODULE).unwrap());

        // Core's one-shot first-enable seed (`online_sources::set_enabled`)
        // runs once behind that write and leaves its own row for Radio —
        // `module.radio.enabled`, hard-coded here rather than reaching for
        // `modules::enabled_key`, which is `pub(crate)` to core. Presence,
        // not a literal encoding this test does not own.
        assert!(settings::get_setting(&reader, "module.radio.enabled")
            .unwrap()
            .is_some());

        // Every module's effective state after a fresh `open`, read through
        // `modules::is_enabled` — the way a real caller reads it — so a
        // future change to the seed list or to a `ModuleDescriptor::
        // default_enabled` goes red here instead of surfacing as an
        // undocumented app-wide side effect.
        let expected = [
            (&modules::SONG_VISUALS_MODULE, true),
            (&modules::LIBRARY_DOCTOR_MODULE, true),
            (&modules::NEW_RELEASES_MODULE, false),
            (&modules::CONCERTS_MODULE, false),
            (&modules::PODCASTS_MODULE, false),
            (&modules::YOUTUBE_MODULE, false),
            (&modules::RADIO_MODULE, true),
            (&modules::ARTWORK_MODULE, true),
            (&modules::ONLINE_LYRICS_MODULE, false),
            (&modules::LISTENBRAINZ_MODULE, false),
            (&modules::LASTFM_MODULE, false),
        ];
        for (module, expected_enabled) in expected {
            assert_eq!(
                modules::is_enabled(&reader, module).unwrap(),
                expected_enabled,
                "module {} state after a fresh open",
                module.id
            );
        }
    }

    #[test]
    fn net_4c_a_stored_off_is_overridden_on_the_next_open() {
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
