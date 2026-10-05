//! One database snapshot for a quick-open session.

use std::path::Path;

use reprise_core::browser::SortDirection;
use reprise_core::db::Db;
use reprise_core::queries::{
    LibraryTrackOrder, LibraryTrackRequest, LibraryTrackScope, WindowRange,
};
use reprise_view::quick_open::{QuickOpenAction, QuickOpenCandidate, QuickOpenKind};

pub(super) struct CandidateSnapshot {
    pub(super) change_id: i64,
    pub(super) candidates: Vec<QuickOpenCandidate>,
}

pub(super) fn load_candidates(path: &Path) -> Result<CandidateSnapshot, String> {
    let db = Db::open_ready_read_only(path).map_err(|error| error.to_string())?;
    let change_id = reprise_core::events::latest_id(&db).map_err(|error| error.to_string())?;
    let mut candidates = Vec::new();
    load_tracks(&db, &mut candidates)?;
    load_albums(&db, &mut candidates)?;
    load_artists(&db, &mut candidates)?;
    load_playlists(&db, &mut candidates)?;
    if reprise_core::modules::is_enabled(&db, &reprise_core::modules::PODCASTS_MODULE)
        .map_err(|error| error.to_string())?
    {
        load_podcasts(&db, &mut candidates)?;
    }
    if reprise_core::modules::is_enabled(&db, &reprise_core::modules::RADIO_MODULE)
        .map_err(|error| error.to_string())?
    {
        load_radio(&db, &mut candidates)?;
    }
    Ok(CandidateSnapshot {
        change_id,
        candidates,
    })
}

fn load_tracks(db: &Db, candidates: &mut Vec<QuickOpenCandidate>) -> Result<(), String> {
    let mut offset = 0;
    loop {
        let window = reprise_core::queries::query_library_tracks(
            db,
            &LibraryTrackRequest {
                scope: LibraryTrackScope::All,
                search: String::new(),
                order: LibraryTrackOrder::Sorted {
                    field: "title".into(),
                    direction: SortDirection::Ascending,
                },
                window: WindowRange { offset, limit: 500 },
            },
        )
        .map_err(|error| error.to_string())?;
        for track in window.rows {
            let album = nonblank(&track.album);
            let effective_album_artist = if track.album_artist.trim().is_empty() {
                &track.artist
            } else {
                &track.album_artist
            };
            let album_artist = album
                .as_ref()
                .map(|_| effective_album_artist.trim().to_owned());
            let artist = nonblank(&track.artist);
            let subtitle = match (&artist, &album) {
                (Some(artist), Some(album)) => format!("{artist} · {album}"),
                (Some(artist), None) => artist.clone(),
                (None, Some(album)) => album.clone(),
                (None, None) => String::new(),
            };
            let mut search_text = vec![track.title.clone()];
            search_text.extend(artist.iter().cloned());
            search_text.extend(album.iter().cloned());
            candidates.push(QuickOpenCandidate::new(
                QuickOpenKind::Track,
                track.title,
                subtitle,
                search_text,
                track.play_count,
                QuickOpenAction::PlayTrack {
                    track_id: track.id,
                    album,
                    album_artist,
                    artist,
                },
            ));
        }
        if !window.has_more {
            break;
        }
        offset += 500;
    }
    Ok(())
}

fn load_albums(db: &Db, candidates: &mut Vec<QuickOpenCandidate>) -> Result<(), String> {
    let mut offset = 0;
    loop {
        let window =
            reprise_core::queries::query_albums(db, "", WindowRange { offset, limit: 500 })
                .map_err(|error| error.to_string())?;
        candidates.extend(window.rows.into_iter().map(|album| {
            QuickOpenCandidate::new(
                QuickOpenKind::Album,
                album.album.clone(),
                album.album_artist.clone(),
                vec![album.album.clone()],
                album.total_play_count,
                QuickOpenAction::NavigateAlbum {
                    album: album.album,
                    album_artist: album.album_artist,
                },
            )
        }));
        if !window.has_more {
            break;
        }
        offset += 500;
    }
    Ok(())
}

