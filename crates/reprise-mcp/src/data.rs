//! Synchronous database access, run off the async runtime via
//! `spawn_blocking`. Every function here opens its own short-lived
//! `rusqlite::Connection` (the "stateless reader per call" model of the
//! multi-frontend-core plan) and reaches the library **only** through
//! `reprise-core` facades — this crate contains no SQL of its own.

use std::path::Path;

use reprise_core::ai_conversion;
use reprise_core::ai_jobs::{self, EnqueueOutcome};
use reprise_core::ai_staging::StagingStore;
use reprise_core::db::{self, DbError};
use reprise_core::library::playlists;
use reprise_core::models::Track;
use reprise_core::queries;
use reprise_core::sound_profile::{self, AnalysisVersions};
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use crate::capability;
use crate::dto::{
    CreateInstrumentalResult, CreatePlaylistResult, InstrumentalJobDto, LibrarySummary,
    PlaylistDto, PlaylistsResult, SearchTracksResult, TrackDto,
};

/// Default page size when a search omits `limit`.
pub const DEFAULT_SEARCH_LIMIT: i64 = 50;
/// Hard ceiling on a search page (spec §6: MCP responses are hard-limited).
pub const MAX_SEARCH_LIMIT: i64 = 200;
/// Maximum explicit track ids accepted by a write tool (`music_create_playlist`
/// and `music_create_instrumental` share the same spec limit).
pub const MAX_TRACK_IDS: usize = 500;

// Fixed sort for search — a stable, predictable order for a metadata lookup.
const SEARCH_SORT_FIELD: &str = "title";
const SEARCH_SORT_DIR: &str = "asc";

/// Model id stamped on every instrumental job MCP enqueues — the dedup
/// fingerprint and the `REPRISE_AI_MODEL` provenance tag (Beschluss 16, plan
/// 2.4/5).
///
/// **Known gap:** `reprise-core` exposes no canonical "current instrumental
/// model id" for the non-worker enqueuers (the app context menu, the CLI and
/// this server) to share, and MCP must not link `reprise-stems` to read it from
/// the backend. So this is a placeholder that matches the value the core
/// promotion tests treat as canonical (the spike's htdemucs choice). Until core
/// provides a shared source (a `stem_separation` constant, or a settings key
/// written at model-download time), MCP-enqueued jobs only dedup against jobs
/// enqueued with this same id. Routed through one function so that becomes a
/// one-line change.
const INSTRUMENTAL_MODEL_ID: &str = "htdemucs@4";

fn instrumental_model_id() -> &'static str {
    INSTRUMENTAL_MODEL_ID
}

/// Wall-clock seconds since the Unix epoch — the injected `now` the job facades
/// take. A backwards clock degrades to 0 rather than panicking.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

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
    let artist_count = queries::query_artist_count(&conn).map_err(DataError::Db)?;
    let album_count = queries::query_album_count(&conn).map_err(DataError::Db)?;

    let coverage = sound_profile::library_coverage(&conn, AnalysisVersions::current())
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
    if track_ids.len() > MAX_TRACK_IDS {
        return Err(DataError::InvalidInput(format!(
            "too many track ids: {} (maximum {MAX_TRACK_IDS})",
            track_ids.len()
        )));
    }
    reject_absent_track_ids(&conn, track_ids)?;

    let playlist_id =
        playlists::create_with_tracks(&mut conn, trimmed, track_ids).map_err(map_create_error)?;

    Ok(CreatePlaylistResult {
        playlist_id,
        name: trimmed.to_string(),
        track_count: track_ids.len(),
    })
}

