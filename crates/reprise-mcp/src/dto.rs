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
use reprise_core::queries::{AlbumSummary, ArtistSummary};
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

/// Artist metadata exposed to agents. The representative cover source remains
/// internal because it is a filesystem path.
#[derive(Debug, Clone, Serialize)]
pub struct ArtistDto {
    pub artist: String,
    pub track_count: i64,
    pub album_count: i64,
    pub total_plays: i64,
}

impl From<&ArtistSummary> for ArtistDto {
    fn from(summary: &ArtistSummary) -> Self {
        Self {
            artist: summary.artist.clone(),
            track_count: summary.track_count,
            album_count: summary.album_count,
            total_plays: summary.total_plays,
        }
    }
}

/// A page of artist search results.
#[derive(Debug, Clone, Serialize)]
pub struct SearchArtistsResult {
    pub artists: Vec<ArtistDto>,
    pub total: usize,
    pub offset: i64,
    pub limit: i64,
    pub returned: usize,
    pub has_more: bool,
}

/// Album metadata exposed to agents. Cover source paths and internal activity
/// timestamps are deliberately omitted.
#[derive(Debug, Clone, Serialize)]
pub struct AlbumDto {
    pub album: String,
    pub album_artist: String,
    pub track_count: i64,
    pub year: Option<i32>,
    pub total_duration_ms: i64,
    pub total_play_count: i64,
}

impl From<&AlbumSummary> for AlbumDto {
    fn from(summary: &AlbumSummary) -> Self {
        Self {
            album: summary.album.clone(),
            album_artist: summary.album_artist.clone(),
            track_count: summary.track_count,
            year: summary.year,
            total_duration_ms: summary.total_duration_ms,
            total_play_count: summary.total_play_count,
        }
    }
}

/// A page of album search results.
#[derive(Debug, Clone, Serialize)]
pub struct SearchAlbumsResult {
    pub albums: Vec<AlbumDto>,
    pub total: usize,
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

/// One playlist plus a page of its durable membership, in playlist order.
#[derive(Debug, Clone, Serialize)]
pub struct PlaylistContentsResult {
    pub playlist: PlaylistDto,
    pub tracks: Vec<TrackDto>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
    pub returned: usize,
    pub has_more: bool,
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

/// Shared pagination and substring-filter parameters for artist and album
/// discovery.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct BrowseLibraryParams {
    /// Case-insensitive substring. Empty matches every artist or album.
    #[serde(default)]
    pub query: String,
    /// Maximum rows to return (1..=200, default 50).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Zero-based offset into the matching rows (default 0).
    #[serde(default)]
    pub offset: Option<u32>,
}

/// Parameters for reading one manual playlist's membership.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct GetPlaylistParams {
    pub playlist_id: i64,
    /// Maximum tracks to return (1..=200, default 50).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Zero-based offset in playlist order (default 0).
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

/// Parameters for one non-destructive playlist update. This is deliberately a
/// root object (rather than a serde-tagged enum) because MCP requires every
/// tool input schema to have root `type: object`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct UpdatePlaylistParams {
    /// `rename` or `add_tracks`.
    pub action: String,
    pub playlist_id: i64,
    /// Required only for `rename`.
    #[serde(default)]
    pub name: Option<String>,
    /// Required and non-empty only for `add_tracks`.
    #[serde(default)]
    pub track_ids: Vec<i64>,
}

/// Result of one non-destructive playlist update.
#[derive(Debug, Clone, Serialize)]
pub struct UpdatePlaylistResult {
    pub playlist_id: i64,
    pub name: String,
    pub track_count: i64,
    pub action: String,
    pub affected: usize,
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

/// Parameters for `music_playback_control` (transport-only: play/pause/stop/
/// next/previous, no target). Only the `mpris`-gated `music_playback_control`
/// tool uses this, so it is gated the same way.
#[cfg(feature = "mpris")]
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PlaybackControlParams {
    /// One of: "play", "pause", "stop", "next", "previous".
    pub action: String,
}

/// Empty root-object parameters for the live playback-state read.
#[cfg(feature = "mpris")]
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct PlaybackStateParams {}

/// Path-free live state returned by `music_get_playback_state`.
#[cfg(feature = "mpris")]
#[derive(Debug, Clone, Serialize)]
pub struct PlaybackStateDto {
    /// One of: "playing", "paused", "stopped".
    pub status: String,
    pub track_id: Option<i64>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub position_ms: i64,
    pub volume: f64,
    pub shuffle: bool,
    /// One of: "off", "all", "one".
    pub repeat: String,
}

/// Root-object parameters for `music_set_playback`. The selected `action`
/// determines which one of the optional value fields is required.
#[cfg(feature = "mpris")]
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SetPlaybackParams {
    /// One of: "set_volume", "seek", "set_shuffle", "set_repeat".
    pub action: String,
    /// New volume in the inclusive 0.0..=1.0 range.
    #[serde(default)]
    pub volume: Option<f64>,
    /// Relative seek offset in seconds; negative values seek backwards.
    #[serde(default)]
    pub offset_seconds: Option<f64>,
    /// New shuffle state.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// One of: "off", "all", "one".
    #[serde(default)]
    pub repeat: Option<String>,
}

/// Parameters for `music_play`. Exactly one of `track_ids`/`playlist_id` must
/// be set — enforced by `data::resolve_play_ids`, not by the schema (rmcp/
/// schemars has no "exactly one of" combinator). Only the `mpris`-gated
/// `music_play` tool (and `data::resolve_play_ids`) uses this, so it is gated
/// the same way.
#[cfg(feature = "mpris")]
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PlayParams {
    /// An explicit ordered list of track ids to play. Mutually exclusive with `playlist_id`.
    #[serde(default)]
    pub track_ids: Option<Vec<i64>>,
    /// A playlist id to play (resolved to its tracks). Mutually exclusive with `track_ids`.
    #[serde(default)]
    pub playlist_id: Option<i64>,
}
