//! Internet radio discovery, favorites, and stream metadata.

pub mod click;
pub mod config;
pub mod http;
pub mod icy;
pub mod playlist;
pub mod search;
pub mod servers;
pub mod station;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StationRow {
    pub id: i64,
    pub uuid: Option<String>,
    pub name: String,
    pub stream_url: String,
    pub homepage: Option<String>,
    pub favicon_url: Option<String>,
    pub genre: Option<String>,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub country_code: Option<String>,
    pub votes: Option<i64>,
    pub added_at: i64,
    pub removed_at: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum RadioError {
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
    #[error("{0}")]
    Unavailable(String),
}
