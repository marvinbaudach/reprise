//! Synchronous database access, run off the async runtime via
//! `spawn_blocking`. Every function here opens its own short-lived
//! [`reprise_core::db::Db`] handle (the "stateless reader per call" model of the
//! multi-frontend-core plan) and reaches the library **only** through
//! `reprise-core` facades — this crate contains no SQL of its own.

use std::path::Path;

use reprise_core::db::Db;
use reprise_core::db::DbError;
use reprise_core::library::playlists;
use reprise_core::queries;
use reprise_core::view_source::ViewSource;

use crate::capability;
pub(crate) use crate::data_concerts::list_concerts;
use crate::dto::{
    AlbumDto, ArtistDto, CreatePlaylistResult, LibrarySummary, PlaylistContentsResult, PlaylistDto,
    PlaylistsResult, SearchAlbumsResult, SearchArtistsResult, SearchTracksResult, TrackDto,
};

/// Default page size when a search omits `limit`.
pub const DEFAULT_SEARCH_LIMIT: i64 = 50;
/// Hard ceiling on a search page (spec §6: MCP responses are hard-limited).
pub const MAX_SEARCH_LIMIT: i64 = 200;
/// Maximum explicit track ids accepted by a write tool (spec limit).
pub const MAX_TRACK_IDS: usize = 500;
const SUMMARY_WINDOW_SIZE: i64 = 500;

/// A failure while serving a request. The `error`/`server`-facing variants are
/// logged and mapped to opaque protocol errors (never leaked); the
/// caller-fixable variants become caller-visible tool errors.
#[derive(Debug)]
pub enum DataError {
    /// A query failed.
    Db(rusqlite::Error),
    /// The database could not be opened.
    Open(DbError),
    /// The required capability is not granted.
    CapabilityDenied(&'static str),
    /// Caller input was invalid (caller-visible message).
    InvalidInput(String),
    /// A tag-write job in another surface currently owns the shared slot.
    TagWriteBusy,
    /// A Library Doctor invariant failed. Logged, never exposed to callers.
    Internal(String),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(error) => write!(f, "database error: {error}"),
            Self::Open(error) => write!(f, "database open error: {error}"),
            Self::CapabilityDenied(capability) => {
                write!(f, "capability denied: {capability}")
            }
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::TagWriteBusy => write!(f, "another tag-writing job is already running"),
            Self::Internal(message) => write!(f, "internal error: {message}"),
        }
    }
}

pub(crate) fn open(path: &Path) -> Result<Db, DataError> {
    Db::open_ready(path).map_err(DataError::Open)
}

pub(crate) fn require_read(db: &Db) -> Result<(), DataError> {
    if capability::library_read_enabled(db).map_err(DataError::Db)? {
        Ok(())
    } else {
        Err(DataError::CapabilityDenied("library:read"))
    }
}

fn resolve_limit(limit: Option<u32>) -> i64 {
    match limit {
        None => DEFAULT_SEARCH_LIMIT,
        Some(requested) => i64::from(requested).clamp(1, MAX_SEARCH_LIMIT),
    }
}

