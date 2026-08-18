use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};

use super::candidates::{self, ArtistCandidate, MAX_ARTISTS_PER_RUN};
use super::resolution::{self, LedgerArtist, ResolvedIdentity, StoredOutcome};
use super::similar::{self, HttpSimilarFetch, SimilarFetch, SIMILAR_SEEDS};
use super::{
    artist_due, backoff_delay, dedupe_key, merge, ArtistRef, ConcertError, ConcertFailure,
    EventProvider, ProviderError, ProviderEvent, ProviderKind, Resolution,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefreshSummary {
    pub attempted: usize,
    pub resolved: usize,
    pub unmatched: usize,
    pub failed: usize,
    pub events_upserted: usize,
    pub failures: Vec<ConcertFailure>,
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub fn refresh(
    db: &crate::db::Db,
    providers: &[Box<dyn EventProvider>],
    today: NaiveDate,
    now: i64,
    force: bool,
) -> Result<RefreshSummary, ConcertError> {
    let conn = db.conn();
    refresh_cancellable_in(
        conn,
        providers,
        today,
        now,
        force,
        &CancellationToken::default(),
    )
}

pub fn refresh_cancellable(
    db: &crate::db::Db,
    providers: &[Box<dyn EventProvider>],
    today: NaiveDate,
    now: i64,
    force: bool,
    cancelled: &CancellationToken,
) -> Result<RefreshSummary, ConcertError> {
    refresh_cancellable_with_progress(db, providers, today, now, force, cancelled, &|_, _| {})
}

pub fn refresh_cancellable_with_progress(
    db: &crate::db::Db,
    providers: &[Box<dyn EventProvider>],
    today: NaiveDate,
    now: i64,
    force: bool,
    cancelled: &CancellationToken,
    progress: &dyn Fn(usize, usize),
) -> Result<RefreshSummary, ConcertError> {
    let conn = db.conn();
    refresh_with_similar_fetch_cancellable(
        conn,
        providers,
        today,
        now,
        force,
        (&HttpSimilarFetch, crate::scrobbling::BUNDLED_API_KEY),
        &RefreshControl {
            cancelled,
            progress,
        },
    )
}

#[cfg(test)]
pub(crate) fn refresh_with_similar_fetch(
    db: &crate::db::Db,
    providers: &[Box<dyn EventProvider>],
    today: NaiveDate,
    now: i64,
    force: bool,
    similar_fetch: &dyn SimilarFetch,
    lastfm_api_key: Option<&str>,
) -> Result<RefreshSummary, ConcertError> {
    let conn = db.conn();
    refresh_with_similar_fetch_cancellable(
        conn,
        providers,
        today,
        now,
        force,
        (similar_fetch, lastfm_api_key),
        &RefreshControl {
            cancelled: &CancellationToken::default(),
            progress: &|_, _| {},
        },
    )
}

fn refresh_cancellable_in(
    conn: &Connection,
    providers: &[Box<dyn EventProvider>],
    today: NaiveDate,
    now: i64,
    force: bool,
    cancelled: &CancellationToken,
) -> Result<RefreshSummary, ConcertError> {
    refresh_with_similar_fetch_cancellable(
        conn,
        providers,
        today,
        now,
        force,
        (&HttpSimilarFetch, crate::scrobbling::BUNDLED_API_KEY),
        &RefreshControl {
            cancelled,
            progress: &|_, _| {},
        },
    )
}

struct RefreshControl<'a> {
    cancelled: &'a CancellationToken,
    progress: &'a dyn Fn(usize, usize),
}

fn refresh_with_similar_fetch_cancellable(
    conn: &Connection,
    providers: &[Box<dyn EventProvider>],
    today: NaiveDate,
    now: i64,
    force: bool,
    similar: (&dyn SimilarFetch, Option<&str>),
    control: &RefreshControl<'_>,
) -> Result<RefreshSummary, ConcertError> {
    let cancelled = control.cancelled;
    let progress = control.progress;
    if cancelled.is_cancelled() {
        return Ok(RefreshSummary::default());
    }
    if providers.is_empty() {
        delete_past_events(conn, today)?;
        return Err(ConcertError::MissingCredentials);
    }
    let cutoff =
        now.saturating_sub(super::config::window_days_in(conn)?.saturating_mul(24 * 60 * 60));
    let library_artists = candidates::library_candidates(conn, cutoff)?;
    let mut candidates = library_artists
        .iter()
        .filter(|candidate| artist_due(candidate.last_attempt_at, now, force))
        .cloned()
        .collect::<Vec<_>>();
    let similar_config = super::config::similar_config_in(conn)?;
    if similar_config.enabled && candidates.len() < MAX_ARTISTS_PER_RUN {
        if cancelled.is_cancelled() {
            return Ok(RefreshSummary::default());
        }
        let seeds = candidates::seed_artists(conn, cutoff, SIMILAR_SEEDS)?;
        let mut similar = similar::similar_candidates(
            conn,
            &seeds,
            &library_artists,
            similar.0,
            similar_config,
            similar.1,
        )?;
        similar.retain(|candidate| artist_due(candidate.last_attempt_at, now, force));
        candidates.extend(similar);
    }
    candidates.truncate(MAX_ARTISTS_PER_RUN);
    let total = candidates.len();
    progress(0, total);
    let mut summary = RefreshSummary::default();
    for (index, candidate) in candidates.into_iter().enumerate() {
        if index > 0 {
            progress(index, total);
        }
        if cancelled.is_cancelled() {
            return Ok(summary);
        }
        let stored = resolution::load(conn, &candidate.key)?;
        if resolution::negative_retry_blocked(stored.as_ref(), now) {
            continue;
        }
        summary.attempted += 1;
        let artist = ledger_artist(&candidate);
        let resolved = match cached_provider(providers, stored.as_ref()) {
            Some((provider, provider_id, mbid_verified)) => {
                ResolvedProvider::Found(provider, provider_id, mbid_verified)
            }
            None => match resolve_provider(providers, &candidate, cancelled) {
                Ok(resolved) => resolved,
                Err(AttemptFailure::Failed(error)) => {
                    if cancelled.is_cancelled() {
                        return Ok(summary);
                    }
                    tracing::warn!(
                        artist = candidate.name,
                        %error,
                        "concert artist resolution failed"
                    );
                    resolution::store_failed(conn, &artist, now)?;
                    summary.failed += 1;
                    summary.failures.push(error.into());
                    continue;
                }
                Err(AttemptFailure::QuietPeriod(error)) => {
                    if cancelled.is_cancelled() {
                        return Ok(summary);
                    }
                    resolution::store_failed(conn, &artist, now)?;
                    delete_past_events(conn, today)?;
                    return Err(error.into());
                }
                Err(AttemptFailure::Cancelled) => return Ok(summary),
            },
        };
        let ResolvedProvider::Found(provider, provider_id, mbid_verified) = resolved else {
            if cancelled.is_cancelled() {
                return Ok(summary);
            }
            resolution::store_unmatched(conn, &artist, now)?;
            summary.unmatched += 1;
            continue;
        };
        let events = match retry_provider_call(cancelled, || provider.events(&provider_id)) {
            Ok(events) => events,
            Err(AttemptFailure::Failed(error)) => {
                if cancelled.is_cancelled() {
                    return Ok(summary);
                }
                tracing::warn!(
                    artist = candidate.name,
                    %error,
                    "concert event fetch failed"
                );
                resolution::store_failed(conn, &artist, now)?;
                summary.failed += 1;
                summary.failures.push(error.into());
                continue;
            }
            Err(AttemptFailure::QuietPeriod(error)) => {
                if cancelled.is_cancelled() {
                    return Ok(summary);
                }
                resolution::store_failed(conn, &artist, now)?;
                delete_past_events(conn, today)?;
                return Err(error.into());
            }
            Err(AttemptFailure::Cancelled) => return Ok(summary),
        };
        let today_key = today.format("%Y-%m-%d").to_string();
        let events = merge(
            artist.key,
            events
                .into_iter()
                .filter(|event| event.date_key.as_str() >= today_key.as_str())
                .collect(),
        );
        let identity = ResolvedIdentity {
            provider: provider.kind(),
            provider_id: &provider_id,
            mbid_verified,
        };
        if cancelled.is_cancelled() {
            return Ok(summary);
        }
        let upserted = reconcile_artist(conn, &artist, &identity, &events, today, now)?;
        summary.resolved += 1;
        summary.events_upserted += upserted;
    }
    progress(total, total);
    delete_past_events(conn, today)?;
    Ok(summary)
}

enum ResolvedProvider<'a> {
    Found(&'a dyn EventProvider, String, bool),
    Unmatched,
}

fn resolve_provider<'a>(
    providers: &'a [Box<dyn EventProvider>],
    candidate: &ArtistCandidate,
    cancelled: &CancellationToken,
) -> Result<ResolvedProvider<'a>, AttemptFailure> {
    let artist = ArtistRef {
        name: &candidate.name,
        mbid: candidate.mbid.as_deref(),
    };
    for kind in [ProviderKind::Bandsintown, ProviderKind::Ticketmaster] {
        let Some(provider) = provider_for(providers, kind) else {
            continue;
        };
        match retry_provider_call(cancelled, || provider.resolve(&artist))? {
            Resolution::Resolved {
                provider_id,
                mbid_verified,
            } => {
                return Ok(ResolvedProvider::Found(
                    provider,
                    provider_id,
                    mbid_verified,
                ))
            }
            Resolution::Unmatched => {}
        }
    }
    Ok(ResolvedProvider::Unmatched)
}

