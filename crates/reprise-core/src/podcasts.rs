//! Podcast subscriptions, episodes, refresh, and provider boundaries.

pub mod channel_window;
pub mod config;
pub mod discovery;
pub mod download_state;
pub mod downloads;
pub mod feed;
pub mod http;
pub mod itunes;
pub mod phone_sync;
pub mod pipeline;
pub mod query;
pub mod refresh;
pub mod source_artwork;
pub mod status;
pub mod store;
pub mod url_detect;
pub mod wanted_on_device;
pub mod youtube;
pub mod ytdlp;
mod ytdlp_download;
pub mod ytdlp_search;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PodcastKind {
    Rss,
    Youtube,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpisodeStatus {
    New,
    Resume,
    Played,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscriptionRow {
    pub id: i64,
    pub kind: PodcastKind,
    pub feed_url: String,
    pub title: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_fetch_at: Option<i64>,
    pub last_outcome: Option<String>,
    pub auto_download: bool,
    pub sync_to_phone: bool,
    pub added_at: i64,
    pub removed_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodeRow {
    pub id: i64,
    pub subscription_id: i64,
    pub guid: String,
    pub title: String,
    pub show: String,
    pub show_image_url: Option<String>,
    pub kind: PodcastKind,
    pub audio_url: String,
    pub page_url: Option<String>,
    pub published_at: Option<i64>,
    pub duration_secs: Option<i64>,
    pub downloaded_path: Option<String>,
    pub downloaded_bytes: Option<i64>,
    pub played_at: Option<i64>,
    pub position_ms: i64,
    pub first_seen_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceGroup {
    pub subscription_id: i64,
    pub title: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub kind: PodcastKind,
    pub sync_to_phone: bool,
    pub episodes: Vec<EpisodeRow>,
}

#[derive(Debug, thiserror::Error)]
pub enum PodcastError {
    #[error("request timed out")]
    Timeout,
    #[error("network request failed: {0}")]
    Transport(String),
    #[error("server returned HTTP {0}")]
    HttpStatus(u16),
    #[error("response body could not be read: {0}")]
    Body(String),
    #[error("response could not be parsed: {0}")]
    Parse(String),
    #[error("not modified")]
    NotModified,
    #[error("{0}")]
    YtDlp(String),
    /// The subscription's kind (RSS or YouTube) is disabled, either at its
    /// own module or the global online-sources gate (`NET-1a`).
    #[error("{0}")]
    Disabled(String),
}
