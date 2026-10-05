//! Quick-open semantic dispatch, kept separate from dialog and worker wiring.

use std::rc::Rc;

use reprise_core::browser::navigation::{NavigationIntent, SidebarTarget, SourceKind};
use reprise_core::browser::{AlbumKey, ArtistKey, BrowserPlace};
use reprise_core::connectivity::{self, ActionOutcome, Connectivity};
use reprise_core::db::Db;
use reprise_core::view_source::ViewSource;
use reprise_view::quick_open::{QuickOpenAction, QuickOpenCandidate, QuickOpenKind};
use reprise_view::search_scope::SearchScope;

use super::super::{metadata_navigation::MetadataNavigator, section_search::SectionSearch};

pub(super) trait DispatchTarget {
    fn play_next(&mut self, track_id: i64);
    fn play_context(
        &mut self,
        track_id: i64,
        album: Option<&str>,
        album_artist: Option<&str>,
        artist: Option<&str>,
    );
    fn play_station(&mut self, action: &QuickOpenAction);
    fn navigate(&mut self, intent: NavigationIntent, reason: &'static str);
    fn connectivity(&self) -> Connectivity;
    fn no_connection_retry(&mut self, message: &str);
}

pub(super) struct RuntimeDispatch<'a> {
    player: Option<&'a Rc<crate::ui::player_controller::PlayerController>>,
    db: &'a Db,
    navigator: &'a MetadataNavigator,
    connectivity: Connectivity,
}

impl<'a> RuntimeDispatch<'a> {
    pub(super) const fn new(
        player: Option<&'a Rc<crate::ui::player_controller::PlayerController>>,
        db: &'a Db,
        navigator: &'a MetadataNavigator,
        connectivity: Connectivity,
    ) -> Self {
        Self {
            player,
            db,
            navigator,
            connectivity,
        }
    }
}

pub(super) fn dispatch_item(
    item: &QuickOpenCandidate,
    play_next: bool,
    target: &mut impl DispatchTarget,
) {
    match &item.action {
        QuickOpenAction::PlayTrack {
            track_id,
            album,
            album_artist,
            artist,
        } => {
            if play_next {
                target.play_next(*track_id);
            } else {
                target.play_context(
                    *track_id,
                    album.as_deref(),
                    album_artist.as_deref(),
                    artist.as_deref(),
                );
            }
        }
        action @ QuickOpenAction::PlayStation { .. } => {
            match connectivity::live_stream_action_outcome(target.connectivity()) {
                ActionOutcome::RunsNow => target.play_station(action),
                ActionOutcome::NoConnectionRetry => target.no_connection_retry(
                    &crate::ui::strings::text(crate::ui::strings::RADIO_NO_CONNECTION_RETRY),
                ),
                ActionOutcome::QueuedOffline => {
                    tracing::error!("live-stream gate returned an invalid queued outcome");
                }
            }
        }
        QuickOpenAction::NavigateAlbum {
            album,
            album_artist,
        } => target.navigate(
            NavigationIntent::OpenAlbum {
                album: AlbumKey::new(album, album_artist),
                anchor_track_id: None,
            },
            "quick open album",
        ),
        QuickOpenAction::NavigateArtist { artist } => target.navigate(
            NavigationIntent::OpenArtist {
                artist: ArtistKey::new(artist),
                anchor_track_id: None,
            },
            "quick open artist",
        ),
        QuickOpenAction::NavigatePlaylist { playlist_id, smart } => target.navigate(
            NavigationIntent::Sidebar(if *smart {
                SidebarTarget::Smart(*playlist_id)
            } else {
                SidebarTarget::Playlist(*playlist_id)
            }),
            "quick open playlist",
        ),
        QuickOpenAction::NavigatePodcast { subscription_id } => target.navigate(
            NavigationIntent::RevealEpisode {
                subscription_id: *subscription_id,
                episode_id: None,
                kind: SourceKind::Podcasts,
            },
            "quick open podcast",
        ),
    }
}

