//! Leak-safe data-transfer objects and tool parameter types.
//!
//! Every response shape here is the D19 allow-list and nothing more: opaque
//! track ids plus display metadata (title, artist, album, year, genre, rating,
//! duration). A file path, XDG/cache/database path, lyric, device serial,
//! credential, or raw listen event must never appear on any of these structs —
//! the `leak_matrix` integration tests assert exactly that against live
//! responses. Mapping from the richer core types deliberately drops the
//! disallowed fields (e.g. [`Track::path`](reprise_core::models::Track::path)).

use reprise_core::ai_jobs::{AiJob, JobState};
use reprise_core::library::playlists::PlaylistSummary;
use reprise_core::models::Track;
use rmcp::schemars;
use serde::{Deserialize, Serialize};

/// Track metadata exposed to agents — the D19 allow-list only.
#[derive(Debug, Clone, Serialize)]
pub struct TrackDto {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: Option<i32>,
    pub genre: String,
    pub rating: i32,
    pub duration_ms: i64,
}

impl From<&Track> for TrackDto {
    fn from(track: &Track) -> Self {
        Self {
            id: track.id,
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            year: track.year,
            genre: track.genre.clone(),
            rating: track.rating,
            duration_ms: track.duration_ms,
        }
    }
}

/// A page of search results plus the cursor fields a client needs to paginate.
#[derive(Debug, Clone, Serialize)]
pub struct SearchTracksResult {
    pub tracks: Vec<TrackDto>,
    /// Total matches across the whole library (not just this page).
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
    pub returned: usize,
    pub has_more: bool,
}

/// The `reprise://library/summary` resource body.
#[derive(Debug, Clone, Serialize)]
pub struct LibrarySummary {
    pub track_count: i64,
    pub artist_count: i64,
    pub album_count: i64,
    pub total_duration_ms: i64,
}

/// One playlist in the `reprise://playlists` resource — no paths, per D17.
#[derive(Debug, Clone, Serialize)]
pub struct PlaylistDto {
    pub id: i64,
    pub name: String,
    pub track_count: i64,
}

impl From<&PlaylistSummary> for PlaylistDto {
    fn from(summary: &PlaylistSummary) -> Self {
        Self {
            id: summary.id,
            name: summary.name.clone(),
            track_count: summary.track_count,
        }
    }
}

/// The `reprise://playlists` resource body.
#[derive(Debug, Clone, Serialize)]
pub struct PlaylistsResult {
    pub playlists: Vec<PlaylistDto>,
}

/// The `music_create_playlist` result body.
#[derive(Debug, Clone, Serialize)]
pub struct CreatePlaylistResult {
    pub playlist_id: i64,
    pub name: String,
    pub track_count: usize,
}

/// Parameters for `music_search_tracks`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SearchTracksParams {
    /// Case-insensitive substring matched against title, artist, album and
    /// genre. Empty matches the whole library.
    #[serde(default)]
    pub query: String,
    /// Maximum tracks to return (1..=200, default 50).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Zero-based offset into the full result set (default 0).
    #[serde(default)]
    pub offset: Option<u32>,
}

/// Parameters for `music_create_playlist`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct CreatePlaylistParams {
    /// Human-readable playlist name; must be non-empty after trimming.
    pub name: String,
    /// Explicit, ordered track ids (duplicates allowed; at most 500).
    pub track_ids: Vec<i64>,
}

/// Serde default for [`CreateInstrumentalParams::save`] — Beschluss 15 defaults
/// the save intent to `true`.
pub fn default_true() -> bool {
    true
}

/// Parameters for `music_create_instrumental`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct CreateInstrumentalParams {
    /// Explicit source track ids to render instrumentals of (at least one, at
    /// most 500). Duplicates and tracks with existing work are de-duplicated
    /// against the job queue (Beschluss 16).
    pub track_ids: Vec<i64>,
    /// Whether each finished render is destined for the library (`true`,
    /// default) or should wait in the Conversion staging view for an explicit
    /// save/discard decision (`false`). Default true (Beschluss 15).
    #[serde(default = "default_true")]
    pub save: bool,
}

