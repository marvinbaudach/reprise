//! Source-agnostic remote stats types used to display ListenBrainz and
//! Last.fm listening data in the "My Stats" screen. A remote source
//! **replaces** the local view entirely — its data is never summed with local
//! counters.
//!
//! These compatibility types are intentionally isolated from the editorial
//! My Stats screen: STATS-0 requires that screen to use local `listen_events`
//! only. The remote clients remain available for their existing, unwired
//! fetch paths and never feed `stats_snapshot::compute`.

use super::stats_screen::{HeadlineTotals, MonthlyListens, TopAlbum, TopArtist};

/// Which data source feeds the stats screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatsSource {
    Local,
    ListenBrainz { username: String },
    LastFm { username: String },
}

/// A complete stats payload from a remote service, ready for the UI layer.
#[derive(Debug, Clone)]
pub struct RemoteStats {
    pub source: StatsSource,
    pub headline: HeadlineTotals,
    pub top_artists: Vec<TopArtist>,
    pub top_albums: Vec<TopAlbum>,
    pub monthly: Vec<MonthlyListens>,
}

/// Errors that can occur when fetching remote listening statistics.
#[derive(Debug, thiserror::Error)]
pub enum RemoteStatsError {
    #[error("network error: {0}")]
    Network(String),
    #[error("failed to parse remote response: {0}")]
    ParseError(String),
    #[error("user not found: {0}")]
    UserNotFound(String),
}