fn cached_provider<'a>(
    providers: &'a [Box<dyn EventProvider>],
    stored: Option<&resolution::StoredResolution>,
) -> Option<(&'a dyn EventProvider, String, bool)> {
    let stored = stored?;
    if stored.outcome != Some(StoredOutcome::Ok) {
        return None;
    }
    let provider = provider_for(providers, stored.provider?)?;
    Some((provider, stored.provider_id.clone()?, stored.mbid_verified))
}

fn provider_for(
    providers: &[Box<dyn EventProvider>],
    kind: ProviderKind,
) -> Option<&dyn EventProvider> {
    providers
        .iter()
        .find(|provider| provider.kind() == kind)
        .map(Box::as_ref)
}

enum AttemptFailure {
    Failed(ProviderError),
    QuietPeriod(ProviderError),
    Cancelled,
}

fn retry_provider_call<T>(
    cancelled: &CancellationToken,
    mut operation: impl FnMut() -> Result<T, ProviderError>,
) -> Result<T, AttemptFailure> {
    let mut attempt = 1;
    loop {
        if cancelled.is_cancelled() {
            return Err(AttemptFailure::Cancelled);
        }
        let error = match operation() {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        if cancelled.is_cancelled() {
            return Err(AttemptFailure::Cancelled);
        }
        let retry_after = match &error {
            ProviderError::RateLimited { retry_after } => *retry_after,
            ProviderError::HttpStatus(500..=599) => None,
            _ => return Err(AttemptFailure::Failed(error)),
        };
        if retry_after.is_some_and(|seconds| seconds > 60) {
            return Err(AttemptFailure::QuietPeriod(error));
        }
        let Some(delay) = backoff_delay(attempt, retry_after) else {
            return Err(AttemptFailure::Failed(error));
        };
        if !wait_backoff(delay, cancelled) {
            return Err(AttemptFailure::Cancelled);
        }
        attempt += 1;
    }
}

pub(super) fn wait_backoff(mut delay: Duration, cancelled: &CancellationToken) -> bool {
    const SLICE: Duration = Duration::from_millis(50);
    while !delay.is_zero() {
        if cancelled.is_cancelled() {
            return false;
        }
        let slice = delay.min(SLICE);
        std::thread::sleep(slice);
        delay = delay.saturating_sub(slice);
    }
    !cancelled.is_cancelled()
}

fn ledger_artist(candidate: &ArtistCandidate) -> LedgerArtist<'_> {
    LedgerArtist {
        key: &candidate.key,
        name: &candidate.name,
        mbid: candidate.mbid.as_deref(),
        is_similar: candidate.is_similar,
        similar_to: candidate.similar_to.as_deref(),
    }
}

