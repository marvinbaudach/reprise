use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use chrono::NaiveDate;
use rusqlite::params;

use super::{refresh, ArtistRef, EventProvider};
use crate::concerts::{ProviderError, ProviderEvent, ProviderKind, Resolution, TicketAvailability};

struct ResolveProvider {
    kind: ProviderKind,
    resolution: Result<Resolution, ProviderError>,
    events: Vec<ProviderEvent>,
    resolve_calls: Arc<AtomicUsize>,
}

impl ResolveProvider {
    fn new(
        kind: ProviderKind,
        resolution: Result<Resolution, ProviderError>,
        events: Vec<ProviderEvent>,
    ) -> Self {
        Self {
            kind,
            resolution,
            events,
            resolve_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl EventProvider for ResolveProvider {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    fn resolve(&self, _artist: &ArtistRef<'_>) -> Result<Resolution, ProviderError> {
        self.resolve_calls.fetch_add(1, Ordering::Relaxed);
        self.resolution.clone()
    }

    fn events(&self, _provider_id: &str) -> Result<Vec<ProviderEvent>, ProviderError> {
        Ok(self.events.clone())
    }
}

fn database_with_artist() -> crate::db::Db {
    let database = crate::db::Db::open_in_memory().unwrap();
    database
        .conn()
        .execute(
            "INSERT INTO listen_events (
               track_id, played_at, ms_played, artist
             ) VALUES (1, ?1, 1, ?2)",
            params![1_000, "Lorna Shore"],
        )
        .unwrap();
    database
}

fn event() -> ProviderEvent {
    ProviderEvent {
        provider: ProviderKind::Ticketmaster,
        availability: TicketAvailability::Unknown,
        starts_at: "2026-10-17T19:00:00".into(),
        date_key: "2026-10-17".into(),
        venue: "Zenith".into(),
        city: "Munich".into(),
        region: Some("BY".into()),
        country: Some("DE".into()),
        latitude: Some(48.17),
        longitude: Some(11.55),
        ticket_url: Some("https://tickets.example/1".into()),
        ticket_source: Some("Example".into()),
        event_url: Some("https://events.example/1".into()),
    }
}

#[test]
fn ticketmaster_resolves_after_bandsintown_returns_http_403() {
    let database = database_with_artist();
    let bandsintown = ResolveProvider::new(
        ProviderKind::Bandsintown,
        Err(ProviderError::HttpStatus(403)),
        Vec::new(),
    );
    let ticketmaster = ResolveProvider::new(
        ProviderKind::Ticketmaster,
        Ok(Resolution::Resolved {
            provider_id: "tm-id".into(),
            mbid_verified: false,
        }),
        vec![event()],
    );

    let summary = refresh(
        &database,
        &[Box::new(bandsintown), Box::new(ticketmaster)],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
    )
    .unwrap();

    assert_eq!(summary.resolved, 1);
    assert_eq!(summary.failed, 0);
    assert!(summary.failures.is_empty());
    let provider: String = database
        .conn()
        .query_row("SELECT provider FROM concert_artists", [], |row| row.get(0))
        .unwrap();
    assert_eq!(provider, "ticketmaster");
}

#[test]
fn all_provider_errors_record_a_failure_and_keep_the_first_error() {
    let database = database_with_artist();
    let providers: Vec<Box<dyn EventProvider>> = vec![
        Box::new(ResolveProvider::new(
            ProviderKind::Bandsintown,
            Err(ProviderError::HttpStatus(403)),
            Vec::new(),
        )),
        Box::new(ResolveProvider::new(
            ProviderKind::Ticketmaster,
            Err(ProviderError::Transport),
            Vec::new(),
        )),
    ];

    let summary = refresh(
        &database,
        &providers,
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
    )
    .unwrap();

    assert_eq!(summary.failed, 1);
    assert_eq!(summary.unmatched, 0);
    assert_eq!(summary.failures.len(), 1);
    assert!(summary.failures[0]
        .source_error()
        .details("now")
        .to_string()
        .contains("HTTP status 403"));
    assert_eq!(stored_outcome(&database), "failed");
}

#[test]
fn all_unmatched_providers_store_an_unmatched_resolution() {
    let database = database_with_artist();
    let providers: Vec<Box<dyn EventProvider>> = vec![
        Box::new(ResolveProvider::new(
            ProviderKind::Bandsintown,
            Ok(Resolution::Unmatched),
            Vec::new(),
        )),
        Box::new(ResolveProvider::new(
            ProviderKind::Ticketmaster,
            Ok(Resolution::Unmatched),
            Vec::new(),
        )),
    ];

    let summary = refresh(
        &database,
        &providers,
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
    )
    .unwrap();

    assert_eq!(summary.failed, 0);
    assert_eq!(summary.unmatched, 1);
    assert!(summary.failures.is_empty());
    assert_eq!(stored_outcome(&database), "unmatched");
}

#[test]
fn provider_error_followed_by_unmatched_records_a_failure() {
    let database = database_with_artist();
    let providers: Vec<Box<dyn EventProvider>> = vec![
        Box::new(ResolveProvider::new(
            ProviderKind::Bandsintown,
            Err(ProviderError::HttpStatus(403)),
            Vec::new(),
        )),
        Box::new(ResolveProvider::new(
            ProviderKind::Ticketmaster,
            Ok(Resolution::Unmatched),
            Vec::new(),
        )),
    ];

    let summary = refresh(
        &database,
        &providers,
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
    )
    .unwrap();

    assert_eq!(summary.failed, 1);
    assert_eq!(summary.unmatched, 0);
    assert_eq!(summary.failures.len(), 1);
    assert_eq!(stored_outcome(&database), "failed");
}

#[test]
fn quiet_period_aborts_without_asking_the_next_provider() {
    let database = database_with_artist();
    let ticketmaster = ResolveProvider::new(
        ProviderKind::Ticketmaster,
        Ok(Resolution::Resolved {
            provider_id: "tm-id".into(),
            mbid_verified: false,
        }),
        vec![event()],
    );
    let ticketmaster_calls = ticketmaster.resolve_calls.clone();
    let providers: Vec<Box<dyn EventProvider>> = vec![
        Box::new(ResolveProvider::new(
            ProviderKind::Bandsintown,
            Err(ProviderError::RateLimited {
                retry_after: Some(61),
            }),
            Vec::new(),
        )),
        Box::new(ticketmaster),
    ];

    let result = refresh(
        &database,
        &providers,
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
    );

    assert!(matches!(
        result,
        Err(crate::concerts::ConcertError::Provider(
            ProviderError::RateLimited {
                retry_after: Some(61)
            }
        ))
    ));
    assert_eq!(ticketmaster_calls.load(Ordering::Relaxed), 0);
    assert_eq!(stored_outcome(&database), "failed");
}

fn stored_outcome(database: &crate::db::Db) -> String {
    database
        .conn()
        .query_row("SELECT last_outcome FROM concert_artists", [], |row| {
            row.get(0)
        })
        .unwrap()
}
