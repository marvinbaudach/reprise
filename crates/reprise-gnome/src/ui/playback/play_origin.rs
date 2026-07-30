//! Playback origin: which view a `play_from_view` snapshot was started from,
//! plus a display label resolved at play time. Powers the Queue view's
//! named virtual context-tail header (QUE-7) and NAV-9b's jump target.
//! The label is resolved once when playback starts (not on display) so a
//! playlist renamed mid-playback keeps the name the user pressed play on,
//! and so a session restore never needs a second lookup.

use reprise_core::browser::BrowserPlace;
use reprise_core::db::Db;
use reprise_core::library::playlists;
use reprise_core::library::session::SessionSource;
use reprise_core::view_source::ViewSource;

use crate::ui::strings;

/// Where the current playback context came from. `place` is the immutable,
/// structured jump target; `label` is the equally immutable human name shown
/// in the Queue view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlayOrigin {
    pub place: BrowserPlace,
    pub label: String,
}

impl PlayOrigin {
    /// The fallback origin: the full library, labeled like the sidebar's
    /// "Music" row.
    pub(crate) fn library() -> Self {
        Self {
            place: BrowserPlace::from(ViewSource::Library),
            label: strings::text(strings::SIDEBAR_MUSIC),
        }
    }
}

/// Builds the origin for a play started from `place`, resolving the display
/// label once. Queue itself is a projection of the active snapshot rather
/// than an independent origin, so only that destination collapses to Music.
pub(crate) fn resolve(conn: &Db, place: &BrowserPlace) -> PlayOrigin {
    let source = place.view_source();
    match source {
        ViewSource::Queue => PlayOrigin::library(),
        _ => PlayOrigin {
            place: place.clone(),
            label: resolve_label(conn, &source),
        },
    }
}

fn resolve_label(conn: &Db, source: &ViewSource) -> String {
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
        ViewSource::Genre(genre) => genre.clone(),
        ViewSource::RecentlyAdded => strings::text(strings::SIDEBAR_RECENTLY_ADDED),
        ViewSource::Missing => strings::text(strings::SIDEBAR_MISSING_FILES),
        ViewSource::ImportErrors => strings::text(strings::SIDEBAR_IMPORT_ERRORS),
        ViewSource::Library
        | ViewSource::Queue
        | ViewSource::MyStats
        | ViewSource::Releases
        | ViewSource::Concerts
        | ViewSource::Podcasts
        | ViewSource::Youtube
        | ViewSource::Radio
        | ViewSource::Conversions => strings::text(strings::SIDEBAR_MUSIC),
    }
}

/// Origin for a container-play from an artist hero ("Play all"/"Shuffle").
pub(crate) fn from_artist(artist: &str) -> PlayOrigin {
    PlayOrigin {
        place: BrowserPlace::from(ViewSource::Artist(artist.to_string())),
        label: artist.to_string(),
    }
}

/// Persists both the legacy source projection and the complete frozen place.
pub(crate) fn to_session(
    origin: Option<&PlayOrigin>,
) -> (Option<SessionSource>, Option<String>, Option<BrowserPlace>) {
    let Some(origin) = origin else {
        return (None, None, None);
    };
    let source = match origin.place.view_source() {
        ViewSource::Playlist(id) => SessionSource::Playlist(id),
        ViewSource::Smart(id) => SessionSource::Smart(id),
        ViewSource::RecentlyAdded => SessionSource::RecentlyAdded,
        ViewSource::Missing => SessionSource::Missing,
        ViewSource::ImportErrors => SessionSource::ImportErrors,
        _ => SessionSource::Library,
    };
    (
        Some(source),
        Some(origin.label.clone()),
        Some(origin.place.clone()),
    )
}