fn reconcile_artist(
    conn: &Connection,
    artist: &LedgerArtist<'_>,
    identity: &ResolvedIdentity<'_>,
    events: &[ProviderEvent],
    today: NaiveDate,
    now: i64,
) -> Result<usize, rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    let mut fresh_keys = HashSet::with_capacity(events.len());
    let mut upserted = 0;
    for event in events {
        let key = dedupe_key(artist.key, &event.date_key, &event.city);
        let keep_stored_listing = stored_provider_owned_listing_wins(
            &transaction,
            &key,
            identity.provider,
            event.ticket_url.as_deref(),
        )?;
        fresh_keys.insert(key.clone());
        upserted += transaction.execute(
            "INSERT INTO concert_events (
               artist_key, artist_name, starts_at, date_key, venue, city,
               region, country, latitude, longitude, ticket_url,
               ticket_source, event_url, provider, is_similar, similar_to,
               fetched_at, dedupe_key, ticket_availability
             ) VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
               ?14, ?15, ?16, ?17, ?18, ?19
             )
             ON CONFLICT(dedupe_key) DO UPDATE SET
               artist_key = excluded.artist_key,
               artist_name = excluded.artist_name,
               starts_at = excluded.starts_at,
               date_key = excluded.date_key,
               venue = CASE WHEN ?20
                 THEN concert_events.venue ELSE excluded.venue END,
               city = excluded.city,
               region = excluded.region,
               country = excluded.country,
               latitude = CASE WHEN ?20
                 THEN concert_events.latitude ELSE excluded.latitude END,
               longitude = CASE WHEN ?20
                 THEN concert_events.longitude ELSE excluded.longitude END,
               ticket_url = CASE WHEN ?20
                 THEN concert_events.ticket_url ELSE excluded.ticket_url END,
               ticket_source = CASE WHEN ?20
                 THEN concert_events.ticket_source ELSE excluded.ticket_source END,
               event_url = CASE WHEN ?20
                 THEN concert_events.event_url ELSE excluded.event_url END,
               provider = excluded.provider,
               is_similar = MIN(concert_events.is_similar, excluded.is_similar),
               similar_to = CASE
                 WHEN MIN(concert_events.is_similar, excluded.is_similar) = 0
                   THEN NULL
                 ELSE excluded.similar_to
               END,
               fetched_at = excluded.fetched_at,
               ticket_availability = excluded.ticket_availability",
            params![
                artist.key,
                artist.name,
                event.starts_at,
                event.date_key,
                event.venue,
                event.city,
                event.region,
                event.country,
                event.latitude,
                event.longitude,
                event.ticket_url,
                event.ticket_source,
                event.event_url,
                identity.provider.to_string(),
                i64::from(artist.is_similar),
                artist.similar_to,
                now,
                key,
                event.availability.as_str(),
                keep_stored_listing
            ],
        )?;
    }
    let mut statement = transaction.prepare(
        "SELECT dedupe_key FROM concert_events
         WHERE artist_key = ?1 AND date_key >= ?2",
    )?;
    let existing = statement
        .query_map(
            params![artist.key, today.format("%Y-%m-%d").to_string()],
            |row| row.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for key in existing {
        if !fresh_keys.contains(&key) {
            transaction.execute(
                "DELETE FROM concert_events WHERE artist_key = ?1 AND dedupe_key = ?2",
                params![artist.key, key],
            )?;
        }
    }
    resolution::store_success(&transaction, artist, identity, now, events.len())?;
    transaction.commit()?;
    Ok(upserted)
}

