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
