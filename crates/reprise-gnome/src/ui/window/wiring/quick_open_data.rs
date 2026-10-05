//! One database snapshot for a quick-open session.

use std::path::Path;

use reprise_core::browser::SortDirection;
use reprise_core::db::Db;
use reprise_core::podcasts::PodcastKind;
use reprise_core::queries::{
    LibraryTrackOrder, LibraryTrackRequest, LibraryTrackScope, WindowRange,
};
use reprise_view::quick_open::{QuickOpenAction, QuickOpenCandidate, QuickOpenKind};

pub(super) struct CandidateSnapshot {
    pub(super) change_id: i64,
    pub(super) candidates: Vec<QuickOpenCandidate>,
}

#[derive(Clone, Copy)]
struct OptionalSources {
    podcasts: bool,
    radio: bool,
}

fn enabled_optional_kinds(sources: OptionalSources) -> Vec<QuickOpenKind> {
    [
        (sources.podcasts, QuickOpenKind::Podcast),
        (sources.radio, QuickOpenKind::Radio),
    ]
    .into_iter()
    .filter_map(|(enabled, kind)| enabled.then_some(kind))
    .collect()
}

pub(super) fn load_candidates(path: &Path) -> Result<CandidateSnapshot, String> {
    let db = Db::open_ready_read_only(path).map_err(|error| error.to_string())?;
    let mut candidates = Vec::new();
    load_tracks(&db, &mut candidates)?;
    load_albums(&db, &mut candidates)?;
    load_artists(&db, &mut candidates)?;
    load_playlists(&db, &mut candidates)?;
    let optional = OptionalSources {
        podcasts: reprise_core::modules::is_enabled(&db, &reprise_core::modules::PODCASTS_MODULE)
            .map_err(|error| error.to_string())?,
        radio: reprise_core::modules::is_enabled(&db, &reprise_core::modules::RADIO_MODULE)
            .map_err(|error| error.to_string())?,
    };
    for kind in enabled_optional_kinds(optional) {
        match kind {
            QuickOpenKind::Podcast => load_podcasts(&db, &mut candidates)?,
            QuickOpenKind::Radio => load_radio(&db, &mut candidates)?,
            _ => unreachable!("only optional source kinds are returned"),
        }
    }
    let change_id = reprise_core::events::latest_id(&db).map_err(|error| error.to_string())?;
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
            candidates.push(QuickOpenCandidate {
                kind: QuickOpenKind::Track,
                title: track.title,
                subtitle,
                search_text,
                play_count: track.play_count,
                action: QuickOpenAction::PlayTrack {
                    track_id: track.id,
                    album,
                    album_artist,
                    artist,
                },
            });
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
        candidates.extend(window.rows.into_iter().map(|album| QuickOpenCandidate {
            kind: QuickOpenKind::Album,
            title: album.album.clone(),
            subtitle: album.album_artist.clone(),
            search_text: vec![album.album.clone()],
            play_count: album.total_play_count,
            action: QuickOpenAction::NavigateAlbum {
                album: album.album,
                album_artist: album.album_artist,
            },
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
        candidates.extend(window.rows.into_iter().map(|artist| QuickOpenCandidate {
            kind: QuickOpenKind::Artist,
            title: artist.artist.clone(),
            subtitle: crate::ui::strings::quick_open_track_count(
                usize::try_from(artist.track_count.max(0)).unwrap_or(usize::MAX),
            ),
            search_text: vec![artist.artist.clone()],
            play_count: artist.total_plays,
            action: QuickOpenAction::NavigateArtist {
                artist: artist.artist,
            },
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
    candidates.extend(lists.into_iter().map(|playlist| QuickOpenCandidate {
        kind: QuickOpenKind::Playlist,
        title: playlist.name.clone(),
        subtitle: crate::ui::strings::quick_open_track_count(
            usize::try_from(playlist.track_count.max(0)).unwrap_or(usize::MAX),
        ),
        search_text: vec![playlist.name],
        play_count: 0,
        action: QuickOpenAction::NavigatePlaylist {
            playlist_id: playlist.id,
            smart: false,
        },
    }));
    let smart =
        reprise_core::library::playlists::list_smart(db).map_err(|error| error.to_string())?;
    candidates.extend(smart.into_iter().map(|playlist| QuickOpenCandidate {
        kind: QuickOpenKind::Playlist,
        title: playlist.name.clone(),
        subtitle: crate::ui::strings::text(crate::ui::strings::QUICK_OPEN_SMART_PLAYLIST),
        search_text: vec![playlist.name],
        play_count: 0,
        action: QuickOpenAction::NavigatePlaylist {
            playlist_id: playlist.id,
            smart: true,
        },
    }));
    Ok(())
}

fn load_podcasts(db: &Db, candidates: &mut Vec<QuickOpenCandidate>) -> Result<(), String> {
    // Core has no lighter public show-list projection yet. Keep that ownership
    // boundary intact and use the existing cached source groups once per open.
    let groups = reprise_core::podcasts::query::list_source_groups(db, PodcastKind::Rss)
        .map_err(|error| error.to_string())?;
    candidates.extend(groups.into_iter().map(|group| QuickOpenCandidate {
        kind: QuickOpenKind::Podcast,
        title: group.title.clone(),
        subtitle: group.author.unwrap_or_default(),
        search_text: vec![group.title],
        play_count: 0,
        action: QuickOpenAction::NavigatePodcast {
            subscription_id: group.subscription_id,
        },
    }));
    Ok(())
}

fn load_radio(db: &Db, candidates: &mut Vec<QuickOpenCandidate>) -> Result<(), String> {
    let stations = reprise_core::radio::station::list(db).map_err(|error| error.to_string())?;
    candidates.extend(stations.into_iter().map(|station| QuickOpenCandidate {
        kind: QuickOpenKind::Radio,
        title: station.name.clone(),
        subtitle: station.genre.clone().unwrap_or_default(),
        search_text: vec![station.name.clone()],
        play_count: station.votes.unwrap_or(0),
        action: QuickOpenAction::PlayStation {
            station_id: station.id,
            name: station.name,
            stream_url: station.stream_url,
            uuid: station.uuid,
        },
    }));
    Ok(())
}

fn nonblank(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use reprise_view::quick_open::QuickOpenKind;

    use super::{enabled_optional_kinds, OptionalSources};

    #[test]
    fn search_17_optional_sources_follow_module_gating() {
        let sources = OptionalSources {
            podcasts: true,
            radio: false,
        };
        assert_eq!(
            enabled_optional_kinds(sources),
            vec![QuickOpenKind::Podcast]
        );
    }
}
