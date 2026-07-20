//! Playback origin: which view a `play_from_view` snapshot was started from,
//! plus a display label resolved at play time. Powers the Queue view's
//! named virtual context-tail header (QUE-7) and NAV-9b's jump target.
//! The label is resolved once when playback starts (not on display) so a
//! playlist renamed mid-playback keeps the name the user pressed play on,
//! and so a session restore never needs a second lookup.

use reprise_core::library::playlists;
use reprise_core::library::session::SessionSource;
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use crate::ui::strings;

/// Where the current playback context came from. `source` is the jump
/// target (NAV-9b); `label` is the human name shown in the Queue view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlayOrigin {
    pub source: ViewSource,
    pub label: String,
}

impl PlayOrigin {
    /// The fallback origin: the full library, labeled like the sidebar's
    /// "Music" row.
    pub(crate) fn library() -> Self {
        Self {
            source: ViewSource::Library,
            label: strings::text(strings::SIDEBAR_MUSIC),
        }
    }
}

/// Builds the origin for a play started from `source`, resolving the
/// display label. Sources that are not a stable "home" (Queue itself,
/// stats, devices) collapse to the library origin.
pub(crate) fn resolve(conn: &Connection, source: &ViewSource) -> PlayOrigin {
    match source {
        ViewSource::Queue | ViewSource::MyStats | ViewSource::Device { .. } => {
            PlayOrigin::library()
        }
        _ => PlayOrigin {
            source: source.clone(),
            label: resolve_label(conn, source),
        },
    }
}

fn resolve_label(conn: &Connection, source: &ViewSource) -> String {
    match source {
        ViewSource::Playlist(id) => playlists::list(conn)
            .ok()
            .and_then(|lists| lists.into_iter().find(|playlist| playlist.id == *id))
            .map_or_else(
                || strings::text(strings::SIDEBAR_MUSIC),
                |playlist| playlist.name,
            ),
        ViewSource::Smart(id) => playlists::list_smart(conn)
            .ok()
            .and_then(|lists| lists.into_iter().find(|playlist| playlist.id == *id))
            .map_or_else(
                || strings::text(strings::SIDEBAR_MUSIC),
                |playlist| playlist.name,
            ),
        ViewSource::Album { album, .. } => album.clone(),
        ViewSource::Artist(artist) => artist.clone(),
        ViewSource::Missing => strings::text(strings::SIDEBAR_MISSING_FILES),
        ViewSource::ImportErrors => strings::text(strings::SIDEBAR_IMPORT_ERRORS),
        ViewSource::Library
        | ViewSource::Queue
        | ViewSource::MyStats
        | ViewSource::Device { .. } => strings::text(strings::SIDEBAR_MUSIC),
    }
}

/// Origin for a container-play from an album card/context menu — no DB
/// lookup needed, the album title itself is the label.
pub(crate) fn from_album_source(source: ViewSource) -> PlayOrigin {
    let label = match &source {
        ViewSource::Album { album, .. } => album.clone(),
        _ => strings::text(strings::SIDEBAR_MUSIC),
    };
    PlayOrigin { source, label }
}

/// Origin for a container-play from an artist hero ("Play all"/"Shuffle").
pub(crate) fn from_artist(artist: &str) -> PlayOrigin {
    PlayOrigin {
        source: ViewSource::Artist(artist.to_string()),
        label: artist.to_string(),
    }
}

/// Session-persistence projection. `SessionSource` cannot carry Album/Artist
/// origins — those collapse to `Library` for the *jump target* while the
/// label (the album/artist name) is kept for the Queue view's section title.
pub(crate) fn to_session(origin: Option<&PlayOrigin>) -> (Option<SessionSource>, Option<String>) {
    let Some(origin) = origin else {
        return (None, None);
    };
    let source = match origin.source {
        ViewSource::Playlist(id) => SessionSource::Playlist(id),
        ViewSource::Smart(id) => SessionSource::Smart(id),
        ViewSource::Missing => SessionSource::Missing,
        ViewSource::ImportErrors => SessionSource::ImportErrors,
        _ => SessionSource::Library,
    };
    (Some(source), Some(origin.label.clone()))
}

/// Inverse of `to_session` for app startup. A stored origin without a label
/// (hand-edited or truncated session JSON) falls back to the library label.
pub(crate) fn from_session(
    source: Option<SessionSource>,
    label: Option<String>,
) -> Option<PlayOrigin> {
    let source = match source? {
        SessionSource::Library | SessionSource::Queue => ViewSource::Library,
        SessionSource::Playlist(id) => ViewSource::Playlist(id),
        SessionSource::Smart(id) => ViewSource::Smart(id),
        SessionSource::Missing => ViewSource::Missing,
        SessionSource::ImportErrors => ViewSource::ImportErrors,
    };
    Some(PlayOrigin {
        source,
        label: label.unwrap_or_else(|| strings::text(strings::SIDEBAR_MUSIC)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn playlist_origin_resolves_its_name_and_survives_the_session_round_trip() {
        let conn = conn();
        let id = playlists::create(&conn, "Late Night").unwrap();
        let origin = resolve(&conn, &ViewSource::Playlist(id));
        assert_eq!(origin.label, "Late Night");
        assert_eq!(origin.source, ViewSource::Playlist(id));

        let (kind, label) = to_session(Some(&origin));
        assert_eq!(from_session(kind, label), Some(origin));
    }

    #[test]
    fn deleted_playlist_falls_back_to_the_library_label() {
        let conn = conn();
        let origin = resolve(&conn, &ViewSource::Playlist(9999));
        assert_eq!(origin.label, strings::text(strings::SIDEBAR_MUSIC));
    }

    #[test]
    fn album_origin_keeps_its_label_but_collapses_to_library_in_the_session() {
        let conn = conn();
        let origin = resolve(
            &conn,
            &ViewSource::Album {
                album: "Neverbloom".into(),
                album_artist: "Make Them Suffer".into(),
            },
        );
        assert_eq!(origin.label, "Neverbloom");

        let (kind, label) = to_session(Some(&origin));
        assert_eq!(kind, Some(SessionSource::Library));
        let restored = from_session(kind, label).unwrap();
        assert_eq!(restored.source, ViewSource::Library);
        assert_eq!(restored.label, "Neverbloom");
    }

    #[test]
    fn queue_and_transient_sources_collapse_to_the_library_origin() {
        let conn = conn();
        assert_eq!(resolve(&conn, &ViewSource::Queue), PlayOrigin::library());
        assert_eq!(resolve(&conn, &ViewSource::MyStats), PlayOrigin::library());
    }

    #[test]
    fn absent_session_origin_stays_absent() {
        assert_eq!(from_session(None, Some("orphan".into())), None);
        assert_eq!(to_session(None), (None, None));
    }
}
