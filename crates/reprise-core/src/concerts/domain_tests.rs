use chrono::NaiveDate;
use rusqlite::{params, Connection};

use super::{
    artist_due, config, count_unseen, count_upcoming, geocode_url, haversine_km, jitter_seconds,
    mark_scope_seen, parse_geocode, query_events, query_unseen, refresh_due, ConcertFilter,
    DateHorizon,
};

fn conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

fn insert_event(
    conn: &Connection,
    id: i64,
    date: &str,
    country: &str,
    latitude: Option<f64>,
    is_similar: bool,
) {
    conn.execute(
        "INSERT INTO concert_events (
           id, artist_key, artist_name, starts_at, date_key, venue, city,
           country, latitude, longitude, provider, is_similar, fetched_at,
           seen_at, dedupe_key
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 11.58,
                   'bandsintown', ?10, 100, NULL, ?11)",
        params![
            id,
            format!("artist-{id}"),
            format!("Artist {id}"),
            format!("{date}T19:00:00"),
            date,
            format!("Venue {id}"),
            if id == 3 { "Berlin" } else { "Munich" },
            country,
            latitude,
            i64::from(is_similar),
            format!("{date}|city|venue-{id}")
        ],
    )
    .unwrap();
}

#[test]
fn haversine_matches_the_munich_berlin_reference() {
    let distance = haversine_km(48.1372, 11.5756, 52.5200, 13.4050);
    assert!((distance - 504.0).abs() <= 5.04, "{distance}");
    assert_eq!(haversine_km(48.0, 11.0, 48.0, 11.0), 0.0);
}

#[test]
fn geocode_url_and_parser_are_tolerant_and_pure() {
    assert_eq!(
        geocode_url("München & Umgebung"),
        "https://nominatim.openstreetmap.org/search?q=M%C3%BCnchen%20%26%20Umgebung&format=json&limit=1&addressdetails=1"
    );
    let location =
        parse_geocode(r#"[{"lat":"48.13","lon":"11.57","display_name":"Munich, Bavaria"}]"#)
            .unwrap()
            .unwrap();
    assert_eq!(location.lat, 48.13);
    assert_eq!(location.display_name, "Munich, Bavaria");
    assert_eq!(location.country_code, None);
    assert_eq!(parse_geocode("[]").unwrap(), None);
    assert!(parse_geocode("{broken").is_err());
}

/// `RAD-5`: "Metal in DE" and "Near you" both need a country code, and the
/// only source Reprise is allowed to use is data already present in the
/// forward-geocode response city search already makes — never a second,
/// reverse-geocoding network call (`O-4`). Nominatim's `addressdetails=1`
/// (added to the existing request above) returns a structured, lowercase
/// ISO 3166-1 alpha-2 `country_code`, which this normalizes to uppercase to
/// match radio-browser's own convention (`radio::search::StationCandidate`).
#[test]
fn rad_5_geocode_parses_the_addressdetails_country_code_when_present() {
    let with_country = parse_geocode(
        r#"[{"lat":"52.52","lon":"13.405","display_name":"Berlin, Deutschland",
             "address":{"country_code":"de"}}]"#,
    )
    .unwrap()
    .unwrap();
    assert_eq!(with_country.country_code.as_deref(), Some("DE"));

    // No `address` object at all (e.g. an older cached fixture, or a
    // provider response that omitted it) — honestly `None`, never guessed.
    let without_address =
        parse_geocode(r#"[{"lat":"48.13","lon":"11.57","display_name":"Munich, Bavaria"}]"#)
            .unwrap()
            .unwrap();
    assert_eq!(without_address.country_code, None);
}

