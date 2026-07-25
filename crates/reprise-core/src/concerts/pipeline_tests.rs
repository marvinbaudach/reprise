use std::cell::Cell;

use chrono::NaiveDate;
use rusqlite::{params, Connection};

use super::similar::{SimilarArtist, SimilarFetch};
use super::{
    refresh, ArtistRef, BandsintownProvider, ConcertError, EventProvider, ProviderError,
    ProviderEvent, ProviderKind, Resolution, TicketmasterProvider,
};

fn conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

fn seed_play(conn: &Connection, artist: &str, played_at: i64) {
    conn.execute(
        "INSERT INTO listen_events (
           track_id, played_at, ms_played, artist
         ) VALUES (1, ?1, 1, ?2)",
        params![played_at, artist],
    )
    .unwrap();
}

fn event(venue: &str) -> ProviderEvent {
    ProviderEvent {
        provider: ProviderKind::Ticketmaster,
        starts_at: "2026-10-17T19:00:00".into(),
        date_key: "2026-10-17".into(),
        venue: venue.into(),
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

struct FakeProvider {
    kind: ProviderKind,
    resolution: Resolution,
    events: Vec<ProviderEvent>,
    resolve_calls: Cell<usize>,
}

impl FakeProvider {
    fn new(kind: ProviderKind, resolution: Resolution, events: Vec<ProviderEvent>) -> Self {
        Self {
            kind,
            resolution,
            events,
            resolve_calls: Cell::new(0),
        }
    }
}

impl EventProvider for FakeProvider {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    fn resolve(&self, _artist: &ArtistRef<'_>) -> Result<Resolution, ProviderError> {
        self.resolve_calls.set(self.resolve_calls.get() + 1);
        Ok(self.resolution.clone())
    }

    fn events(&self, _provider_id: &str) -> Result<Vec<ProviderEvent>, ProviderError> {
        Ok(self.events.clone())
    }
}

struct FailingEventsProvider;

impl EventProvider for FailingEventsProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Ticketmaster
    }

    fn resolve(&self, _artist: &ArtistRef<'_>) -> Result<Resolution, ProviderError> {
        unreachable!("the stored resolution should be reused")
    }

    fn events(&self, _provider_id: &str) -> Result<Vec<ProviderEvent>, ProviderError> {
        Err(ProviderError::Transport)
    }
}

struct FakeSimilarFetch {
    artists: Vec<SimilarArtist>,
}

impl SimilarFetch for FakeSimilarFetch {
    fn listenbrainz(&self, _mbid: &str) -> Result<Vec<SimilarArtist>, ProviderError> {
        Ok(self.artists.clone())
    }

    fn lastfm(
        &self,
        _name: &str,
        _api_key: &str,
        _limit: usize,
    ) -> Result<Vec<SimilarArtist>, ProviderError> {
        Ok(self.artists.clone())
    }
}

#[test]
fn fallback_uses_ticketmaster_only_after_bandsintown_unmatched() {
    let conn = conn();
    seed_play(&conn, "Lorna Shore", 1_000);
    let bandsintown =
        FakeProvider::new(ProviderKind::Bandsintown, Resolution::Unmatched, Vec::new());
    let ticketmaster = FakeProvider::new(
        ProviderKind::Ticketmaster,
        Resolution::Resolved {
            provider_id: "tm-id".into(),
            mbid_verified: false,
        },
        vec![event("Zenith")],
    );
    let providers: Vec<Box<dyn EventProvider>> =
        vec![Box::new(ticketmaster), Box::new(bandsintown)];

    let summary = refresh(
        &conn,
        &providers,
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
    )
    .unwrap();

    assert_eq!(summary.attempted, 1);
    assert_eq!(summary.resolved, 1);
    assert_eq!(summary.events_upserted, 1);
    let provider: String = conn
        .query_row("SELECT provider FROM concert_artists", [], |row| row.get(0))
        .unwrap();
    assert_eq!(provider, "ticketmaster");
}

