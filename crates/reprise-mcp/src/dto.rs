//! Leak-safe data-transfer objects and tool parameter types.
//!
//! Every response shape here is the D19 allow-list and nothing more: opaque
//! track ids plus display metadata (title, artist, album, year, genre, rating,
//! duration). A file path, XDG/cache/database path, lyric, device serial,
//! credential, or raw listen event must never appear on any of these structs —
//! the `leak_matrix` integration tests assert exactly that against live
//! responses. Mapping from the richer core types deliberately drops the
//! disallowed fields (e.g. [`Track::path`](reprise_core::models::Track::path)).

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
    /// Tracks with an up-to-date audio-character analysis.
    pub analyzed_tracks: u64,
    /// Present tracks considered for analysis coverage (the denominator).
    pub analysis_total: u64,
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
