use chrono::{Duration, Months, NaiveDate};
use rusqlite::{params, Connection};

use super::{haversine_km, ConcertFilter, ConcertRow, DateHorizon};
use crate::location::AppLocation;

struct StoredEvent {
    row: ConcertRow,
    seen_at: Option<i64>,
}

/// One complete row from the durable concert-event cache.
///
/// This is intentionally separate from [`ConcertRow`]: the UI projection
/// carries derived distance but does not need cache bookkeeping, while
/// headless read surfaces need the complete stored record without applying
/// the current UI filters.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedConcertEvent {
    pub id: i64,
    pub artist_key: String,
    pub artist_name: String,
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
    pub provider: String,
    pub is_similar: bool,
    pub similar_to: Option<String>,
    pub fetched_at: i64,
    pub seen_at: Option<i64>,
    pub dedupe_key: String,
}

/// Reads every durable concert-event field without applying the current UI
/// date, radius, country, horizon, or similar-artist filters.
pub fn query_cached_events(conn: &Connection) -> Result<Vec<CachedConcertEvent>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, artist_key, artist_name, starts_at, date_key, venue, city,
                region, country, latitude, longitude, ticket_url, ticket_source,
                event_url, provider, is_similar, similar_to, fetched_at, seen_at,
                dedupe_key
         FROM concert_events
         ORDER BY date_key ASC, starts_at ASC, lower(artist_name) ASC, id ASC",
    )?;
    let events = statement
        .query_map([], |row| {
            Ok(CachedConcertEvent {
                id: row.get(0)?,
                artist_key: row.get(1)?,
                artist_name: row.get(2)?,
                starts_at: row.get(3)?,
                date_key: row.get(4)?,
                venue: row.get(5)?,
                city: row.get(6)?,
                region: row.get(7)?,
                country: row.get(8)?,
                latitude: row.get(9)?,
                longitude: row.get(10)?,
                ticket_url: row.get(11)?,
                ticket_source: row.get(12)?,
                event_url: row.get(13)?,
                provider: row.get(14)?,
                is_similar: row.get::<_, i64>(15)? != 0,
                similar_to: row.get(16)?,
                fetched_at: row.get(17)?,
                seen_at: row.get(18)?,
                dedupe_key: row.get(19)?,
            })
        })?
        .collect();
    events
}

pub fn query_events(
    conn: &Connection,
    filter: &ConcertFilter,
    location: Option<&AppLocation>,
    today: NaiveDate,
) -> Result<Vec<ConcertRow>, rusqlite::Error> {
    Ok(filtered_events(conn, filter, location, today)?
        .into_iter()
        .map(|event| event.row)
        .collect())
}

pub fn count_upcoming(
    conn: &Connection,
    filter: &ConcertFilter,
    location: Option<&AppLocation>,
    today: NaiveDate,
) -> Result<i64, rusqlite::Error> {
    Ok(filtered_events(conn, filter, location, today)?.len() as i64)
}

pub fn query_unseen(
    conn: &Connection,
    filter: &ConcertFilter,
    location: Option<&AppLocation>,
    today: NaiveDate,
    limit: usize,
) -> Result<Vec<ConcertRow>, rusqlite::Error> {
    Ok(filtered_events(conn, filter, location, today)?
        .into_iter()
        .filter(|event| event.seen_at.is_none())
        .take(limit)
        .map(|event| event.row)
        .collect())
}

pub fn count_unseen(
    conn: &Connection,
    filter: &ConcertFilter,
    location: Option<&AppLocation>,
    today: NaiveDate,
) -> Result<i64, rusqlite::Error> {
    Ok(filtered_events(conn, filter, location, today)?
        .into_iter()
        .filter(|event| event.seen_at.is_none())
        .count() as i64)
}

pub fn mark_scope_seen(
    conn: &Connection,
    filter: &ConcertFilter,
    location: Option<&AppLocation>,
    today: NaiveDate,
    now: i64,
) -> Result<usize, rusqlite::Error> {
    let ids = filtered_events(conn, filter, location, today)?
        .into_iter()
        .filter(|event| event.seen_at.is_none())
        .map(|event| event.row.id)
        .collect::<Vec<_>>();
    let transaction = conn.unchecked_transaction()?;
    for id in &ids {
        transaction.execute(
            "UPDATE concert_events SET seen_at = ?1 WHERE id = ?2 AND seen_at IS NULL",
            params![now, id],
        )?;
    }
    transaction.commit()?;
    Ok(ids.len())
}

