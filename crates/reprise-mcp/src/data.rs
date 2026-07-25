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
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;
use serde::Serialize;

use crate::capability;
use crate::dto::{
    BatchProgressDto, CreateInstrumentalResult, CreatePlaylistResult, InstrumentalJobDto,
    JobStatusDto, JobStatusResult, LibrarySummary, PlaylistDto, PlaylistsResult,
    SearchTracksResult, TrackDto,
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
/// Sourced from `reprise_core::stem_separation::CURRENT_MODEL_ID`, the single
/// canonical constant the app, the CLI and this server all share, so
/// MCP-enqueued jobs dedup against jobs from every other surface (the earlier
/// gap — each frontend hardcoding the id and risking drift — is now closed).
/// Kept behind one function so a future model bump stays a one-line change.
fn instrumental_model_id() -> &'static str {
    reprise_core::stem_separation::CURRENT_MODEL_ID
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

    Ok(LibrarySummary {
        track_count: stats.track_count,
        artist_count,
        album_count,
        total_duration_ms: stats.total_duration_ms,
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

#[derive(Debug, Serialize)]
pub struct ConcertsResource {
    events: Vec<ConcertResourceEvent>,
    filter_applied: bool,
    latest_fetch_at: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ConcertResourceEvent {
    date: String,
    starts_at: String,
    artist: String,
    venue: String,
    city: String,
    region: Option<String>,
    country: Option<String>,
    distance_km: Option<f64>,
    ticket_url: Option<String>,
    ticket_source: Option<String>,
    event_url: Option<String>,
    provider: String,
    is_similar: bool,
    similar_to: Option<String>,
}

/// Upcoming concerts after the saved filters, with no filesystem paths.
pub fn list_concerts(path: &Path) -> Result<ConcertsResource, DataError> {
    let conn = open(path)?;
    require_read(&conn)?;
    let filter = reprise_core::concerts::config::persisted_filter(&conn).map_err(DataError::Db)?;
    let location = reprise_core::concerts::config::location(&conn).map_err(DataError::Db)?;
    let events = reprise_core::concerts::query_events(
        &conn,
        &filter,
        location.as_ref(),
        chrono::Local::now().date_naive(),
    )
    .map_err(DataError::Db)?
    .into_iter()
    .map(|event| ConcertResourceEvent {
        date: event.date_key,
        starts_at: event.starts_at,
        artist: event.artist_name,
        venue: event.venue,
        city: event.city,
        region: event.region,
        country: event.country,
        distance_km: event.distance_km,
        ticket_url: event.ticket_url,
        ticket_source: event.ticket_source,
        event_url: event.event_url,
        provider: event.provider,
        is_similar: event.is_similar,
        similar_to: event.similar_to,
    })
    .collect();
    Ok(ConcertsResource {
        events,
        filter_applied: true,
        latest_fetch_at: reprise_core::concerts::latest_fetch_at(&conn).map_err(DataError::Db)?,
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
/// 16). `save` (default true) routes the batch: `true` persists the auto-promote
/// intent so the completion path files each finished render into the library
/// without a manual save (the automation wants the saved result); `false` routes
/// through the Conversion staging playlist (ensuring it exists) so the render
/// awaits an explicit save/discard decision (Beschluss 15). Either way the
/// enqueue lands atomically with its `change_log` events, so a running app shows
/// the new jobs live.
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
        // save=true carries the auto-promote intent: the completion path files
        // the finished render into the library without a manual save (Beschluss
        // 15, the automation default).
        ai_jobs::enqueue_instrumental_batch(&conn, &staging, track_ids, model_id, true, now)
            .map_err(DataError::Db)?
    } else {
        // save=false routes through the Conversion staging playlist with no
        // auto-promote intent, so each render awaits an explicit save/discard.
        ai_conversion::add_batch_to_conversion(&conn, &staging, track_ids, model_id, false, now)
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
            " Each finished render is then saved into your library automatically — \
             no manual save step is needed.",
        );
    } else {
        hint.push_str(" Each finished render waits in the Conversion view to save or discard.");
    }
    hint
}

/// Reports the status of instrumental jobs by explicit ids and/or a batch id
/// (plan 3.2). Read-only job metadata, so it needs only `library:read`. The
/// response is strictly the D19 allow-list — states, progress, result track ids
/// and timestamps, never a source/render path or a staging location.
pub fn job_status(
    db_path: &Path,
    job_ids: &[i64],
    batch_id: Option<&str>,
) -> Result<JobStatusResult, DataError> {
    let conn = open(db_path)?;
    require_read(&conn)?;

    if job_ids.is_empty() && batch_id.is_none() {
        return Err(DataError::InvalidInput(
            "provide job_ids or a batch_id".to_string(),
        ));
    }
    if job_ids.len() > MAX_TRACK_IDS {
        return Err(DataError::InvalidInput(format!(
            "too many job ids: {} (maximum {MAX_TRACK_IDS})",
            job_ids.len()
        )));
    }

    // A BTreeMap keyed on job id gives a stable id-ordered result and folds the
    // batch and the explicit ids into one set without duplicates.
    let mut by_id: std::collections::BTreeMap<i64, JobStatusDto> =
        std::collections::BTreeMap::new();

    let batch = match batch_id {
        Some(batch_id) => {
            for job in ai_jobs::list_jobs_in_batch(&conn, batch_id).map_err(DataError::Db)? {
                by_id.insert(job.id, JobStatusDto::from(&job));
            }
            let progress = ai_jobs::batch_progress(&conn, batch_id).map_err(DataError::Db)?;
            Some(BatchProgressDto {
                batch_id: batch_id.to_string(),
                total: progress.total,
                done: progress.done,
                failed: progress.failed,
                cancelled: progress.cancelled,
                running: progress.running,
                queued: progress.queued,
                permille: progress.permille,
            })
        }
        None => None,
    };

    for &id in job_ids {
        if by_id.contains_key(&id) {
            continue;
        }
        if let Some(job) = ai_jobs::get_job(&conn, id).map_err(DataError::Db)? {
            by_id.insert(id, JobStatusDto::from(&job));
        }
    }

    Ok(JobStatusResult {
        jobs: by_id.into_values().collect(),
        batch,
    })
}

/// Whether playback control is currently permitted (the live
/// `playback:control` setting, no startup snapshot — starting/stopping audio
/// destroys no data, so a fresh grant applies immediately, unlike the
/// write-class capabilities gated by [`capability::write_effective`]/
/// [`capability::ai_create_effective`]). Only the `mpris`-gated playback tools
/// call this, so it is gated the same way.
#[cfg(feature = "mpris")]
pub fn playback_allowed(path: &Path) -> Result<bool, DataError> {
    let conn = open(path)?;
    capability::playback_control_enabled(&conn).map_err(DataError::Db)
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
            let conn = open(path)?;
            require_read(&conn)?;
            playlists::track_ids(&conn, playlist_id).map_err(DataError::Db)?
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
mod tests {
    use super::*;
    use crate::dto::PlayParams;

    /// Seeds one real track row via the actual scanner (`reprise_core::
    /// library::scanner::scan_folder`) over a temp copy of the shared
    /// `sine.flac` fixture, then reads its assigned id back through the
    /// existing `queries::query_track_window` facade — never a raw SQL
    /// literal. `resolve_play_ids`'s playlist path needs a track that is
    /// genuinely present in `tracks` (the `playlist_tracks.track_id` foreign
    /// key is enforced, per `db::open`'s `PRAGMA foreign_keys = ON`), and
    /// `scripts/check-architecture.sh`'s "no SQL outside reprise-core" gate
    /// scans all of `crates/reprise-mcp/src` verbatim — `#[cfg(test)]` blocks
    /// included, unlike the `tests/` integration fixtures it explicitly
    /// exempts — so a hand-written literal `tracks`-table insert here would
    /// trip it even though it never ships in the binary.
    fn scan_one_track(db_path: &Path) -> i64 {
        let mut conn = reprise_core::db::open_migrated(Some(db_path)).unwrap();

        let library_root = tempfile::tempdir().unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        std::fs::copy(&fixture, library_root.path().join("sine.flac")).unwrap();
        reprise_core::library::scanner::scan_folder(&mut conn, library_root.path()).unwrap();

        let source = ViewSource::Library;
        let tracks =
            queries::query_track_window(&mut conn, &source, "title", "asc", "", 0, 10, &[])
                .unwrap();
        assert_eq!(tracks.len(), 1, "expected exactly one scanned track");
        tracks[0].id
    }

    #[test]
    fn resolve_play_ids_enforces_exactly_one_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let track_id = scan_one_track(&path);

        let mut conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
        let pid = playlists::create(&conn, "P").unwrap();
        playlists::add_tracks(&mut conn, pid, &[track_id]).unwrap();
        drop(conn);

        // playlist path
        let ids = resolve_play_ids(
            &path,
            &PlayParams {
                track_ids: None,
                playlist_id: Some(pid),
            },
        )
        .unwrap();
        assert_eq!(ids, vec![track_id]);
        // explicit ids path
        let ids = resolve_play_ids(
            &path,
            &PlayParams {
                track_ids: Some(vec![track_id]),
                playlist_id: None,
            },
        )
        .unwrap();
        assert_eq!(ids, vec![track_id]);
        // neither
        assert!(matches!(
            resolve_play_ids(
                &path,
                &PlayParams {
                    track_ids: None,
                    playlist_id: None,
                }
            ),
            Err(DataError::InvalidInput(_))
        ));
        // both
        assert!(matches!(
            resolve_play_ids(
                &path,
                &PlayParams {
                    track_ids: Some(vec![track_id]),
                    playlist_id: Some(pid),
                }
            ),
            Err(DataError::InvalidInput(_))
        ));
        // empty playlist
        let empty =
            playlists::create(&reprise_core::db::open_migrated(Some(&path)).unwrap(), "E").unwrap();
        assert!(matches!(
            resolve_play_ids(
                &path,
                &PlayParams {
                    track_ids: None,
                    playlist_id: Some(empty),
                }
            ),
            Err(DataError::InvalidInput(_))
        ));
    }
}