#[test]
fn similar_candidates_share_the_pipeline_and_persist_the_seed_caption() {
    let conn = conn();
    seed_play(&conn, "Library Seed", 1_000);
    conn.execute("UPDATE listen_events SET artist_mbid = 'seed-mbid'", [])
        .unwrap();
    crate::library::settings::set_bool(&conn, "concerts.similar_enabled", true).unwrap();
    crate::library::settings::set_setting(&conn, "concerts.similar_count", "10").unwrap();
    let provider = FakeProvider::new(
        ProviderKind::Ticketmaster,
        Resolution::Resolved {
            provider_id: "tm-id".into(),
            mbid_verified: false,
        },
        vec![event("Zenith")],
    );
    let fetch = FakeSimilarFetch {
        artists: vec![SimilarArtist {
            name: "Discovered Artist".into(),
            mbid: Some("discovered-mbid".into()),
            score: 1.0,
        }],
    };

    let summary = super::pipeline::refresh_with_similar_fetch(
        &conn,
        &[Box::new(provider)],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
        &fetch,
        None,
    )
    .unwrap();

    assert_eq!(summary.attempted, 2);
    let similar: (i64, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT is_similar, similar_to, artist_mbid
             FROM concert_artists
             WHERE artist_name = 'Discovered Artist'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        similar,
        (
            1,
            Some("Library Seed".into()),
            Some("discovered-mbid".into())
        )
    );

    seed_play(&conn, "Discovered Artist", 1_001);
    super::pipeline::refresh_with_similar_fetch(
        &conn,
        &[Box::new(FakeProvider::new(
            ProviderKind::Ticketmaster,
            Resolution::Resolved {
                provider_id: "tm-id".into(),
                mbid_verified: false,
            },
            vec![event("Zenith")],
        ))],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        2_000,
        true,
        &fetch,
        None,
    )
    .unwrap();
    let promoted: (i64, Option<String>) = conn
        .query_row(
            "SELECT is_similar, similar_to FROM concert_artists
             WHERE artist_name = 'Discovered Artist'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(promoted, (0, None));
}

#[test]
fn library_candidates_take_the_shared_thirty_artist_budget_first() {
    let conn = conn();
    for index in 0..30 {
        seed_play(&conn, &format!("Library {index:02}"), 1_000);
    }
    conn.execute(
        "UPDATE listen_events SET artist_mbid = 'seed-mbid' WHERE artist = 'Library 00'",
        [],
    )
    .unwrap();
    crate::library::settings::set_bool(&conn, "concerts.similar_enabled", true).unwrap();
    let provider = FakeProvider::new(
        ProviderKind::Ticketmaster,
        Resolution::Resolved {
            provider_id: "tm-id".into(),
            mbid_verified: false,
        },
        Vec::new(),
    );
    let fetch = FakeSimilarFetch {
        artists: vec![SimilarArtist {
            name: "Budget Overflow".into(),
            mbid: Some("overflow".into()),
            score: 1.0,
        }],
    };

    let summary = super::pipeline::refresh_with_similar_fetch(
        &conn,
        &[Box::new(provider)],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
        &fetch,
        None,
    )
    .unwrap();

    assert_eq!(summary.attempted, 30);
    let overflow: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM concert_artists WHERE artist_name = 'Budget Overflow'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(overflow, 0);
}

#[test]
fn fixture_backed_providers_run_end_to_end_without_network() {
    let conn = conn();
    seed_play(&conn, "Lorna Shore", 1_000);
    let fixtures = tempfile::tempdir().unwrap();
    std::fs::write(
        fixtures.path().join("bandsintown-artist-Lorna_Shore.json"),
        r#"{"error":"Not Found"}"#,
    )
    .unwrap();
    std::fs::write(
        fixtures
            .path()
            .join("ticketmaster-attractions-Lorna_Shore.json"),
        r#"{"_embedded":{"attractions":[{"id":"tm-id","name":"Lorna Shore"}]}}"#,
    )
    .unwrap();
    std::fs::write(
        fixtures.path().join("ticketmaster-events-tm-id.json"),
        r#"{"_embedded":{"events":[{
          "url":"https://ticketmaster.example/e/1",
          "dates":{"start":{"localDate":"2026-10-17","localTime":"19:00:00"}},
          "_embedded":{"venues":[{"name":"Zenith","city":{"name":"Munich"}}]}
        }]}}"#,
    )
    .unwrap();
    let providers: Vec<Box<dyn EventProvider>> = vec![
        Box::new(BandsintownProvider::new("approved-app")),
        Box::new(TicketmasterProvider::new("api-key")),
    ];

    let summary = super::http::with_fixture_dir(fixtures.path(), || {
        refresh(
            &conn,
            &providers,
            NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
            1_000,
            false,
        )
        .unwrap()
    });

    assert_eq!(summary.resolved, 1);
    assert_eq!(summary.events_upserted, 1);
}

