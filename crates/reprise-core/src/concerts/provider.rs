use std::{fmt, time::Duration};

use crate::source_error::{SourceError, SourceErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    Bandsintown,
    Ticketmaster,
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Bandsintown => "bandsintown",
            Self::Ticketmaster => "ticketmaster",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ArtistRef<'a> {
    pub name: &'a str,
    pub mbid: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Resolved {
        provider_id: String,
        mbid_verified: bool,
    },
    Unmatched,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderEvent {
    pub provider: ProviderKind,
    pub starts_at: String,
    pub date_key: String,
    pub venue: String,
    pub city: String,
    pub region: Option<String>,
    pub country: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub ticket_url: Option<String>,
    pub ticket_source: Option<String>,
    pub event_url: Option<String>,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProviderError {
    #[error("concert provider returned HTTP 429")]
    RateLimited { retry_after: Option<u64> },
    #[error("concert provider returned HTTP status {0}")]
    HttpStatus(u16),
    #[error("concert provider request timed out")]
    Timeout,
    #[error("concert provider transport failed")]
    Transport,
    #[error("concert provider response body could not be read")]
    Body,
    #[error("concert provider response body exceeds the size limit")]
    BodyTooLarge,
    #[error("concert provider response could not be parsed")]
    Parse,
    #[error("concert provider credentials are missing")]
    MissingCredentials,
}

impl From<&ProviderError> for SourceErrorKind {
    fn from(error: &ProviderError) -> Self {
        match error {
            ProviderError::RateLimited { retry_after } => Self::RateLimited {
                retry_after: retry_after.map(Duration::from_secs),
            },
            ProviderError::HttpStatus(_)
            | ProviderError::Timeout
            | ProviderError::Transport
            | ProviderError::Body
            | ProviderError::BodyTooLarge
            | ProviderError::Parse
            | ProviderError::MissingCredentials => Self::Unreachable,
        }
    }
}

impl From<ProviderError> for SourceError {
    fn from(error: ProviderError) -> Self {
        let kind = SourceErrorKind::from(&error);
        Self::new(kind, "concert provider request failed", error.to_string())
    }
}

pub trait EventProvider: Send {
    fn kind(&self) -> ProviderKind;
    fn resolve(&self, artist: &ArtistRef<'_>) -> Result<Resolution, ProviderError>;
    fn events(&self, provider_id: &str) -> Result<Vec<ProviderEvent>, ProviderError>;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::ProviderError;
    use crate::source_error::{SourceError, SourceErrorKind};

    #[test]
    fn concert_failures_project_rate_limits_without_displaying_statuses() {
        let error = SourceError::from(ProviderError::RateLimited {
            retry_after: Some(360),
        });

        assert_eq!(
            error.kind(),
            &SourceErrorKind::RateLimited {
                retry_after: Some(Duration::from_secs(360))
            }
        );
        assert!(!error.to_string().contains("429"));
        assert_eq!(
            error.details("2026-07-30 14:12").to_string(),
            "concert provider request failed\n\
             concert provider returned HTTP 429\n\
             2026-07-30 14:12 · retry in 6 min"
        );
    }
}
