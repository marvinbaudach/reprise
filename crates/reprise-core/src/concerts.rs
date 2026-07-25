//! Frontend-free Concerts domain types and cache APIs.
//!
//! Network providers, refresh orchestration, filtering, and persistence live
//! in focused sibling modules under `concerts/`; this facade is the stable
//! surface consumed by native frontends, the CLI, and MCP.

use serde::{Deserialize, Serialize};

mod backoff;
mod bandsintown;
#[cfg_attr(not(test), allow(dead_code))]
mod candidates;
pub mod config;
mod dedupe;
mod geo;
mod geocode;
pub mod http;
mod pipeline;
mod provider;
mod query;
mod refresh;
mod resolution;
mod ticketmaster;

pub use backoff::backoff_delay;
pub use bandsintown::BandsintownProvider;
pub use dedupe::{dedupe_key, merge, normalize_component, ticket_source_label};
pub use geo::haversine_km;
pub use geocode::{geocode, geocode_url, parse_geocode, GeocodedLocation};
pub use pipeline::{refresh, RefreshSummary};
pub use provider::{
    ArtistRef, EventProvider, ProviderError, ProviderEvent, ProviderKind, Resolution,
};
pub use query::{
    count_unseen, count_upcoming, latest_fetch_at, mark_scope_seen, query_events, query_unseen,
};
pub use refresh::{artist_due, jitter_seconds, refresh_due};
pub use ticketmaster::TicketmasterProvider;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateHorizon {
    #[default]
    AllUpcoming,
    Next30Days,
    Next3Months,
    Next6Months,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConcertFilter {
    pub radius_km: Option<f64>,
    pub country: Option<String>,
    pub horizon: DateHorizon,
    pub include_similar: bool,
}

impl Default for ConcertFilter {
    fn default() -> Self {
        Self {
            radius_km: None,
            country: None,
            horizon: DateHorizon::AllUpcoming,
            include_similar: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConcertRow {
    pub id: i64,
    pub date_key: String,
    pub starts_at: String,
    pub artist_name: String,
    pub venue: String,
    pub city: String,
    pub region: Option<String>,
    pub country: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub distance_km: Option<f64>,
    pub ticket_url: Option<String>,
    pub ticket_source: Option<String>,
    pub event_url: Option<String>,
    pub provider: String,
    pub is_similar: bool,
    pub similar_to: Option<String>,
}

#[cfg(test)]
#[path = "concerts/domain_tests.rs"]
mod domain_tests;
#[cfg(test)]
#[path = "concerts/pipeline_tests.rs"]
mod pipeline_tests;

#[derive(Debug, thiserror::Error)]
pub enum ConcertError {
    #[error("concert database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("concert provider data is invalid: {0}")]
    InvalidData(String),
    #[error("no concert provider is configured")]
    MissingCredentials,
}