/// Paginated, read-only metadata search over the present library.
pub fn search_tracks(
    path: &Path,
    query: &str,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<SearchTracksResult, DataError> {
    let db = open(path)?;
    require_read(&db)?;

    let limit = resolve_limit(limit);
    let offset = i64::from(offset.unwrap_or(0));
    let window = queries::query_library_metadata_text_search(
        &db,
        query,
        queries::WindowRange { offset, limit },
    )
    .map_err(DataError::Db)?;
    let total = window.total;
    let has_more = window.has_more;
    let dtos: Vec<TrackDto> = window.rows.iter().map(TrackDto::from).collect();
    let returned = dtos.len();

    Ok(SearchTracksResult {
        tracks: dtos,
        total,
        offset,
        limit,
        returned,
        has_more,
    })
}

fn all_artist_summaries(db: &Db) -> Result<Vec<queries::ArtistSummary>, DataError> {
    let mut offset = 0;
    let mut rows = Vec::new();
    loop {
        let window = queries::query_artists(
            db,
            "",
            queries::WindowRange {
                offset,
                limit: SUMMARY_WINDOW_SIZE,
            },
        )
        .map_err(DataError::Db)?;
        let returned = i64::try_from(window.rows.len()).unwrap_or(i64::MAX);
        if returned == 0 && window.has_more {
            return Err(DataError::Db(rusqlite::Error::InvalidQuery));
        }
        rows.extend(window.rows);
        if !window.has_more {
            return Ok(rows);
        }
        offset = offset.saturating_add(returned);
    }
}

fn all_album_summaries(db: &Db) -> Result<Vec<queries::AlbumSummary>, DataError> {
    let mut offset = 0;
    let mut rows = Vec::new();
    loop {
        let window = queries::query_albums(
            db,
            "",
            queries::WindowRange {
                offset,
                limit: SUMMARY_WINDOW_SIZE,
            },
        )
        .map_err(DataError::Db)?;
        let returned = i64::try_from(window.rows.len()).unwrap_or(i64::MAX);
        if returned == 0 && window.has_more {
            return Err(DataError::Db(rusqlite::Error::InvalidQuery));
        }
        rows.extend(window.rows);
        if !window.has_more {
            return Ok(rows);
        }
        offset = offset.saturating_add(returned);
    }
}

/// Paginated artist discovery using the same effective-album-artist grouping
/// as the native Artists view.
pub fn search_artists(
    path: &Path,
    query: &str,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<SearchArtistsResult, DataError> {
    let db = open(path)?;
    require_read(&db)?;

    let needle = query.trim().to_lowercase();
    let matching: Vec<_> = all_artist_summaries(&db)?
        .into_iter()
        .filter(|artist| artist.artist.to_lowercase().contains(&needle))
        .collect();
    let total = matching.len();
    let limit = resolve_limit(limit);
    let offset = i64::from(offset.unwrap_or(0));
    let artists: Vec<ArtistDto> = matching
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(ArtistDto::from)
        .collect();
    let returned = artists.len();
    let has_more = offset.saturating_add(returned as i64) < total as i64;

    Ok(SearchArtistsResult {
        artists,
        total,
        offset,
        limit,
        returned,
        has_more,
    })
}

/// Paginated album discovery using the native Albums view's grouping and
/// stable ordering.
pub fn search_albums(
    path: &Path,
    query: &str,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<SearchAlbumsResult, DataError> {
    let db = open(path)?;
    require_read(&db)?;

    let needle = query.trim().to_lowercase();
    let matching: Vec<_> = all_album_summaries(&db)?
        .into_iter()
        .filter(|album| {
            album.album.to_lowercase().contains(&needle)
                || album.album_artist.to_lowercase().contains(&needle)
        })
        .collect();
    let total = matching.len();
    let limit = resolve_limit(limit);
    let offset = i64::from(offset.unwrap_or(0));
    let albums: Vec<AlbumDto> = matching
        .iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(AlbumDto::from)
        .collect();
    let returned = albums.len();
    let has_more = offset.saturating_add(returned as i64) < total as i64;

    Ok(SearchAlbumsResult {
        albums,
        total,
        offset,
        limit,
        returned,
        has_more,
    })
}

/// Library-wide summary for the `reprise://library/summary` resource.
pub fn library_summary(path: &Path) -> Result<LibrarySummary, DataError> {
    let db = open(path)?;
    require_read(&db)?;

    let stats = queries::query_library_stats(&db, "").map_err(DataError::Db)?;
    let artist_count = queries::query_artist_count(&db, "").map_err(DataError::Db)?;
    let album_count = queries::query_album_count(&db, "").map_err(DataError::Db)?;

    Ok(LibrarySummary {
        track_count: stats.track_count,
        artist_count,
        album_count,
        total_duration_ms: stats.total_duration_ms,
    })
}

/// Playlist listing for the `reprise://playlists` resource.
pub fn list_playlists(path: &Path) -> Result<PlaylistsResult, DataError> {
    let db = open(path)?;
    require_read(&db)?;

    let summaries = playlists::list(&db).map_err(DataError::Db)?;
    let playlists = summaries.iter().map(PlaylistDto::from).collect();
    Ok(PlaylistsResult { playlists })
}

/// Reads a page of one manual playlist in durable playlist order. Membership
/// rows are returned even when their files are currently unavailable, matching
/// the native playlist view.
pub fn playlist_contents(
    path: &Path,
    playlist_id: i64,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<PlaylistContentsResult, DataError> {
    let db = open(path)?;
    require_read(&db)?;

    let summary = playlists::get(&db, playlist_id)
        .map_err(DataError::Db)?
        .ok_or_else(|| DataError::InvalidInput("playlist does not exist".to_owned()))?;
    let source = ViewSource::Playlist(playlist_id);
    let total = queries::query_track_count(&db, &source, "", &[]).map_err(DataError::Db)?;
    let limit = resolve_limit(limit);
    let offset = i64::from(offset.unwrap_or(0));
    let tracks = queries::query_track_window(
        &db,
        &source,
        "playlist_order",
        "asc",
        "",
        offset,
        limit,
        &[],
    )
    .map_err(DataError::Db)?;
    let tracks: Vec<TrackDto> = tracks.iter().map(TrackDto::from).collect();
    let returned = tracks.len();
    let has_more = offset.saturating_add(returned as i64) < total;

    Ok(PlaylistContentsResult {
        playlist: PlaylistDto::from(&summary),
        tracks,
        total,
        offset,
        limit,
        returned,
        has_more,
    })
}

/// Creates a new manual playlist from explicit track ids.
///
/// Capability is re-read here (immediate revocation) and combined with the
/// startup snapshot (a fresh grant needs a restart). The playlist and its
/// `change_log` row land atomically in the core facade's transaction, so a
/// running app sees the new playlist live.
pub fn create_playlist(
    path: &Path,
    write_granted_at_startup: bool,
    name: &str,
    track_ids: &[i64],
) -> Result<CreatePlaylistResult, DataError> {
    let db = open(path)?;
    if !capability::write_effective(&db, write_granted_at_startup).map_err(DataError::Db)? {
        return Err(DataError::CapabilityDenied("playlist:create"));
    }

    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DataError::InvalidInput(
            "playlist name must not be empty".to_string(),
        ));
    }
    if track_ids.len() > MAX_TRACK_IDS {
        return Err(DataError::InvalidInput(format!(
            "too many track ids: {} (maximum {MAX_TRACK_IDS})",
            track_ids.len()
        )));
    }
    reject_absent_track_ids(&db, track_ids)?;

    let playlist_id =
        playlists::create_with_tracks(&db, trimmed, track_ids).map_err(map_create_error)?;

    Ok(CreatePlaylistResult {
        playlist_id,
        name: trimmed.to_string(),
        track_count: track_ids.len(),
    })
}

/// Enforces PRESENT semantics before a write: rejects ids that are not present
/// in the library — either no row at all, or a row whose file is currently
/// missing (a plain foreign-key check would let a missing row through). Lists
/// the offending ids so the caller can correct its request. Shared by the
/// playlist and live-queue mutation paths.
pub(crate) fn reject_absent_track_ids(db: &Db, track_ids: &[i64]) -> Result<(), DataError> {
    if track_ids.is_empty() {
        return Ok(());
    }
    let present: std::collections::HashSet<i64> = queries::filter_present(db, track_ids)
        .map_err(DataError::Db)?
        .into_iter()
        .collect();
    let mut seen = std::collections::HashSet::new();
    let offending: Vec<i64> = track_ids
        .iter()
        .copied()
        .filter(|id| !present.contains(id) && seen.insert(*id))
        .collect();
    if !offending.is_empty() {
        let list = offending
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(DataError::InvalidInput(format!(
            "one or more track ids are not present in the library: {list}"
        )));
    }
    Ok(())
}

/// Validates explicit queue ids against the current PRESENT library view.
///
/// Queue mutation is authorized by `playback:control`, so this check does not
/// additionally require `library:read`; it only prevents callers from
/// inserting unknown or currently missing rows into the live queue.
#[cfg(feature = "mpris")]
pub fn validate_present_track_ids(path: &Path, track_ids: &[i64]) -> Result<(), DataError> {
    let db = open(path)?;
    reject_absent_track_ids(&db, track_ids)
}

/// Whether playback control is currently permitted (the live
/// `playback:control` setting, no startup snapshot — starting/stopping audio
/// destroys no data, so a fresh grant applies immediately, unlike the
/// write-class capabilities gated by [`capability::write_effective`]). Only
/// the `mpris`-gated playback tools
/// call this, so it is gated the same way.
#[cfg(feature = "mpris")]
pub fn playback_allowed(path: &Path) -> Result<bool, DataError> {
    let db = open(path)?;
    capability::playback_control_enabled(&db).map_err(DataError::Db)
}

#[cfg(feature = "mpris")]
pub fn device_sync_allowed(path: &Path, granted_at_startup: bool) -> Result<bool, DataError> {
    let db = open(path)?;
    capability::device_sync_effective(&db, granted_at_startup).map_err(DataError::Db)
}

/// Resolves a `music_play` request to an ordered id list. Exactly one of
/// `track_ids`/`playlist_id` must be set; a playlist is resolved to its tracks
/// via `playlists::track_ids` (in playlist order); an empty/absent result is
/// invalid input (nothing to play). Read-only (`library:read`), like
/// `search_tracks`/`list_playlists` — `music_play` itself, not this
/// resolution step, is what the running app actually plays. Only the
/// `mpris`-gated `music_play` tool calls this, so it is gated the same way.
#[cfg(feature = "mpris")]
pub fn resolve_play_ids(
    path: &Path,
    params: &crate::dto::PlayParams,
) -> Result<Vec<i64>, DataError> {
    let ids = match (&params.track_ids, params.playlist_id) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(DataError::InvalidInput(
                "provide exactly one of track_ids or playlist_id".to_owned(),
            ));
        }
        (Some(track_ids), None) => track_ids.clone(),
        (None, Some(playlist_id)) => {
            let db = open(path)?;
            require_read(&db)?;
            playlists::track_ids(&db, playlist_id).map_err(DataError::Db)?
        }
    };
    if ids.is_empty() {
        return Err(DataError::InvalidInput(
            "no playable tracks to play".to_owned(),
        ));
    }
    Ok(ids)
}

// PRESENT semantics are enforced up front via `queries::filter_present`, so by
// the time `create_with_tracks` runs every id was present. A `playlist_tracks.
// track_id` foreign-key violation here is therefore only a rare race (a track
// hard-deleted between the check and the insert); surface it as caller-fixable
// input rather than an opaque internal error.
fn map_create_error(error: rusqlite::Error) -> DataError {
    if is_constraint_violation(&error) {
        DataError::InvalidInput("one or more track ids do not exist in the library".to_string())
    } else {
        DataError::Db(error)
    }
}

fn is_constraint_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

#[cfg(all(test, feature = "mpris"))]
#[path = "data_tests.rs"]
mod tests;
