//! Quick-open action, worker search, and semantic action dispatch.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::GtkApplicationExt;
use reprise_core::browser::navigation::{NavigationIntent, SidebarTarget, SourceKind};
use reprise_core::browser::{AlbumKey, ArtistKey, BrowserPlace, SortDirection};
use reprise_core::db::Db;
use reprise_core::podcasts::PodcastKind;
use reprise_core::queries::{
    LibraryTrackOrder, LibraryTrackRequest, LibraryTrackScope, WindowRange,
};
use reprise_core::view_source::ViewSource;
use reprise_view::quick_open::{
    rank_and_group, QuickOpenAction, QuickOpenCandidate, QuickOpenKind, QuickOpenRecents,
    QuickOpenRow,
};
use reprise_view::search_scope::SearchScope;

use super::{quick_open::QuickOpenPanel, RuntimeWiring, WiringScratch};

type SearchCallback = Rc<dyn Fn(&str) -> Vec<QuickOpenRow>>;
type ActivateCallback = Rc<dyn Fn(QuickOpenRow, bool)>;

pub(super) fn wire_quick_open_shortcut(
    app: &libadwaita::Application,
    window: &libadwaita::ApplicationWindow,
    panel: &Rc<QuickOpenPanel>,
    search: SearchCallback,
    activate: ActivateCallback,
) {
    panel.connect_query({
        let panel = panel.clone();
        move |query| panel.set_results(search(&query))
    });
    panel.connect_activate(move |row, play_next| activate(row, play_next));
    let action = gio::SimpleAction::new("quick-open", None);
    action.connect_activate({
        let panel = panel.clone();
        let window = window.downgrade();
        move |_, _| {
            if let Some(window) = window.upgrade() {
                panel.present(&window);
            }
        }
    });
    window.add_action(&action);
    app.set_accels_for_action("win.quick-open", &["<Control>k"]);
}

pub(super) fn wire_quick_open(w: &RuntimeWiring<'_>, scratch: &WiringScratch) {
    let panel = Rc::new(QuickOpenPanel::new());
    let generation = Rc::new(Cell::new(0u64));
    let recents = Rc::new(RefCell::new(QuickOpenRecents::default()));
    let database_path = w.db_path.to_path_buf();

    let search: SearchCallback = Rc::new({
        let panel = panel.clone();
        let generation = generation.clone();
        let recents = recents.clone();
        move |query| {
            let next = generation.get().wrapping_add(1);
            generation.set(next);
            if query.trim().is_empty() {
                return recents
                    .borrow()
                    .items()
                    .into_iter()
                    .map(QuickOpenRow::Item)
                    .collect();
            }
            start_search(
                &panel,
                &generation,
                next,
                database_path.clone(),
                query.to_owned(),
            );
            Vec::new()
        }
    });

    let player = w.player.clone();
    let conn = w.conn.clone();
    let navigator = w.metadata_navigator.clone();
    let section_search = scratch.section_search().clone();
    let activate: ActivateCallback = Rc::new(move |row, play_next| match row {
        QuickOpenRow::Item(item) => {
            recents.borrow_mut().remember(item.clone());
            dispatch_item(&item, play_next, player.as_ref(), &conn, &navigator);
        }
        QuickOpenRow::ShowAll { kind, query, .. } => {
            show_all(kind, &query, &navigator, &section_search);
        }
    });
    wire_quick_open_shortcut(w.app, w.window, &panel, search, activate);
}

fn start_search(
    panel: &Rc<QuickOpenPanel>,
    generation: &Rc<Cell<u64>>,
    expected_generation: u64,
    database_path: std::path::PathBuf,
    query: String,
) {
    let receiver = match crate::ui::one_shot_task::spawn("reprise-quick-open", move || {
        load_candidates(&database_path)
            .map(|candidates| rank_and_group(candidates, &query))
            .map(|groups| {
                groups
                    .into_iter()
                    .flat_map(|group| group.rows)
                    .collect::<Vec<_>>()
            })
    }) {
        Ok(receiver) => receiver,
        Err(error) => {
            tracing::warn!(%error, "could not start quick-open search");
            return;
        }
    };
    let panel = panel.clone();
    let generation = generation.clone();
    gtk4::glib::spawn_future_local(async move {
        let result = receiver.recv().await;
        if generation.get() != expected_generation {
            return;
        }
        match result {
            Ok(Ok(rows)) => panel.set_results(rows),
            Ok(Err(error)) => tracing::warn!(%error, "quick-open search failed"),
            Err(error) => tracing::debug!(%error, "quick-open search was cancelled"),
        }
    });
}

