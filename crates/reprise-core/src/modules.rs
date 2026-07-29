//! Registry metadata for optional features shown on the Plugins page.
//!
//! Core player capabilities such as Equalizer and ReplayGain deliberately do
//! not belong here. Device support likewise belongs to Synchronization. This
//! registry is reserved for optional integrations and deliberately opt-in
//! experiences such as local song visuals.

use rusqlite::Connection;

use crate::library::settings;

pub struct ModuleDescriptor {
    /// Stable machine id; forms the settings key `module.<id>.enabled`.
    pub id: &'static str,
    /// UI display name (the Plugins list, spec stage 5).
    pub name: &'static str,
    pub description: &'static str,
    /// Flag value when the settings table has no row for this module.
    pub default_enabled: bool,
    /// Whether changing the setting affects the running application immediately.
    pub applies_live: bool,
}

pub const MPRIS_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "mpris",
    name: "MPRIS",
    description: "GNOME media controls, media keys, and lock-screen integration (D-Bus)",
    default_enabled: true,
    applies_live: false,
};

pub const LISTENBRAINZ_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "listenbrainz",
    name: "ListenBrainz",
    description: "Scrobble completed listens to ListenBrainz (network; off by default)",
    default_enabled: false,
    applies_live: true,
};

pub const LASTFM_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "lastfm",
    name: "Last.fm",
    description: "Scrobble completed listens to Last.fm (network; off by default)",
    default_enabled: false,
    applies_live: true,
};

pub const NEW_RELEASES_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "new_releases",
    name: "New Releases",
    description: "Show upcoming and newly released albums; contacts MusicBrainz",
    default_enabled: false,
    applies_live: true,
};

pub const CONCERTS_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "concerts",
    name: "Concerts",
    description: "Show upcoming concerts for library artists; contacts event providers",
    default_enabled: false,
    applies_live: true,
};

pub const PODCASTS_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "podcasts",
    name: "Podcasts",
    description: "Subscribe to podcast RSS feeds; search via Apple Podcasts",
    default_enabled: false,
    applies_live: true,
};

/// YouTube is a peer of Podcasts and Radio, not a sub-setting of Podcasts
/// (issue #96): "Podcasts off + YouTube on" must be a representable state,
/// which requires its own module flag rather than a boolean nested inside
/// `podcasts::config`.
pub const YOUTUBE_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "youtube",
    name: "YouTube",
    description: "Subscribe to YouTube channels; audio via yt-dlp",
    default_enabled: false,
    applies_live: true,
};

pub const RADIO_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "radio",
    name: "Radio",
    description: "Play favorite stations; each play reports a click to radio-browser.info",
    default_enabled: true,
    applies_live: true,
};

pub const LIBRARY_DOCTOR_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "library_doctor",
    name: "Library Doctor",
    description: "Review local tag cleanup suggestions; optional remote suggestions; contacts MusicBrainz / AcoustID",
    default_enabled: true,
    applies_live: true,
};

pub const COVER_DOWNLOAD_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "cover_download",
    name: "Cover Download",
    description: "Download missing album covers from online services",
    default_enabled: false,
    applies_live: true,
};

pub const ARTIST_PORTRAITS_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "artist_portraits",
    name: "Artist Portraits",
    description: "Download artist portraits from online services",
    default_enabled: false,
    applies_live: true,
};

pub const ONLINE_LYRICS_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "online_lyrics",
    name: "Online Lyrics",
    description: "Load missing lyrics from an online service",
    default_enabled: false,
    applies_live: true,
};

/// `C1`/`SRC-11`: channel, show and station artwork (YouTube thumbnails,
/// iTunes `artworkUrl600`, radio-browser favicons) for the podcast, YouTube
/// and radio library views and their Add dialogs.
pub const SOURCE_IMAGES_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "source_images",
    name: "Source Images",
    description: "Download channel, show, and station artwork for Podcasts, YouTube, and Radio",
    default_enabled: false,
    applies_live: true,
};

pub const SONG_VISUALS_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "song_visuals",
    name: "Song Visuals",
    description: "Show local audio-reactive visuals in Now Playing",
    default_enabled: false,
    applies_live: true,
};

/// Every optional integration the app currently exposes, in Plugins-page order.
pub const ALL_MODULES: &[&ModuleDescriptor] = &[
    &SONG_VISUALS_MODULE,
    &LIBRARY_DOCTOR_MODULE,
    &NEW_RELEASES_MODULE,
    &CONCERTS_MODULE,
    &PODCASTS_MODULE,
    &YOUTUBE_MODULE,
    &RADIO_MODULE,
    &COVER_DOWNLOAD_MODULE,
    &ARTIST_PORTRAITS_MODULE,
    &ONLINE_LYRICS_MODULE,
    &SOURCE_IMAGES_MODULE,
    &LISTENBRAINZ_MODULE,
    &LASTFM_MODULE,
];

