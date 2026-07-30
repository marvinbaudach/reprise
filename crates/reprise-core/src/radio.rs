//! Internet radio discovery, favorites, and stream metadata.

use crate::source_error::{SourceError, SourceErrorKind};

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

impl From<&RadioError> for SourceErrorKind {
    fn from(error: &RadioError) -> Self {
        match error {
            RadioError::Timeout
            | RadioError::Transport(_)
            | RadioError::HttpStatus(_)
            | RadioError::Body(_)
            | RadioError::Parse(_)
            | RadioError::Unavailable(_) => Self::Unreachable,
        }
    }
}

impl From<RadioError> for SourceError {
    fn from(error: RadioError) -> Self {
        let kind = SourceErrorKind::from(&error);
        Self::new(kind, "radio source request failed", error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::RadioError;
    use crate::source_error::{SourceError, SourceErrorKind};

    #[test]
    fn radio_failures_project_without_displaying_the_raw_payload() {
        let raw = "https://private.example/station?token=SECRET returned HTTP 599";
        let error = SourceError::from(RadioError::Transport(raw.into()));

        assert_eq!(error.kind(), &SourceErrorKind::Unreachable);
        assert!(!error.to_string().contains("private.example"));
        assert!(!error.to_string().contains("SECRET"));
        assert!(error.details("2026-07-30 14:12").to_string().contains(raw));
    }
}
