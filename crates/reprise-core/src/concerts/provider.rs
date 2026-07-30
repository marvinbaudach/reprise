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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConcertFailure {
    Source(SourceError),
    MissingCredentials(SourceError),
}

impl ConcertFailure {
    #[must_use]
    pub const fn source_error(&self) -> &SourceError {
        match self {
            Self::Source(error) | Self::MissingCredentials(error) => error,
        }
    }

    #[must_use]
    pub const fn is_missing_credentials(&self) -> bool {
        matches!(self, Self::MissingCredentials(_))
    }
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

impl From<ProviderError> for ConcertFailure {
    fn from(error: ProviderError) -> Self {
        let kind = SourceErrorKind::from(&error);
        let missing_credentials = matches!(error, ProviderError::MissingCredentials);
        let error = SourceError::new(kind, "concert provider request failed", error.to_string());
        if missing_credentials {
            Self::MissingCredentials(error)
        } else {
            Self::Source(error)
        }
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
    use crate::source_error::SourceErrorKind;

    #[test]
    fn concert_failures_project_rate_limits_without_displaying_statuses() {
        let failure = super::ConcertFailure::from(ProviderError::RateLimited {
            retry_after: Some(360),
        });
        let error = failure.source_error();

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

    #[test]
    fn missing_credentials_stay_a_configuration_failure() {
        let failure = super::ConcertFailure::from(ProviderError::MissingCredentials);

        assert!(failure.is_missing_credentials());
    }
}
