//! Registry metadata for optional features shown on the Plugins page.
//!
//! Core player capabilities such as Equalizer and ReplayGain deliberately do
//! not belong here. Device support likewise belongs to Synchronization. This
//! registry is reserved for optional integrations and features that depend on
//! external services or APIs.

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
}

pub const MPRIS_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "mpris",
    name: "MPRIS",
    description: "GNOME media controls, media keys, and lock-screen integration (D-Bus)",
    default_enabled: true,
};

pub const LISTENBRAINZ_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "listenbrainz",
    name: "ListenBrainz",
    description: "Scrobble completed listens to ListenBrainz (network; off by default)",
    default_enabled: false,
};

pub const LASTFM_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "lastfm",
    name: "Last.fm",
    description: "Scrobble completed listens to Last.fm (network; off by default)",
    default_enabled: false,
};

pub const ARTIST_NEWS_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "artist_news",
    name: "Artist & Album News",
    description:
        "Show upcoming and newly released albums from MusicBrainz (network; off by default)",
    default_enabled: false,
};

/// Every optional integration the app currently exposes, in Plugins-page order.
pub const ALL_MODULES: &[&ModuleDescriptor] =
    &[&ARTIST_NEWS_MODULE, &LISTENBRAINZ_MODULE, &LASTFM_MODULE];

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
    fn all_modules_excludes_always_on_cover_download() {
        assert!(!ALL_MODULES
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
    fn artist_news_is_listed_and_defaults_to_disabled() {
        let conn = migrated_conn();
        assert!(ALL_MODULES
            .iter()
            .any(|module| module.id == ARTIST_NEWS_MODULE.id));
        assert!(!is_enabled(&conn, &ARTIST_NEWS_MODULE).unwrap());
    }

    #[test]
    fn artist_news_round_trips() {
        let conn = migrated_conn();
        set_enabled(&conn, &ARTIST_NEWS_MODULE, true).unwrap();
        assert!(is_enabled(&conn, &ARTIST_NEWS_MODULE).unwrap());
        set_enabled(&conn, &ARTIST_NEWS_MODULE, false).unwrap();
        assert!(!is_enabled(&conn, &ARTIST_NEWS_MODULE).unwrap());
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
}