#[test]
fn config_defaults_are_bounded_and_stored_credentials_win() {
    let conn = conn();
    assert_eq!(
        config::credentials_with_env(&conn, |_| Some("environment".into()), None)
            .unwrap()
            .bandsintown_app_id
            .as_deref(),
        Some("environment")
    );
    assert_eq!(config::window_days(&conn).unwrap(), 90);
    assert_eq!(config::similar_config(&conn).unwrap().count, 10);
    assert_eq!(
        config::persisted_filter(&conn).unwrap().radius_km,
        Some(1_000.0)
    );
    assert_eq!(config::RADIUS_PRESETS_KM, [100, 250, 500, 1_000]);
    crate::library::settings::set_setting(&conn, "concerts.window_days", "999").unwrap();
    crate::library::settings::set_setting(&conn, "concerts.similar_count", "0").unwrap();
    crate::library::settings::set_setting(&conn, "concerts.bandsintown_app_id", "stored").unwrap();
    assert_eq!(config::window_days(&conn).unwrap(), 365);
    assert_eq!(config::similar_config(&conn).unwrap().count, 1);
    assert_eq!(
        config::credentials_with_env(&conn, |_| Some("environment".into()), None)
            .unwrap()
            .bandsintown_app_id
            .as_deref(),
        Some("stored")
    );
}

#[test]
fn ticketmaster_credentials_fall_back_to_the_bundled_build_value() {
    let conn = conn();
    let credentials =
        config::credentials_with_env(&conn, |_| None, Some("  dummy-bundled-ticketmaster-key  "))
            .unwrap();
    assert_eq!(
        credentials.ticketmaster_api_key.as_deref(),
        Some("dummy-bundled-ticketmaster-key")
    );

    let credentials = config::credentials_with_env(
        &conn,
        |_| Some(" \t ".to_owned()),
        Some("dummy-bundled-ticketmaster-key"),
    )
    .unwrap();
    assert_eq!(
        credentials.ticketmaster_api_key.as_deref(),
        Some("dummy-bundled-ticketmaster-key")
    );

    let credentials = config::credentials_with_env(&conn, |_| None, Some(" \t ")).unwrap();
    assert_eq!(credentials.ticketmaster_api_key, None);
}

#[test]
fn ticketmaster_credentials_prefer_stored_then_runtime_then_build() {
    let conn = conn();
    let read_runtime = |key: &str| {
        (key == "REPRISE_TICKETMASTER_APIKEY")
            .then(|| "  dummy-runtime-ticketmaster-key  ".to_owned())
    };
    let credentials =
        config::credentials_with_env(&conn, read_runtime, Some("dummy-bundled-ticketmaster-key"))
            .unwrap();
    assert_eq!(
        credentials.ticketmaster_api_key.as_deref(),
        Some("dummy-runtime-ticketmaster-key")
    );

    crate::library::settings::set_setting(
        &conn,
        config::TICKETMASTER_API_KEY,
        "  dummy-stored-ticketmaster-key  ",
    )
    .unwrap();
    let credentials =
        config::credentials_with_env(&conn, read_runtime, Some("dummy-bundled-ticketmaster-key"))
            .unwrap();
    assert_eq!(
        credentials.ticketmaster_api_key.as_deref(),
        Some("dummy-stored-ticketmaster-key")
    );

    crate::library::settings::set_setting(&conn, config::TICKETMASTER_API_KEY, "   ").unwrap();
    let credentials =
        config::credentials_with_env(&conn, read_runtime, Some("dummy-bundled-ticketmaster-key"))
            .unwrap();
    assert_eq!(
        credentials.ticketmaster_api_key.as_deref(),
        Some("dummy-runtime-ticketmaster-key")
    );

    let credentials =
        config::credentials_with_env(&conn, |_| Some("   ".to_owned()), None).unwrap();
    assert_eq!(credentials.ticketmaster_api_key, None);
}

#[test]
fn credential_debug_output_redacts_secret_values() {
    let credentials = config::Credentials {
        bandsintown_app_id: Some("dummy-bandsintown-secret".to_owned()),
        ticketmaster_api_key: Some("dummy-ticketmaster-secret".to_owned()),
    };

    let debug = format!("{credentials:?}");
    assert!(!debug.contains("dummy-bandsintown-secret"));
    assert!(!debug.contains("dummy-ticketmaster-secret"));
    assert!(debug.contains("<redacted>"));
}