/// Parameters for `music_get_job_status`. Supply job ids, a batch id, or both;
/// at least one is required.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct JobStatusParams {
    /// Specific job ids to report on (at most 500).
    #[serde(default)]
    pub job_ids: Vec<i64>,
    /// A batch id (from `music_create_instrumental`) whose jobs to report on,
    /// with aggregate progress.
    #[serde(default)]
    pub batch_id: Option<String>,
}

/// One job's status — the strict D19 allow-list for job metadata: opaque ids,
/// state, progress and timestamps only. **Never** a source/render path or a
/// staging location (`AiJob` already omits `params_json`, `claimed_by`, and the
/// lease; this DTO drops nothing back in).
#[derive(Debug, Clone, Serialize)]
pub struct JobStatusDto {
    pub job_id: i64,
    pub kind: String,
    pub state: JobState,
    pub progress_permille: u16,
    pub batch_id: Option<String>,
    pub source_track_id: Option<i64>,
    /// The saved library track once the render was promoted; null while queued,
    /// running, or staged-but-unsaved.
    pub result_track_id: Option<i64>,
    pub cancel_requested: bool,
    /// A short diagnostic kind for a failed job (never a path).
    pub error_kind: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

impl From<&AiJob> for JobStatusDto {
    fn from(job: &AiJob) -> Self {
        Self {
            job_id: job.id,
            kind: job.kind.clone(),
            state: job.state,
            progress_permille: job.progress_permille,
            batch_id: job.batch_id.clone(),
            source_track_id: job.source_track_id,
            result_track_id: job.result_track_id,
            cancel_requested: job.cancel_requested,
            error_kind: job.error_kind.clone(),
            created_at: job.created_at,
            finished_at: job.finished_at,
        }
    }
}

/// Aggregate progress for a batch — powers a single progress bar (plan 2.4/7).
#[derive(Debug, Clone, Serialize)]
pub struct BatchProgressDto {
    pub batch_id: String,
    pub total: i64,
    pub done: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub running: i64,
    pub queued: i64,
    /// Overall completion in permille (0..=1000).
    pub permille: u16,
}

/// The `music_get_job_status` result body.
#[derive(Debug, Clone, Serialize)]
pub struct JobStatusResult {
    /// Matching jobs, in id order (unknown ids are silently absent).
    pub jobs: Vec<JobStatusDto>,
    /// Aggregate progress, present only when a `batch_id` was queried.
    pub batch: Option<BatchProgressDto>,
}

/// One source track's enqueue outcome inside a `music_create_instrumental`
/// result. Carries opaque ids only — never a file path or staging location.
#[derive(Debug, Clone, Serialize)]
pub struct InstrumentalJobDto {
    /// The source track this job renders.
    pub source_track_id: i64,
    /// The registered job id — pass it to `music_get_job_status`.
    pub job_id: i64,
    /// True when an open, staged, or saved job already covered this track, so
    /// no new render was started and `job_id` references the existing work
    /// (Beschluss 16).
    pub deduplicated: bool,
    /// When the referenced work is an already-saved instrumental, its library
    /// track id; null for a fresh or still-rendering job.
    pub existing_instrumental_track_id: Option<i64>,
}

/// The `music_create_instrumental` result body.
#[derive(Debug, Clone, Serialize)]
pub struct CreateInstrumentalResult {
    /// Groups this invocation's jobs; pass it to `music_get_job_status` for
    /// aggregate progress.
    pub batch_id: String,
    /// Echoes the requested `save` intent.
    pub save: bool,
    /// How many jobs were newly queued.
    pub created: usize,
    /// How many tracks referenced existing work instead of re-rendering.
    pub deduplicated: usize,
    /// Per-source-track outcomes, in request order.
    pub jobs: Vec<InstrumentalJobDto>,
    /// An honest, path-free note on what happens next: jobs stay queued until a
    /// worker (the running app or `reprise-cli jobs work`) renders them, and
    /// where the finished render lands.
    pub queued_hint: String,
}