/// Inverse of `to_session` for app startup. A stored origin without a label
/// (hand-edited or truncated session JSON) falls back to the library label.
pub(crate) fn from_session(
    source: Option<SessionSource>,
    label: Option<String>,
    place: Option<BrowserPlace>,
) -> Option<PlayOrigin> {
    if let Some(place) = place {
        return Some(PlayOrigin {
            place,
            label: label.unwrap_or_else(|| strings::text(strings::SIDEBAR_MUSIC)),
        });
    }
    let source = match source? {
        SessionSource::Library | SessionSource::Queue => ViewSource::Library,
        SessionSource::RecentlyAdded => ViewSource::RecentlyAdded,
        SessionSource::Playlist(id) => ViewSource::Playlist(id),
        SessionSource::Smart(id) => ViewSource::Smart(id),
        SessionSource::Missing => ViewSource::Missing,
        SessionSource::ImportErrors => ViewSource::ImportErrors,
    };
    Some(PlayOrigin {
        place: BrowserPlace::from(source),
        label: label.unwrap_or_else(|| strings::text(strings::SIDEBAR_MUSIC)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Db {
        crate::test_db::open().unwrap()
    }

    #[test]
    fn playlist_origin_resolves_its_name_and_legacy_session_destination() {
        let conn = conn();
        let id = playlists::create(&conn, "Late Night").unwrap();
        let mut place = reprise_core::browser::BrowserPlace::from(ViewSource::Playlist(id));
        place.track_state_mut().unwrap().search = "night".into();
        let origin = resolve(&conn, &place);
        assert_eq!(origin.label, "Late Night");
        assert_eq!(origin.place, place);

        let (kind, label, place) = to_session(Some(&origin));
        let restored = from_session(kind, label, place).unwrap();
        assert_eq!(restored.place, origin.place);
        assert_eq!(restored.label, origin.label);
    }

    #[test]
    fn deleted_playlist_falls_back_to_the_library_label() {
        let conn = conn();
        let origin = resolve(
            &conn,
            &reprise_core::browser::BrowserPlace::from(ViewSource::Playlist(9999)),
        );
        assert_eq!(origin.label, strings::text(strings::SIDEBAR_MUSIC));
    }

    #[test]
    fn album_origin_keeps_its_label_but_collapses_to_library_in_the_session() {
        let conn = conn();
        let place =
            reprise_core::browser::BrowserPlace::fresh_album("Neverbloom", "Make Them Suffer");
        let origin = resolve(&conn, &place);
        assert_eq!(origin.label, "Neverbloom");

        let (kind, label, place) = to_session(Some(&origin));
        assert_eq!(kind, Some(SessionSource::Library));
        let restored = from_session(kind, label, place).unwrap();
        assert_eq!(restored.place, origin.place);
        assert_eq!(restored.label, "Neverbloom");
    }

    #[test]
    fn queue_and_transient_sources_collapse_to_the_library_origin() {
        let conn = conn();
        assert_eq!(
            resolve(
                &conn,
                &reprise_core::browser::BrowserPlace::from(ViewSource::Queue)
            ),
            PlayOrigin::library()
        );
        let stats = reprise_core::browser::BrowserPlace::MyStats;
        let stats_origin = resolve(&conn, &stats);
        assert_eq!(stats_origin.place, stats);
    }

    #[test]
    fn play_8_origin_freezes_the_complete_browser_place() {
        let conn = conn();
        let mut place =
            reprise_core::browser::BrowserPlace::fresh_album("Pain Remains", "Lorna Shore");
        let state = place.track_state_mut().unwrap();
        state.search = "fire".into();
        state.selected_ids = vec![42];

        let origin = resolve(&conn, &place);
        place.track_state_mut().unwrap().search = "changed later".into();

        assert_eq!(origin.place.track_state().unwrap().search, "fire");
        assert_eq!(origin.place.track_state().unwrap().selected_ids, vec![42]);
    }

    #[test]
    fn absent_session_origin_stays_absent() {
        assert_eq!(from_session(None, Some("orphan".into()), None), None);
        assert_eq!(to_session(None), (None, None, None));
    }
}