#[test]
fn candidates_use_recent_plays_and_oldest_attempt_first() {
    let conn = conn();
    for (artist, mbid, played_at) in [
        ("Frequent", Some("frequent-id"), 950),
        ("Frequent", Some("frequent-id"), 960),
        ("Fresh", None, 970),
        ("Expired", None, 100),
    ] {
        conn.execute(
            "INSERT INTO listen_events (
               track_id, played_at, ms_played, artist, artist_mbid
             ) VALUES (1, ?1, 1, ?2, ?3)",
            params![played_at, artist, mbid],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO concert_artists (
           artist_key, artist_name, last_attempt_at
         ) VALUES ('frequent', 'Frequent', 900)",
        [],
    )
    .unwrap();

    let rows = super::candidates::library_candidates(&conn, 900).unwrap();
    assert_eq!(
        rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
        vec!["Fresh", "Frequent"]
    );
    assert_eq!(rows[1].plays, 2);
    assert_eq!(rows[1].mbid.as_deref(), Some("frequent-id"));
    let seeds = super::candidates::seed_artists(&conn, 900, 1).unwrap();
    assert_eq!(seeds[0].name, "Frequent");
}

#[test]
fn refresh_policy_has_exact_boundaries_and_stable_jitter() {
    let now = 1_000_000;
    assert!(artist_due(None, now, false));
    assert!(!artist_due(Some(now - 86_399), now, false));
    assert!(artist_due(Some(now - 86_400), now, false));
    assert!(artist_due(Some(now - 1), now, true));
    assert!(!artist_due(Some(now + 1), now, false));
    assert!(refresh_due(None, now, 7));
    assert!(!refresh_due(Some(now - 86_400), now, 7));
    assert!(refresh_due(Some(now - 86_407), now, 7));
    assert_eq!(jitter_seconds("db"), jitter_seconds("db"));
    assert!((0..=7_200).contains(&jitter_seconds("db")));
}

#[test]
fn query_filters_distance_country_horizon_and_similar_rows() {
    let conn = conn();
    insert_event(&conn, 1, "2026-08-01", "DE", Some(48.14), false);
    insert_event(&conn, 2, "2026-08-02", "CH", Some(47.38), true);
    insert_event(&conn, 3, "2027-01-01", "DE", Some(52.52), false);
    insert_event(&conn, 4, "2026-08-03", "DE", None, false);
    let today = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
    let filter = ConcertFilter {
        radius_km: Some(100.0),
        country: Some("DE".into()),
        horizon: DateHorizon::Next3Months,
        include_similar: false,
    };
    let location = Some(crate::location::AppLocation {
        latitude: 48.1372,
        longitude: 11.5756,
        name: "Munich".into(),
        country_code: Some("DE".into()),
    });

    let rows = query_events(&conn, &filter, location.as_ref(), today).unwrap();
    assert_eq!(rows.iter().map(|row| row.id).collect::<Vec<_>>(), vec![1]);
    assert_eq!(
        count_upcoming(&conn, &filter, location.as_ref(), today).unwrap(),
        rows.len() as i64
    );
}

#[test]
fn seen_cycle_marks_only_the_current_filter_scope() {
    let conn = conn();
    insert_event(&conn, 1, "2026-08-01", "DE", Some(48.14), false);
    insert_event(&conn, 2, "2026-08-02", "CH", Some(47.38), false);
    let today = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
    let mut filter = ConcertFilter {
        country: Some("DE".into()),
        ..ConcertFilter::default()
    };

    assert_eq!(
        query_unseen(&conn, &filter, None, today, 3).unwrap().len(),
        1
    );
    assert_eq!(count_unseen(&conn, &filter, None, today).unwrap(), 1);
    assert_eq!(
        mark_scope_seen(&conn, &filter, None, today, 500).unwrap(),
        1
    );
    assert_eq!(count_unseen(&conn, &filter, None, today).unwrap(), 0);
    filter.country = None;
    assert_eq!(count_unseen(&conn, &filter, None, today).unwrap(), 1);
}
