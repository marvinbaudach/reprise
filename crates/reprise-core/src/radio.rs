//! Internet radio discovery, favorites, and stream metadata.

use std::time::Duration;

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
    Unavailable(RadioFailureDetail),
}

#[derive(Debug)]
pub enum RadioFailureDetail {
    Message(String),
    SourceGone(u16),
    RateLimited { retry_after: Option<Duration> },
}

impl std::fmt::Display for RadioFailureDetail {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
            Self::SourceGone(status) => {
                write!(
                    formatter,
                    "source returned HTTP {status} and has moved or ended"
                )
            }
            Self::RateLimited { .. } => formatter.write_str("server returned HTTP 429"),
        }
    }
}

impl From<String> for RadioFailureDetail {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for RadioFailureDetail {
    fn from(message: &str) -> Self {
        Self::Message(message.to_owned())
    }
}

impl RadioError {
    /// Delay for a background refresh retry under the shared source policy.
    #[must_use]
    pub fn retry_delay(&self, attempt: u32) -> Option<Duration> {
        let retry_after = match self {
            Self::Unavailable(RadioFailureDetail::RateLimited { retry_after }) => *retry_after,
            Self::HttpStatus(500..=599) | Self::Timeout | Self::Transport(_) => None,
            _ => return None,
        };
        crate::source_error::source_backoff_delay(attempt, retry_after)
    }
}

impl RadioError {
    /// The safe, payload-free sentence for a radio provider failure. Mirrors
    /// `PodcastError::classify` and exists for the same reason: the MCP surface
    /// used to keep its own copy of this match, which was free to drift and
    /// which flattened *source gone* and *rate limited* into one message even
    /// though the taxonomy already tells them apart.
    #[must_use]
    pub fn classify(&self) -> &'static str {
        match SourceErrorKind::from(self) {
            SourceErrorKind::SourceGone => "radio station has moved or ended",
            SourceErrorKind::RateLimited { .. } => "radio station is rate limited",
            SourceErrorKind::HelperOutdated => "radio station needs an updated helper",
            SourceErrorKind::Offline => "radio station is offline",
            SourceErrorKind::Unreachable => match self {
                RadioError::Timeout => "radio stream timed out",
                RadioError::Transport(_) => "radio stream could not be reached",
                RadioError::HttpStatus(_) => "radio stream returned an HTTP error",
                RadioError::Body(_) | RadioError::Parse(_) => "radio stream returned invalid data",
                RadioError::Unavailable(_) => "radio stream is unavailable",
            },
        }
    }
}

impl From<&RadioError> for SourceErrorKind {
    fn from(error: &RadioError) -> Self {
        match error {
            RadioError::Unavailable(RadioFailureDetail::SourceGone(_)) => Self::SourceGone,
            RadioError::Unavailable(RadioFailureDetail::RateLimited { retry_after }) => {
                Self::RateLimited {
                    retry_after: *retry_after,
                }
            }
            RadioError::Timeout
            | RadioError::Transport(_)
            | RadioError::HttpStatus(_)
            | RadioError::Body(_)
            | RadioError::Parse(_)
            | RadioError::Unavailable(RadioFailureDetail::Message(_)) => Self::Unreachable,
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
    use super::{RadioError, RadioFailureDetail};
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

    #[test]
    fn retryable_radio_failures_use_the_shared_backoff_policy() {
        let rate_limited = RadioError::Unavailable(RadioFailureDetail::RateLimited {
            retry_after: Some(std::time::Duration::from_secs(6)),
        });

        assert_eq!(
            rate_limited.retry_delay(1),
            Some(std::time::Duration::from_secs(6))
        );
        assert_eq!(
            RadioError::HttpStatus(503).retry_delay(2),
            Some(std::time::Duration::from_secs(4))
        );
        assert_eq!(RadioError::HttpStatus(403).retry_delay(1), None);
    }
}
