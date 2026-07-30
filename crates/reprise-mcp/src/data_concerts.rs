//! Path-free Concerts resource projection for the local MCP server.

use std::path::Path;

use serde::Serialize;

use crate::data::{open, require_read, DataError};

#[derive(Debug, Serialize)]
pub(crate) struct ConcertsResource {
    events: Vec<ConcertResourceEvent>,
    filter_applied: bool,
    latest_fetch_at: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ConcertResourceEvent {
    date: String,
    starts_at: String,
    artist: String,
    venue: String,
    city: String,
    region: Option<String>,
    country: Option<String>,
    distance_km: Option<f64>,
    ticket_url: Option<String>,
    ticket_source: Option<String>,
    event_url: Option<String>,
    provider: String,
    is_similar: bool,
    similar_to: Option<String>,
}

/// Upcoming concerts after the saved filters, with no filesystem paths.
pub(crate) fn list_concerts(path: &Path) -> Result<ConcertsResource, DataError> {
    let db = open(path)?;
    require_read(&db)?;
    let filter = reprise_core::concerts::config::persisted_filter(&db).map_err(DataError::Db)?;
    let location = reprise_core::concerts::config::location(&db).map_err(DataError::Db)?;
    let events = reprise_core::concerts::query_events(
        &db,
        &filter,
        location.as_ref(),
        chrono::Local::now().date_naive(),
    )
    .map_err(DataError::Db)?
    .into_iter()
    .map(|event| ConcertResourceEvent {
        date: event.date_key,
        starts_at: event.starts_at,
        artist: event.artist_name,
        venue: event.venue,
        city: event.city,
        region: event.region,
        country: event.country,
        distance_km: event.distance_km,
        ticket_url: event.ticket_url,
        ticket_source: event.ticket_source,
        event_url: event.event_url,
        provider: event.provider,
        is_similar: event.is_similar,
        similar_to: event.similar_to,
    })
    .collect();
    Ok(ConcertsResource {
        events,
        filter_applied: true,
        latest_fetch_at: reprise_core::concerts::latest_fetch_at(&db).map_err(DataError::Db)?,
    })
}