#[test]
fn reconcile_removes_cancelled_events_and_preserves_seen_state() {
    let conn = conn();
    seed_play(&conn, "Lorna Shore", 1_000);
    let first = FakeProvider::new(
        ProviderKind::Ticketmaster,
        Resolution::Resolved {
            provider_id: "tm-id".into(),
            mbid_verified: false,
        },
        vec![event("Zenith"), event("Backstage")],
    );
    refresh(
        &conn,
        &[Box::new(first)],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
    )
    .unwrap();
    conn.execute(
        "UPDATE concert_events SET seen_at = 77 WHERE venue = 'Zenith'",
        [],
    )
    .unwrap();
    let second = FakeProvider::new(
        ProviderKind::Ticketmaster,
        Resolution::Resolved {
            provider_id: "tm-id".into(),
            mbid_verified: false,
        },
        vec![event("Zenith")],
    );
    refresh(
        &conn,
        &[Box::new(second)],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        90_000,
        false,
    )
    .unwrap();

    let stored: (i64, Option<i64>) = conn
        .query_row(
            "SELECT COUNT(*), MAX(seen_at) FROM concert_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored, (1, Some(77)));
}

#[test]
fn fresh_negative_resolution_blocks_even_a_forced_refresh() {
    let conn = conn();
    seed_play(&conn, "Unknown", 1_000);
    let provider = FakeProvider::new(ProviderKind::Bandsintown, Resolution::Unmatched, Vec::new());
    refresh(
        &conn,
        &[Box::new(provider)],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
    )
    .unwrap();
    let provider = FakeProvider::new(
        ProviderKind::Bandsintown,
        Resolution::Resolved {
            provider_id: "now-known".into(),
            mbid_verified: false,
        },
        vec![event("Zenith")],
    );
    let summary = refresh(
        &conn,
        &[Box::new(provider)],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_001,
        true,
    )
    .unwrap();
    assert_eq!(summary.attempted, 0);
    assert_eq!(summary.events_upserted, 0);
}

#[test]
fn failed_refresh_preserves_cached_events_and_events_found() {
    let conn = conn();
    seed_play(&conn, "Lorna Shore", 1_000);
    let first = FakeProvider::new(
        ProviderKind::Ticketmaster,
        Resolution::Resolved {
            provider_id: "tm-id".into(),
            mbid_verified: false,
        },
        vec![event("Zenith")],
    );
    refresh(
        &conn,
        &[Box::new(first)],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
    )
    .unwrap();

    let summary = refresh(
        &conn,
        &[Box::new(FailingEventsProvider)],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        90_000,
        false,
    )
    .unwrap();
    assert_eq!(summary.failed, 1);
    let ledger: (String, i64) = conn
        .query_row(
            "SELECT last_outcome, events_found FROM concert_artists",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(ledger, ("failed".into(), 1));
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM concert_events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn cleanup_removes_past_events_and_missing_credentials_is_typed() {
    let conn = conn();
    conn.execute(
        "INSERT INTO concert_events (
           artist_key, artist_name, starts_at, date_key, venue, city,
           provider, fetched_at, dedupe_key
         ) VALUES ('past', 'Past', '2026-01-01T00:00:00', '2026-01-01',
                   'Old', 'Town', 'bandsintown', 1, 'past|town|old')",
        [],
    )
    .unwrap();
    seed_play(&conn, "Artist", 1_000);
    let result = refresh(
        &conn,
        &[],
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
        1_000,
        false,
    );
    assert!(matches!(result, Err(ConcertError::MissingCredentials)));
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM concert_events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