fn load_candidates(path: &Path) -> Result<Vec<QuickOpenCandidate>, String> {
    let db = Db::open_ready_read_only(path).map_err(|error| error.to_string())?;
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
    Ok(candidates)
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
            candidates.push(QuickOpenCandidate {
                kind: QuickOpenKind::Track,
                title: track.title,
                subtitle,
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
            subtitle: crate::i18n::format_message(
                &crate::i18n::gettext("{count} tracks"),
                &[("count", &artist.track_count.to_string())],
            ),
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
        title: playlist.name,
        subtitle: crate::i18n::format_message(
            &crate::i18n::gettext("{count} tracks"),
            &[("count", &playlist.track_count.to_string())],
        ),
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
        title: playlist.name,
        subtitle: crate::i18n::gettext("Smart playlist"),
        play_count: 0,
        action: QuickOpenAction::NavigatePlaylist {
            playlist_id: playlist.id,
            smart: true,
        },
    }));
    Ok(())
}

fn load_podcasts(db: &Db, candidates: &mut Vec<QuickOpenCandidate>) -> Result<(), String> {
    let groups = reprise_core::podcasts::query::list_source_groups(db, PodcastKind::Rss)
        .map_err(|error| error.to_string())?;
    candidates.extend(groups.into_iter().map(|group| QuickOpenCandidate {
        kind: QuickOpenKind::Podcast,
        title: group.title,
        subtitle: group.author.unwrap_or_default(),
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

fn dispatch_item(
    item: &QuickOpenCandidate,
    play_next: bool,
    player: Option<&Rc<crate::ui::player_controller::PlayerController>>,
    db: &Db,
    navigator: &super::super::metadata_navigation::MetadataNavigator,
) {
    match &item.action {
        QuickOpenAction::PlayTrack {
            track_id,
            album,
            album_artist,
            artist,
        } => {
            let Some(player) = player else { return };
            if play_next {
                player.play_next(&[*track_id]);
                return;
            }
            play_track_context(
                player,
                db,
                *track_id,
                album.as_deref(),
                album_artist.as_deref(),
                artist.as_deref(),
            );
        }
        QuickOpenAction::PlayStation {
            station_id,
            name,
            stream_url,
            uuid,
        } => {
            let Some(player) = player else { return };
            if !gio::NetworkMonitor::default().is_network_available() {
                tracing::debug!(station_id, "quick-open radio play skipped while offline");
                return;
            }
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
    let (ids, place) = if let (Some(album), Some(album_artist)) = (album, album_artist) {
        (
            reprise_core::queries::query_album_canonical_track_ids(db, album, album_artist)
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "quick-open album context query failed");
                    vec![track_id]
                }),
            BrowserPlace::fresh_album(album, album_artist),
        )
    } else if let Some(artist) = artist {
        (
            reprise_core::queries::query_artist_canonical_track_ids(db, artist).unwrap_or_else(
                |error| {
                    tracing::warn!(%error, "quick-open artist context query failed");
                    vec![track_id]
                },
            ),
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
    let start = ids.iter().position(|id| *id == track_id).unwrap_or(0);
    let origin = crate::ui::playback::play_origin::resolve(db, &place);
    player.play_from_view(ids, start, origin);
}

fn show_all(
    kind: QuickOpenKind,
    query: &str,
    navigator: &super::super::metadata_navigation::MetadataNavigator,
    section_search: &Rc<super::section_search_ui::SectionSearch>,
) {
    let (target, scope) = match kind {
        QuickOpenKind::Podcast => (SidebarTarget::Podcasts, SearchScope::Podcasts),
        QuickOpenKind::Radio => (SidebarTarget::Radio, SearchScope::Radio),
        QuickOpenKind::Track
        | QuickOpenKind::Album
        | QuickOpenKind::Artist
        | QuickOpenKind::Playlist => (SidebarTarget::Music, SearchScope::Tracks),
    };
    navigator.navigate(NavigationIntent::Sidebar(target), "quick open show all");
    let query = query.to_owned();
    let section_search = section_search.clone();
    gtk4::glib::idle_add_local_once(move || section_search.set_query(scope, &query));
}