pub fn latest_fetch_at(conn: &Connection) -> Result<Option<i64>, rusqlite::Error> {
    super::refresh::latest_attempt(conn)
}

fn filtered_events(
    conn: &Connection,
    filter: &ConcertFilter,
    location: Option<&AppLocation>,
    today: NaiveDate,
) -> Result<Vec<StoredEvent>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, starts_at, date_key, artist_name, venue, city, region,
                country, latitude, longitude, ticket_url, ticket_source,
                event_url, provider, is_similar, similar_to, seen_at
         FROM concert_events
         WHERE date_key >= ?1
         ORDER BY date_key ASC, starts_at ASC, lower(artist_name) ASC, id ASC",
    )?;
    let events = statement.query_map([today.format("%Y-%m-%d").to_string()], |row| {
        let latitude: Option<f64> = row.get(8)?;
        let longitude: Option<f64> = row.get(9)?;
        let distance_km = location.and_then(|location| {
            latitude.zip(longitude).map(|(latitude, longitude)| {
                haversine_km(location.latitude, location.longitude, latitude, longitude)
            })
        });
        Ok(StoredEvent {
            row: ConcertRow {
                id: row.get(0)?,
                starts_at: row.get(1)?,
                date_key: row.get(2)?,
                artist_name: row.get(3)?,
                venue: row.get(4)?,
                city: row.get(5)?,
                region: row.get(6)?,
                country: row.get(7)?,
                latitude,
                longitude,
                distance_km,
                ticket_url: row.get(10)?,
                ticket_source: row.get(11)?,
                event_url: row.get(12)?,
                provider: row.get(13)?,
                is_similar: row.get::<_, i64>(14)? != 0,
                similar_to: row.get(15)?,
            },
            seen_at: row.get(16)?,
        })
    })?;
    let last_date = horizon_end(today, filter.horizon);
    let mut filtered = Vec::new();
    for event in events {
        let event = event?;
        if !filter.include_similar && event.row.is_similar {
            continue;
        }
        if filter.country.as_deref().is_some_and(|country| {
            !event
                .row
                .country
                .as_deref()
                .is_some_and(|actual| actual.eq_ignore_ascii_case(country.trim()))
        }) {
            continue;
        }
        if last_date
            .as_ref()
            .is_some_and(|last_date| event.row.date_key.as_str() > last_date.as_str())
        {
            continue;
        }
        if location.is_some()
            && filter.radius_km.is_some_and(|radius| {
                !event
                    .row
                    .distance_km
                    .is_some_and(|distance| distance <= radius)
            })
        {
            continue;
        }
        filtered.push(event);
    }
    Ok(filtered)
}

fn horizon_end(today: NaiveDate, horizon: DateHorizon) -> Option<String> {
    let date = match horizon {
        DateHorizon::AllUpcoming => return None,
        DateHorizon::Next30Days => today.checked_add_signed(Duration::days(30)),
        DateHorizon::Next3Months => today.checked_add_months(Months::new(3)),
        DateHorizon::Next6Months => today.checked_add_months(Months::new(6)),
    }?;
    Some(date.format("%Y-%m-%d").to_string())
}

/// Whether the cache holds any event that came from a similar artist.
///
/// The filter bar offers the "include similar" switch only when there is
/// something for it to reveal; without this the switch appears and does
/// nothing.
pub fn has_similar_events(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM concert_events WHERE is_similar = 1)",
        [],
        |row| row.get::<_, bool>(0),
    )
}

/// The distinct countries the cached events name, ordered for display.
///
/// Blank and whitespace-only values are dropped rather than offered as an
/// unnamed choice.
pub fn known_countries(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut statement = conn.prepare(
        "SELECT DISTINCT trim(country) FROM concert_events
         WHERE country IS NOT NULL AND trim(country) <> ''
         ORDER BY lower(trim(country))",
    )?;
    let rows = statement.query_map([], |row| row.get(0))?;
    // A row that will not convert costs the list that one entry, not all of
    // them: an unusable value in one cached event must not empty the filter.
    Ok(rows.filter_map(Result::ok).collect())
}
