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

/// Root-object parameters for the safe live queue surface.
#[cfg(feature = "mpris")]
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct QueueParams {
    /// One of: "status", "add_next", "add_last", "clear".
    pub action: String,
    /// Ordered track ids required by add_next and add_last.
    #[serde(default)]
    pub track_ids: Option<Vec<i64>>,
}

/// Collision-safe identity of one item in a live queue response.
#[cfg(feature = "mpris")]
#[derive(Debug, Clone, Serialize)]
pub struct QueueItemDto {
    pub kind: String,
    pub id: i64,
}

#[cfg(feature = "mpris")]
impl From<reprise_runtime_protocol::queue::QueueItem> for QueueItemDto {
    fn from(item: reprise_runtime_protocol::queue::QueueItem) -> Self {
        Self {
            kind: item.kind,
            id: item.id,
        }
    }
}

/// Bounded live queue state. Totals describe the complete sections even when
/// the returned item windows are capped.
#[cfg(feature = "mpris")]
#[derive(Debug, Clone, Serialize)]
pub struct QueueStateDto {
    pub current_track_id: Option<i64>,
    /// Legacy track-only projection. Episodes are omitted.
    pub play_next_track_ids: Vec<i64>,
    pub play_next_items: Vec<QueueItemDto>,
    /// Legacy track-only projection of the automatic context.
    pub context_track_ids: Vec<i64>,
    pub context_items: Vec<QueueItemDto>,
    pub play_next_total: u64,
    pub context_total: u64,
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
