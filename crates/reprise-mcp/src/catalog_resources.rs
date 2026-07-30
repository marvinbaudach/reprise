//! Complete, path-free Concerts and Releases resource projections.
//!
//! These readers deliberately keep the existing filtered `reprise://concerts`
//! resource intact. The complete resources expose every durable event/history
//! field through `reprise-core` facades, never through adapter-owned SQL, and
//! reduce provider credentials to configured/not-configured booleans.

use std::path::Path;

use reprise_core::artist_news::LibraryPresence;
use reprise_core::artist_news_history::{HistoryStatus, ReleaseHistoryRecord};
use reprise_core::concerts::{CachedConcertEvent, ConcertFilter, DateHorizon};
use serde::Serialize;

use crate::data::{self, DataError};

#[derive(Debug, Serialize)]
pub struct CompleteConcertsResource {
    events: Vec<CompleteConcertEvent>,
    location: Option<ConcertLocation>,
    filter: ConcertFilterResource,
    window_days: i64,
    similar_artists: SimilarArtistsConfig,
    providers: ProviderConfiguration,
    latest_fetch_at: Option<i64>,
}

#[derive(Debug, Serialize)]
struct CompleteConcertEvent {
    id: i64,
    artist_key: String,
    artist_name: String,
    starts_at: String,
    date_key: String,
    venue: String,
    city: String,
    region: Option<String>,
    country: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    ticket_url: Option<String>,
    ticket_source: Option<String>,
    event_url: Option<String>,
    provider: String,
    is_similar: bool,
    similar_to: Option<String>,
    fetched_at: i64,
    seen_at: Option<i64>,
    dedupe_key: String,
}

impl From<CachedConcertEvent> for CompleteConcertEvent {
    fn from(event: CachedConcertEvent) -> Self {
        Self {
            id: event.id,
            artist_key: event.artist_key,
            artist_name: event.artist_name,
            starts_at: event.starts_at,
            date_key: event.date_key,
            venue: event.venue,
            city: event.city,
            region: event.region,
            country: event.country,
            latitude: event.latitude,
            longitude: event.longitude,
            ticket_url: event.ticket_url,
            ticket_source: event.ticket_source,
            event_url: event.event_url,
            provider: event.provider,
            is_similar: event.is_similar,
            similar_to: event.similar_to,
            fetched_at: event.fetched_at,
            seen_at: event.seen_at,
            dedupe_key: event.dedupe_key,
        }
    }
}

#[derive(Debug, Serialize)]
struct ConcertLocation {
    latitude: f64,
    longitude: f64,
    name: String,
}

#[derive(Debug, Serialize)]
struct ConcertFilterResource {
    radius_km: Option<f64>,
    country: Option<String>,
    horizon: &'static str,
    include_similar: bool,
}

impl From<ConcertFilter> for ConcertFilterResource {
    fn from(filter: ConcertFilter) -> Self {
        Self {
            radius_km: filter.radius_km,
            country: filter.country,
            horizon: horizon_name(filter.horizon),
            include_similar: filter.include_similar,
        }
    }
}

#[derive(Debug, Serialize)]
struct SimilarArtistsConfig {
    enabled: bool,
    count: usize,
}

#[derive(Debug, Serialize)]
struct ProviderConfiguration {
    ticketmaster: bool,
    bandsintown: bool,
}