fn stored_provider_owned_listing_wins(
    conn: &Connection,
    dedupe_key: &str,
    incoming_provider: ProviderKind,
    incoming_ticket_url: Option<&str>,
) -> Result<bool, rusqlite::Error> {
    let stored = conn
        .query_row(
            "SELECT provider, ticket_url FROM concert_events WHERE dedupe_key = ?1",
            [dedupe_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?;
    let Some((stored_provider, stored_ticket_url)) = stored else {
        return Ok(false);
    };
    let stored_provider = match stored_provider.as_str() {
        "bandsintown" => Some(ProviderKind::Bandsintown),
        "ticketmaster" => Some(ProviderKind::Ticketmaster),
        _ => None,
    };
    Ok(stored_provider.is_some_and(|provider| {
        provider.owns_ticket_url(stored_ticket_url.as_deref())
            && !incoming_provider.owns_ticket_url(incoming_ticket_url)
    }))
}

pub(crate) fn delete_past_events(
    conn: &Connection,
    today: NaiveDate,
) -> Result<usize, rusqlite::Error> {
    conn.execute(
        "DELETE FROM concert_events WHERE date_key < ?1",
        [today.format("%Y-%m-%d").to_string()],
    )
}

#[cfg(test)]
#[path = "pipeline_reconcile_tests.rs"]
mod reconcile_tests;