pub(crate) fn enabled_key(module: &ModuleDescriptor) -> String {
    format!("module.{}.enabled", module.id)
}

pub fn is_enabled(conn: &Connection, module: &ModuleDescriptor) -> Result<bool, rusqlite::Error> {
    settings::get_bool(conn, &enabled_key(module), module.default_enabled)
}

pub fn set_enabled(
    conn: &Connection,
    module: &ModuleDescriptor,
    value: bool,
) -> Result<(), rusqlite::Error> {
    settings::set_bool(conn, &enabled_key(module), value)
}

#[cfg(test)]
mod tests {
    use super::*;

    use rusqlite::Connection;

    fn migrated_conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn modules_default_to_their_declared_default() {
        let conn = migrated_conn();
        assert!(is_enabled(&conn, &MPRIS_MODULE).unwrap()); // default_enabled: true
    }

    #[test]
    fn set_enabled_persists_and_round_trips() {
        let conn = migrated_conn();
        set_enabled(&conn, &MPRIS_MODULE, false).unwrap();
        assert!(!is_enabled(&conn, &MPRIS_MODULE).unwrap());
        set_enabled(&conn, &MPRIS_MODULE, true).unwrap();
        assert!(is_enabled(&conn, &MPRIS_MODULE).unwrap());
    }

    #[test]
    fn enabled_key_is_namespaced_per_module() {
        assert_eq!(enabled_key(&MPRIS_MODULE), "module.mpris.enabled");
    }

    #[test]
    fn all_modules_excludes_mpris_which_is_always_on() {
        // MPRIS is unconditional (no user toggle), so it is not a Plugins-page
        // module even though MPRIS_MODULE still describes it.
        assert!(!ALL_MODULES.iter().any(|m| m.id == "mpris"));
    }

    #[test]
    fn all_modules_includes_opt_in_cover_download() {
        assert!(ALL_MODULES
            .iter()
            .any(|module| module.id == "cover_download"));
    }

    #[test]
    fn listenbrainz_defaults_to_disabled_and_has_a_namespaced_key() {
        let conn = migrated_conn();
        assert!(!is_enabled(&conn, &LISTENBRAINZ_MODULE).unwrap());
        assert_eq!(
            enabled_key(&LISTENBRAINZ_MODULE),
            "module.listenbrainz.enabled"
        );
    }

    #[test]
    fn all_modules_lists_listenbrainz_once() {
        assert_eq!(
            ALL_MODULES
                .iter()
                .filter(|module| module.id == "listenbrainz")
                .count(),
            1
        );
    }

    #[test]
    fn listenbrainz_enabled_state_round_trips() {
        let conn = migrated_conn();
        set_enabled(&conn, &LISTENBRAINZ_MODULE, true).unwrap();
        assert!(is_enabled(&conn, &LISTENBRAINZ_MODULE).unwrap());
        set_enabled(&conn, &LISTENBRAINZ_MODULE, false).unwrap();
        assert!(!is_enabled(&conn, &LISTENBRAINZ_MODULE).unwrap());
    }

    #[test]
    fn nr_7_new_releases_is_listed_and_defaults_to_disabled() {
        let conn = migrated_conn();
        assert!(ALL_MODULES
            .iter()
            .any(|module| module.id == NEW_RELEASES_MODULE.id));
        assert_eq!(NEW_RELEASES_MODULE.id, "new_releases");
        assert_eq!(NEW_RELEASES_MODULE.name, "New Releases");
        assert!(!is_enabled(&conn, &NEW_RELEASES_MODULE).unwrap());
    }

    #[test]
    fn concerts_is_a_live_opt_in_module() {
        let conn = migrated_conn();
        let descriptor = ALL_MODULES
            .iter()
            .copied()
            .find(|module| module.id == "concerts")
            .expect("Concerts must be exposed on the Plugins page");

        assert_eq!(descriptor.name, "Concerts");
        assert!(!descriptor.default_enabled);
        assert!(descriptor.applies_live);
        assert!(!is_enabled(&conn, &CONCERTS_MODULE).unwrap());
    }

    #[test]
    fn doc_7a_library_doctor_is_live_local_only_and_always_available() {
        let conn = migrated_conn();
        let descriptor = ALL_MODULES
            .iter()
            .copied()
            .find(|module| module.id == "library_doctor")
            .expect("Library Doctor must be exposed on the Plugins page");

        assert_eq!(descriptor.name, "Library Doctor");
        assert!(descriptor.applies_live);
        assert!(descriptor.default_enabled);
        assert!(is_enabled(&conn, descriptor).unwrap());
        assert!(!settings::get_bool(&conn, "library_doctor.remote.enabled", false).unwrap());

        set_enabled(&conn, descriptor, true).unwrap();
        assert!(is_enabled(&conn, descriptor).unwrap());
        assert!(!settings::get_bool(&conn, "library_doctor.remote.enabled", false).unwrap());
    }

