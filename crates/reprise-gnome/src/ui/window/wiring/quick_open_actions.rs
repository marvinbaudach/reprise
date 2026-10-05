//! Quick-open semantic dispatch, kept separate from dialog and worker wiring.

use std::rc::Rc;

use reprise_core::browser::navigation::{NavigationIntent, SidebarTarget, SourceKind};
use reprise_core::browser::{AlbumKey, ArtistKey, BrowserPlace};
use reprise_core::db::Db;
use reprise_core::view_source::ViewSource;
use reprise_view::quick_open::{QuickOpenAction, QuickOpenCandidate, QuickOpenKind};
use reprise_view::search_scope::SearchScope;

use super::super::{metadata_navigation::MetadataNavigator, section_search::SectionSearch};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackActivation {
    PlayNext,
    PlayContext,
}

const fn track_activation(play_next: bool) -> TrackActivation {
    if play_next {
        TrackActivation::PlayNext
    } else {
        TrackActivation::PlayContext
    }
}

pub(super) fn dispatch_item(
    item: &QuickOpenCandidate,
    play_next: bool,
    player: Option<&Rc<crate::ui::player_controller::PlayerController>>,
    db: &Db,
    navigator: &MetadataNavigator,
) {
    match &item.action {
        QuickOpenAction::PlayTrack {
            track_id,
            album,
            album_artist,
            artist,
        } => {
            let Some(player) = player else { return };
            match track_activation(play_next) {
                TrackActivation::PlayNext => {
                    player.play_next(&[*track_id]);
                }
                TrackActivation::PlayContext => play_track_context(
                    player,
                    db,
                    *track_id,
                    album.as_deref(),
                    album_artist.as_deref(),
                    artist.as_deref(),
                ),
            }
        }
        QuickOpenAction::PlayStation {
            station_id,
            name,
            stream_url,
            uuid,
        } => {
            let Some(player) = player else { return };
            let media = crate::ui::playback::external_media::ExternalMedia::Radio {
                station_id: *station_id,
                name: name.clone(),
                stream_url: stream_url.clone(),
                uuid: uuid.clone(),
            };
            if let Err(error) = player.play_external(media) {
                tracing::warn!(%error, "quick-open station could not start");
            }
        }
        QuickOpenAction::NavigateAlbum {
            album,
            album_artist,
        } => navigator.navigate(
            NavigationIntent::OpenAlbum {
                album: AlbumKey::new(album, album_artist),
                anchor_track_id: None,
            },
            "quick open album",
        ),
        QuickOpenAction::NavigateArtist { artist } => navigator.navigate(
            NavigationIntent::OpenArtist {
                artist: ArtistKey::new(artist),
                anchor_track_id: None,
            },
            "quick open artist",
        ),
        QuickOpenAction::NavigatePlaylist { playlist_id, smart } => navigator.navigate(
            NavigationIntent::Sidebar(if *smart {
                SidebarTarget::Smart(*playlist_id)
            } else {
                SidebarTarget::Playlist(*playlist_id)
            }),
            "quick open playlist",
        ),
        QuickOpenAction::NavigatePodcast { subscription_id } => navigator.navigate(
            NavigationIntent::RevealEpisode {
                subscription_id: *subscription_id,
                episode_id: None,
                kind: SourceKind::Podcasts,
            },
            "quick open podcast",
        ),
    }
}

fn play_track_context(
    player: &Rc<crate::ui::player_controller::PlayerController>,
    db: &Db,
    track_id: i64,
    album: Option<&str>,
    album_artist: Option<&str>,
    artist: Option<&str>,
) {
    let (ids, place) = resolve_track_context(
        track_id,
        album,
        album_artist,
        artist,
        |album, album_artist| {
            reprise_core::queries::query_album_canonical_track_ids(db, album, album_artist)
        },
        |artist| reprise_core::queries::query_artist_canonical_track_ids(db, artist),
    );
    let start = ids.iter().position(|id| *id == track_id).unwrap_or(0);
    let origin = crate::ui::playback::play_origin::resolve(db, &place);
    player.play_from_view(ids, start, origin);
}