/// Enforces PRESENT semantics before a write: rejects ids that are not present
/// in the library — either no row at all, or a row whose file is currently
/// missing (a plain foreign-key check would let a missing row through). Lists
/// the offending ids so the caller can correct its request. Shared by
/// `music_create_playlist` and `music_create_instrumental`.
fn reject_absent_track_ids(conn: &Connection, track_ids: &[i64]) -> Result<(), DataError> {
    if track_ids.is_empty() {
        return Ok(());
    }
    let present: std::collections::HashSet<i64> = queries::filter_present(conn, track_ids)
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

/// Registers one instrumental (vocal-removal) job per source track and returns
/// immediately with the job/batch ids — rendering happens later in a worker
/// (plan 3.2). Capability is re-read here (immediate revocation) combined with
/// the startup snapshot (a fresh grant needs a restart). Tracks already covered
/// by an open, staged, or saved job are referenced, not re-rendered (Beschluss
/// 16). `save` (default true) routes the batch: `true` enqueues directly (the
/// automation wants the saved result), `false` also ensures the Conversion
/// staging playlist exists so the render awaits an explicit save/discard
/// decision (Beschluss 15). Either way the enqueue lands atomically with its
/// `change_log` events, so a running app shows the new jobs live.
pub fn create_instrumental(
    db_path: &Path,
    staging_path: &Path,
    ai_granted_at_startup: bool,
    track_ids: &[i64],
    save: bool,
) -> Result<CreateInstrumentalResult, DataError> {
    let conn = open(db_path)?;

    if !capability::ai_create_effective(&conn, ai_granted_at_startup).map_err(DataError::Db)? {
        return Err(DataError::CapabilityDenied("ai:create"));
    }
    if track_ids.is_empty() {
        return Err(DataError::InvalidInput(
            "at least one track id is required".to_string(),
        ));
    }
    if track_ids.len() > MAX_TRACK_IDS {
        return Err(DataError::InvalidInput(format!(
            "too many track ids: {} (maximum {MAX_TRACK_IDS})",
            track_ids.len()
        )));
    }
    reject_absent_track_ids(&conn, track_ids)?;

    let staging = StagingStore::new(staging_path);
    let model_id = instrumental_model_id();
    let now = now_secs();
    let batch = if save {
        ai_jobs::enqueue_instrumental_batch(&conn, &staging, track_ids, model_id, now)
            .map_err(DataError::Db)?
    } else {
        ai_conversion::add_batch_to_conversion(&conn, &staging, track_ids, model_id, now)
            .map_err(DataError::Db)?
    };

    // `batch.jobs` is in input order, one per source track.
    let jobs: Vec<InstrumentalJobDto> = track_ids
        .iter()
        .zip(batch.jobs.iter())
        .map(|(&source_track_id, outcome)| match *outcome {
            EnqueueOutcome::Created { job_id } => InstrumentalJobDto {
                source_track_id,
                job_id,
                deduplicated: false,
                existing_instrumental_track_id: None,
            },
            EnqueueOutcome::Deduplicated {
                job_id,
                result_track_id,
            } => InstrumentalJobDto {
                source_track_id,
                job_id,
                deduplicated: true,
                existing_instrumental_track_id: result_track_id,
            },
        })
        .collect();
    let created = jobs.iter().filter(|job| !job.deduplicated).count();
    let deduplicated = jobs.len() - created;

    Ok(CreateInstrumentalResult {
        batch_id: batch.batch_id,
        save,
        created,
        deduplicated,
        queued_hint: queued_hint(created, deduplicated, save),
        jobs,
    })
}

/// Builds the honest, path-free `queued_hint`: how many jobs are queued, that
/// they stay queued until a worker (the running app or `reprise-cli jobs work`)
/// renders them, and where the finished render lands given `save` (plan 3.2:
/// "die MCP-Antwort sagt das ehrlich und nennt beide Abarbeitungswege").
fn queued_hint(created: usize, deduplicated: usize, save: bool) -> String {
    let mut hint = if created > 0 {
        format!("Queued {created} instrumental job(s).")
    } else {
        "No new jobs were queued.".to_string()
    };
    if deduplicated > 0 {
        hint.push_str(&format!(
            " {deduplicated} track(s) already had an instrumental or a pending job \
             and were referenced, not re-rendered."
        ));
    }
    hint.push_str(
        " Jobs stay queued until a worker renders them: the Reprise app while it \
         is open, or `reprise-cli jobs work`.",
    );
    if save {
        hint.push_str(
            " Finished renders can then be saved to your library from the app or \
             with `reprise-cli instrumental save`.",
        );
    } else {
        hint.push_str(" Each finished render waits in the Conversion view to save or discard.");
    }
    hint
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
