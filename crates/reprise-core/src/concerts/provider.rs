use std::fmt;

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
    #[error("concert provider rate limited the request")]
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

pub trait EventProvider: Send {
    fn kind(&self) -> ProviderKind;
    fn resolve(&self, artist: &ArtistRef<'_>) -> Result<Resolution, ProviderError>;
    fn events(&self, provider_id: &str) -> Result<Vec<ProviderEvent>, ProviderError>;
}