fn resolve_track_context<E>(
    track_id: i64,
    album: Option<&str>,
    album_artist: Option<&str>,
    artist: Option<&str>,
    mut album_ids: impl FnMut(&str, &str) -> Result<Vec<i64>, E>,
    mut artist_ids: impl FnMut(&str) -> Result<Vec<i64>, E>,
) -> (Vec<i64>, BrowserPlace) {
    let (ids, place) = if let (Some(album), Some(album_artist)) = (album, album_artist) {
        (
            album_ids(album, album_artist).unwrap_or_else(|_| vec![track_id]),
            BrowserPlace::fresh_album(album, album_artist),
        )
    } else if let Some(artist) = artist {
        (
            artist_ids(artist).unwrap_or_else(|_| vec![track_id]),
            BrowserPlace::from(ViewSource::Artist(artist.to_owned())),
        )
    } else {
        (vec![track_id], BrowserPlace::from(ViewSource::Library))
    };
    let ids = if ids.contains(&track_id) {
        ids
    } else {
        vec![track_id]
    };
    (ids, place)
}

pub(super) fn show_all(
    kind: QuickOpenKind,
    query: &str,
    navigator: &MetadataNavigator,
    section_search: &Rc<SectionSearch>,
) {
    let Some((target, scope)) = show_all_target(kind) else {
        return;
    };
    navigator.navigate(NavigationIntent::Sidebar(target), "quick open show all");
    let query = query.to_owned();
    let section_search = section_search.clone();
    gtk4::glib::idle_add_local_once(move || section_search.set_query(scope, &query));
}

fn show_all_target(kind: QuickOpenKind) -> Option<(SidebarTarget, SearchScope)> {
    match kind {
        QuickOpenKind::Track => Some((SidebarTarget::Music, SearchScope::Tracks)),
        QuickOpenKind::Radio => Some((SidebarTarget::Radio, SearchScope::Radio)),
        QuickOpenKind::Album | QuickOpenKind::Artist | QuickOpenKind::Playlist => None,
        QuickOpenKind::Podcast => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_17_track_context_falls_from_album_to_artist_to_track() {
        let (album, _) = resolve_track_context(
            2,
            Some("Blue"),
            Some("Joni Mitchell"),
            Some("Joni Mitchell"),
            |_, _| Ok::<_, ()>(vec![1, 2, 3]),
            |_| Ok::<_, ()>(vec![2, 4]),
        );
        assert_eq!(album, vec![1, 2, 3]);

        let (artist, _) = resolve_track_context(
            2,
            None,
            None,
            Some("Joni Mitchell"),
            |_, _| Ok::<_, ()>(Vec::new()),
            |_| Ok::<_, ()>(vec![2, 4]),
        );
        assert_eq!(artist, vec![2, 4]);

        let (single, _) = resolve_track_context(
            2,
            None,
            None,
            None,
            |_, _| Ok::<_, ()>(Vec::new()),
            |_| Ok::<_, ()>(Vec::new()),
        );
        assert_eq!(single, vec![2]);
    }

    #[test]
    fn search_17_show_all_only_targets_matching_search_surfaces() {
        assert_eq!(
            show_all_target(QuickOpenKind::Track),
            Some((SidebarTarget::Music, SearchScope::Tracks))
        );
        assert_eq!(show_all_target(QuickOpenKind::Album), None);
        assert_eq!(show_all_target(QuickOpenKind::Playlist), None);
    }

    #[test]
    fn search_17_alt_enter_dispatches_play_next() {
        assert_eq!(track_activation(true), TrackActivation::PlayNext);
        assert_eq!(track_activation(false), TrackActivation::PlayContext);
    }
}