/// Complete durable concert cache plus effective non-secret configuration.
pub fn complete_concerts(path: &Path) -> Result<CompleteConcertsResource, DataError> {
    let db = data::open(path)?;
    data::require_read(&db)?;
    let location = reprise_core::concerts::config::location(&db)
        .map_err(DataError::Db)?
        .map(|location| ConcertLocation {
            latitude: location.latitude,
            longitude: location.longitude,
            name: location.name,
        });
    let filter = reprise_core::concerts::config::persisted_filter(&db).map_err(DataError::Db)?;
    let similar = reprise_core::concerts::config::similar_config(&db).map_err(DataError::Db)?;
    let providers = reprise_core::concerts::config::credentials(&db).map_err(DataError::Db)?;
    let events = reprise_core::concerts::query_cached_events(&db)
        .map_err(DataError::Db)?
        .into_iter()
        .map(CompleteConcertEvent::from)
        .collect();

    Ok(CompleteConcertsResource {
        events,
        location,
        filter: filter.into(),
        window_days: reprise_core::concerts::config::window_days(&db).map_err(DataError::Db)?,
        similar_artists: SimilarArtistsConfig {
            enabled: similar.enabled,
            count: similar.count,
        },
        providers: ProviderConfiguration {
            ticketmaster: providers.ticketmaster_api_key.is_some(),
            bandsintown: providers.bandsintown_app_id.is_some(),
        },
        latest_fetch_at: reprise_core::concerts::latest_fetch_at(&db).map_err(DataError::Db)?,
    })
}

#[derive(Debug, Serialize)]
pub struct ReleasesResource {
    releases: Vec<CompleteRelease>,
    latest_fetch_at: Option<i64>,
}

#[derive(Debug, Serialize)]
struct CompleteRelease {
    release_group_mbid: String,
    artist_name: String,
    artist_mbid: String,
    title: String,
    release_type: String,
    first_release_date: String,
    fetched_at: i64,
    seen_at: Option<i64>,
    hidden: bool,
    fallback_accent: String,
    first_seen: Option<i64>,
    hidden_at: Option<i64>,
    announce_url: Option<String>,
    track_count: Option<i64>,
    local_track_count: i64,
    library_presence: &'static str,
    history_status: &'static str,
}

impl From<ReleaseHistoryRecord> for CompleteRelease {
    fn from(release: ReleaseHistoryRecord) -> Self {
        let history_status = history_status_name(&release.history_status());
        Self {
            release_group_mbid: release.release_group_mbid,
            artist_name: release.artist_name,
            artist_mbid: release.artist_mbid,
            title: release.title,
            release_type: release.release_type,
            first_release_date: release.first_release_date,
            fetched_at: release.fetched_at,
            seen_at: release.seen_at,
            hidden: release.hidden,
            fallback_accent: release.fallback_accent,
            first_seen: release.first_seen,
            hidden_at: release.hidden_at,
            announce_url: release.announce_url,
            track_count: release.track_count,
            local_track_count: release.local_track_count,
            library_presence: library_presence_name(release.presence),
            history_status,
        }
    }
}

/// Complete durable New Releases history, including hidden entries.
pub fn releases(path: &Path) -> Result<ReleasesResource, DataError> {
    let db = data::open(path)?;
    data::require_read(&db)?;
    let releases = reprise_core::artist_news_history::query_complete_history(
        &db,
        chrono::Local::now().date_naive(),
    )
    .map_err(DataError::Db)?
    .into_iter()
    .map(CompleteRelease::from)
    .collect();
    Ok(ReleasesResource {
        releases,
        latest_fetch_at: reprise_core::artist_news::latest_fetched_at(&db)
            .map_err(DataError::Db)?,
    })
}

fn horizon_name(horizon: DateHorizon) -> &'static str {
    match horizon {
        DateHorizon::AllUpcoming => "all_upcoming",
        DateHorizon::Next30Days => "next_30_days",
        DateHorizon::Next3Months => "next_3_months",
        DateHorizon::Next6Months => "next_6_months",
    }
}

fn library_presence_name(presence: LibraryPresence) -> &'static str {
    match presence {
        LibraryPresence::Absent => "absent",
        LibraryPresence::Partial => "partial",
        LibraryPresence::Complete => "complete",
    }
}

fn history_status_name(status: &HistoryStatus) -> &'static str {
    match status {
        HistoryStatus::New => "new",
        HistoryStatus::Seen => "seen",
        HistoryStatus::Hidden => "hidden",
    }
}
