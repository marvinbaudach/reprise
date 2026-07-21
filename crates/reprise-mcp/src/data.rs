//! Synchronous database access, run off the async runtime via
//! `spawn_blocking`. Every function here opens its own short-lived
//! `rusqlite::Connection` (the "stateless reader per call" model of the
//! multi-frontend-core plan) and reaches the library **only** through
//! `reprise-core` facades — this crate contains no SQL of its own.

use std::path::Path;

use reprise_core::audio_analysis::CURRENT_EXTRACTOR_VERSION;
use reprise_core::db::{self, DbError};
use reprise_core::library::playlists;
use reprise_core::models::Track;
use reprise_core::queries;
use reprise_core::sound_profile::{self, AnalysisVersions, CURRENT_PROFILE_VERSION};
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use crate::capability;
use crate::dto::{
    CreatePlaylistResult, LibrarySummary, PlaylistDto, PlaylistsResult, SearchTracksResult,
    TrackDto,
};

/// Default page size when a search omits `limit`.
pub const DEFAULT_SEARCH_LIMIT: i64 = 50;
/// Hard ceiling on a search page (spec §6: MCP responses are hard-limited).
pub const MAX_SEARCH_LIMIT: i64 = 200;
/// Maximum explicit track ids accepted by `music_create_playlist` (spec limit).
pub const MAX_PLAYLIST_TRACKS: usize = 500;

// Fixed sort for search — a stable, predictable order for a metadata lookup.
const SEARCH_SORT_FIELD: &str = "title";
const SEARCH_SORT_DIR: &str = "asc";

/// A failure while serving a request. The `error`/`server`-facing variants are
/// logged and mapped to opaque protocol errors (never leaked); the
/// caller-fixable variants become caller-visible tool errors.
#[derive(Debug)]
pub enum DataError {
    /// A query failed.
    Db(rusqlite::Error),
    /// The database could not be opened.
    Open(DbError),
    /// An internal invariant failed (path-free message; logged, not leaked).
    Internal(String),
    /// The required capability is not granted.
    CapabilityDenied(&'static str),
    /// Caller input was invalid (caller-visible message).
    InvalidInput(String),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(error) => write!(f, "database error: {error}"),
            Self::Open(error) => write!(f, "database open error: {error}"),
            Self::Internal(message) => write!(f, "internal error: {message}"),
            Self::CapabilityDenied(capability) => {
                write!(f, "capability denied: {capability}")
            }
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
        }
    }
}

fn open(path: &Path) -> Result<Connection, DataError> {
    // The database was migrated once at startup; per call we open a plain
    // connection (WAL, `busy_timeout`, `foreign_keys` all set by `db::open`)
    // without re-running migrations or the change-log prune, so a read stays a
    // read and does not contend on the write lock.
    db::open(Some(path)).map_err(DataError::Open)
}

fn require_read(conn: &Connection) -> Result<(), DataError> {
    if capability::library_read_enabled(conn).map_err(DataError::Db)? {
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
    let mut conn = open(path)?;
    require_read(&conn)?;

    let limit = resolve_limit(limit);
    let offset = i64::from(offset.unwrap_or(0));
    let source = ViewSource::Library;

    let total = queries::query_track_count(&conn, &source, query, &[]).map_err(DataError::Db)?;
    let tracks: Vec<Track> = queries::query_track_window(
        &mut conn,
        &source,
        SEARCH_SORT_FIELD,
        SEARCH_SORT_DIR,
        query,
        offset,
        limit,
        &[],
    )
    .map_err(DataError::Db)?;

    let dtos: Vec<TrackDto> = tracks.iter().map(TrackDto::from).collect();
    let returned = dtos.len();
    let has_more = offset.saturating_add(returned as i64) < total;

    Ok(SearchTracksResult {
        tracks: dtos,
        total,
        offset,
        limit,
        returned,
        has_more,
    })
}

/// Library-wide summary for the `reprise://library/summary` resource.
pub fn library_summary(path: &Path) -> Result<LibrarySummary, DataError> {
    let conn = open(path)?;
    require_read(&conn)?;

    let stats = queries::query_library_stats(&conn, "").map_err(DataError::Db)?;
    let artist_count = queries::query_artists(&conn).map_err(DataError::Db)?.len() as i64;
    let album_count = queries::query_albums(&conn).map_err(DataError::Db)?.len() as i64;

    let versions = AnalysisVersions::new(CURRENT_EXTRACTOR_VERSION, CURRENT_PROFILE_VERSION)
        .map_err(|error| DataError::Internal(format!("invalid analysis versions: {error}")))?;
    let coverage = sound_profile::library_coverage(&conn, versions)
        .map_err(|error| DataError::Internal(format!("coverage query failed: {error}")))?;

    Ok(LibrarySummary {
        track_count: stats.track_count,
        artist_count,
        album_count,
        total_duration_ms: stats.total_duration_ms,
        analyzed_tracks: coverage.analyzed,
        analysis_total: coverage.total,
    })
}

/// Playlist listing for the `reprise://playlists` resource.
pub fn list_playlists(path: &Path) -> Result<PlaylistsResult, DataError> {
    let conn = open(path)?;
    require_read(&conn)?;

    let summaries = playlists::list(&conn).map_err(DataError::Db)?;
    let playlists = summaries.iter().map(PlaylistDto::from).collect();
    Ok(PlaylistsResult { playlists })
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
    let mut conn = open(path)?;

    if !capability::write_effective(&conn, write_granted_at_startup).map_err(DataError::Db)? {
        return Err(DataError::CapabilityDenied("playlist:create"));
    }

    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DataError::InvalidInput(
            "playlist name must not be empty".to_string(),
        ));
    }
    if track_ids.len() > MAX_PLAYLIST_TRACKS {
        return Err(DataError::InvalidInput(format!(
            "too many track ids: {} (maximum {MAX_PLAYLIST_TRACKS})",
            track_ids.len()
        )));
    }

    let playlist_id =
        playlists::create_with_tracks(&mut conn, trimmed, track_ids).map_err(map_create_error)?;

    Ok(CreatePlaylistResult {
        playlist_id,
        name: trimmed.to_string(),
        track_count: track_ids.len(),
    })
}

// A `playlist_tracks.track_id` foreign-key violation means a supplied id does
// not exist; surface it as caller-fixable input rather than an opaque internal
// error. (Enforcing full PRESENT semantics — rejecting ids that exist but are
// missing_since != NULL — needs a core facade that does not exist yet; see the
// package-B report.)
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
