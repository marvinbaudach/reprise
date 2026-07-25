//! Frontend-free Concerts domain types and cache APIs.
//!
//! Network providers, refresh orchestration, filtering, and persistence live
//! in focused sibling modules under `concerts/`; this facade is the stable
//! surface consumed by native frontends, the CLI, and MCP.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, thiserror::Error)]
pub enum ConcertError {
    #[error("concert database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("concert provider data is invalid: {0}")]
    InvalidData(String),
    #[error("no concert provider is configured")]
    MissingCredentials,
}