fn load_artists(db: &Db, candidates: &mut Vec<QuickOpenCandidate>) -> Result<(), String> {
    let mut offset = 0;
    loop {
        let window =
            reprise_core::queries::query_artists(db, "", WindowRange { offset, limit: 500 })
                .map_err(|error| error.to_string())?;
        candidates.extend(window.rows.into_iter().map(|artist| {
            QuickOpenCandidate::new(
                QuickOpenKind::Artist,
                artist.artist.clone(),
                crate::ui::strings::quick_open_track_count(
                    usize::try_from(artist.track_count.max(0)).unwrap_or(usize::MAX),
                ),
                vec![artist.artist.clone()],
                artist.total_plays,
                QuickOpenAction::NavigateArtist {
                    artist: artist.artist,
                },
            )
        }));
        if !window.has_more {
            break;
        }
        offset += 500;
    }
    Ok(())
}

fn load_playlists(db: &Db, candidates: &mut Vec<QuickOpenCandidate>) -> Result<(), String> {
    let lists = reprise_core::library::playlists::list(db).map_err(|error| error.to_string())?;
    candidates.extend(lists.into_iter().map(|playlist| {
        QuickOpenCandidate::new(
            QuickOpenKind::Playlist,
            playlist.name.clone(),
            crate::ui::strings::quick_open_track_count(
                usize::try_from(playlist.track_count.max(0)).unwrap_or(usize::MAX),
            ),
            vec![playlist.name],
            0,
            QuickOpenAction::NavigatePlaylist {
                playlist_id: playlist.id,
                smart: false,
            },
        )
    }));
    let smart =
        reprise_core::library::playlists::list_smart(db).map_err(|error| error.to_string())?;
    candidates.extend(smart.into_iter().map(|playlist| {
        QuickOpenCandidate::new(
            QuickOpenKind::Playlist,
            playlist.name.clone(),
            crate::ui::strings::text(crate::ui::strings::QUICK_OPEN_SMART_PLAYLIST),
            vec![playlist.name],
            0,
            QuickOpenAction::NavigatePlaylist {
                playlist_id: playlist.id,
                smart: true,
            },
        )
    }));
    Ok(())
}

fn load_podcasts(db: &Db, candidates: &mut Vec<QuickOpenCandidate>) -> Result<(), String> {
    let subscriptions = reprise_core::podcasts::store::active_subscriptions(db)
        .map_err(|error| error.to_string())?;
    candidates.extend(
        subscriptions
            .into_iter()
            .filter(|subscription| subscription.kind == reprise_core::podcasts::PodcastKind::Rss)
            .map(|subscription| {
                QuickOpenCandidate::new(
                    QuickOpenKind::Podcast,
                    subscription.title.clone(),
                    subscription.author.unwrap_or_default(),
                    vec![subscription.title],
                    0,
                    QuickOpenAction::NavigatePodcast {
                        subscription_id: subscription.id,
                    },
                )
            }),
    );
    Ok(())
}

fn load_radio(db: &Db, candidates: &mut Vec<QuickOpenCandidate>) -> Result<(), String> {
    let stations = reprise_core::radio::station::list(db).map_err(|error| error.to_string())?;
    candidates.extend(stations.into_iter().map(|station| {
        QuickOpenCandidate::new(
            QuickOpenKind::Radio,
            station.name.clone(),
            station.genre.clone().unwrap_or_default(),
            vec![station.name.clone()],
            station.votes.unwrap_or(0),
            QuickOpenAction::PlayStation {
                station_id: station.id,
                name: station.name,
                stream_url: station.stream_url,
                uuid: station.uuid,
            },
        )
    }));
    Ok(())
}

fn nonblank(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use reprise_core::podcasts::store::{add_or_restore, NewSubscription};
    use reprise_core::podcasts::PodcastKind;
    use reprise_view::quick_open::QuickOpenKind;

    use super::load_podcasts;

    #[test]
    fn search_17_podcast_candidates_need_no_episode_query() {
        let db = crate::test_db::open().unwrap();
        let subscription_id = add_or_restore(
            &db,
            &NewSubscription {
                kind: PodcastKind::Rss,
                feed_url: "https://example.test/feed".into(),
                title: "Empty show".into(),
                author: Some("Host".into()),
                image_url: None,
                auto_download: false,
            },
            1,
        )
        .unwrap();
        let mut candidates = Vec::new();

        load_podcasts(&db, &mut candidates).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, QuickOpenKind::Podcast);
        assert!(matches!(
            candidates[0].action,
            reprise_view::quick_open::QuickOpenAction::NavigatePodcast {
                subscription_id: id
            } if id == subscription_id
        ));
    }
}