    #[test]
    fn new_releases_round_trips() {
        let conn = migrated_conn();
        set_enabled(&conn, &NEW_RELEASES_MODULE, true).unwrap();
        assert!(is_enabled(&conn, &NEW_RELEASES_MODULE).unwrap());
        set_enabled(&conn, &NEW_RELEASES_MODULE, false).unwrap();
        assert!(!is_enabled(&conn, &NEW_RELEASES_MODULE).unwrap());
    }

    #[test]
    fn all_modules_includes_opt_in_artist_portraits() {
        assert!(ALL_MODULES
            .iter()
            .any(|module| module.id == "artist_portraits"));
    }

    #[test]
    fn src_11_all_modules_includes_opt_in_source_images() {
        let conn = migrated_conn();
        assert!(ALL_MODULES
            .iter()
            .any(|module| module.id == "source_images"));
        assert_eq!(
            enabled_key(&SOURCE_IMAGES_MODULE),
            "module.source_images.enabled"
        );
        assert!(!is_enabled(&conn, &SOURCE_IMAGES_MODULE).unwrap());
    }

    #[test]
    fn network_modules_default_off_and_apply_live() {
        let conn = migrated_conn();
        for module in [
            &COVER_DOWNLOAD_MODULE,
            &ARTIST_PORTRAITS_MODULE,
            &ONLINE_LYRICS_MODULE,
            &SOURCE_IMAGES_MODULE,
        ] {
            assert!(!module.default_enabled, "{} must be opt-in", module.id);
            assert!(module.applies_live, "{} must apply live", module.id);
            assert!(!is_enabled(&conn, module).unwrap());
            assert_eq!(
                ALL_MODULES
                    .iter()
                    .filter(|registered| registered.id == module.id)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn all_modules_excludes_core_playback_features() {
        assert!(!ALL_MODULES.iter().any(|module| module.id == "equalizer"));
        assert!(!ALL_MODULES.iter().any(|module| module.id == "replaygain"));
    }

    #[test]
    fn lastfm_is_registered_once_default_off_with_namespaced_key() {
        let conn = migrated_conn();
        assert!(!is_enabled(&conn, &LASTFM_MODULE).unwrap());
        assert_eq!(enabled_key(&LASTFM_MODULE), "module.lastfm.enabled");
        assert_eq!(
            ALL_MODULES
                .iter()
                .filter(|module| module.id == "lastfm")
                .count(),
            1
        );
    }

    #[test]
    fn ac_23_song_visuals_are_a_live_opt_in_module() {
        let conn = migrated_conn();
        let descriptor = ALL_MODULES
            .iter()
            .copied()
            .find(|module| module.id == "song_visuals")
            .expect("Song Visuals must be exposed on the Plugins page");

        assert_eq!(descriptor.name, "Song Visuals");
        assert!(!descriptor.default_enabled);
        assert!(descriptor.applies_live);
        assert!(!is_enabled(&conn, descriptor).unwrap());
        set_enabled(&conn, descriptor, true).unwrap();
        assert!(is_enabled(&conn, descriptor).unwrap());
    }

    #[test]
    fn src_1_podcasts_default_off_and_radio_defaults_on() {
        let conn = migrated_conn();

        assert!(!is_enabled(&conn, &PODCASTS_MODULE).unwrap());
        assert!(is_enabled(&conn, &RADIO_MODULE).unwrap());
        for id in ["podcasts", "radio"] {
            let descriptor = ALL_MODULES
                .iter()
                .find(|module| module.id == id)
                .expect("source module must be registered");
            assert!(descriptor.applies_live);
        }
    }

    #[test]
    fn source_modules_are_registered_once() {
        for module in [&PODCASTS_MODULE, &YOUTUBE_MODULE, &RADIO_MODULE] {
            assert_eq!(
                ALL_MODULES
                    .iter()
                    .filter(|registered| registered.id == module.id)
                    .count(),
                1
            );
        }
    }

    /// Issue #96: YouTube is a peer module, independent of Podcasts, so
    /// "Podcasts off + YouTube on" is representable in the data model —
    /// not only in the presentation.
    #[test]
    fn youtube_is_a_peer_module_independent_of_podcasts() {
        let conn = migrated_conn();

        assert!(!is_enabled(&conn, &YOUTUBE_MODULE).unwrap());
        assert_eq!(enabled_key(&YOUTUBE_MODULE), "module.youtube.enabled");
        assert!(ALL_MODULES
            .iter()
            .any(|module| module.id == "youtube" && module.applies_live));

        set_enabled(&conn, &YOUTUBE_MODULE, true).unwrap();
        assert!(is_enabled(&conn, &YOUTUBE_MODULE).unwrap());
        assert!(
            !is_enabled(&conn, &PODCASTS_MODULE).unwrap(),
            "Podcasts off + YouTube on must be a valid, independent state"
        );
    }
}