impl DispatchTarget for RuntimeDispatch<'_> {
    fn play_next(&mut self, track_id: i64) {
        if let Some(player) = self.player {
            player.play_next(&[track_id]);
        }
    }

    fn play_context(
        &mut self,
        track_id: i64,
        album: Option<&str>,
        album_artist: Option<&str>,
        artist: Option<&str>,
    ) {
        if let Some(player) = self.player {
            play_track_context(player, self.db, track_id, album, album_artist, artist);
        }
    }

    fn play_station(&mut self, action: &QuickOpenAction) {
        let Some(player) = self.player else { return };
        let QuickOpenAction::PlayStation {
            station_id,
            name,
            stream_url,
            uuid,
        } = action
        else {
            return;
        };
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

    fn navigate(&mut self, intent: NavigationIntent, reason: &'static str) {
        self.navigator.navigate(intent, reason);
    }

    fn connectivity(&self) -> Connectivity {
        self.connectivity
    }

    fn no_connection_retry(&mut self, message: &str) {
        tracing::debug!("quick-open radio play skipped: no connection, retry when online");
        if let Some(player) = self.player {
            player.show_toast(message);
        }
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

    #[derive(Default)]
    struct RecordingDispatch {
        connectivity: Connectivity,
        played_next: Vec<i64>,
        stations: usize,
        retry_messages: Vec<String>,
    }

    impl DispatchTarget for RecordingDispatch {
        fn play_next(&mut self, track_id: i64) {
            self.played_next.push(track_id);
        }

        fn play_context(&mut self, _: i64, _: Option<&str>, _: Option<&str>, _: Option<&str>) {}

        fn play_station(&mut self, _: &QuickOpenAction) {
            self.stations += 1;
        }

        fn navigate(&mut self, _: NavigationIntent, _: &'static str) {}

        fn connectivity(&self) -> Connectivity {
            self.connectivity
        }

        fn no_connection_retry(&mut self, message: &str) {
            self.retry_messages.push(message.to_owned());
        }
    }

    fn track_candidate() -> QuickOpenCandidate {
        QuickOpenCandidate::new(
            QuickOpenKind::Track,
            "Blue".into(),
            "Joni Mitchell".into(),
            vec!["Blue".into()],
            0,
            QuickOpenAction::PlayTrack {
                track_id: 7,
                album: None,
                album_artist: None,
                artist: None,
            },
        )
    }

    fn station_candidate() -> QuickOpenCandidate {
        QuickOpenCandidate::new(
            QuickOpenKind::Radio,
            "Radio".into(),
            String::new(),
            vec!["Radio".into()],
            0,
            QuickOpenAction::PlayStation {
                station_id: 9,
                name: "Radio".into(),
                stream_url: "https://radio.test/stream".into(),
                uuid: None,
            },
        )
    }

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
    fn search_17_alt_enter_dispatches_play_next_through_the_player_seam() {
        let mut target = RecordingDispatch::default();

        dispatch_item(&track_candidate(), true, &mut target);

        assert_eq!(target.played_next, vec![7]);
    }

    #[test]
    fn search_17_radio_dispatch_uses_the_live_stream_gate() {
        let mut offline = RecordingDispatch {
            connectivity: Connectivity::Offline,
            ..RecordingDispatch::default()
        };
        dispatch_item(&station_candidate(), false, &mut offline);
        assert_eq!(offline.stations, 0);
        assert_eq!(
            offline.retry_messages,
            vec![crate::ui::strings::text(
                crate::ui::strings::RADIO_NO_CONNECTION_RETRY
            )]
        );

        let mut online = RecordingDispatch::default();
        dispatch_item(&station_candidate(), false, &mut online);
        assert_eq!(online.stations, 1);
        assert!(online.retry_messages.is_empty());
    }
}
