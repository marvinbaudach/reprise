mod common;

use common::{code, parse_json, stdout, Harness};
use rusqlite::params;

fn seed_concerts(harness: &Harness) {
    let db = harness.db();
    let conn = harness.fixture_connection();
    conn.execute(
        "INSERT INTO concert_artists (
           artist_key, artist_name, last_attempt_at, last_outcome, events_found
         ) VALUES ('seed', 'Seed Artist', 1234, 'ok', 3)",
        [],
    )
    .unwrap();
    for (index, artist, country, similar) in [
        (1, "Lorna Shore", "DE", 0),
        (2, "Architects", "GB", 0),
        (3, "Similar Act", "DE", 1),
    ] {
        conn.execute(
            "INSERT INTO concert_events (
               id, artist_key, artist_name, starts_at, date_key, venue, city,
               region, country, latitude, longitude, ticket_url,
               ticket_source, event_url, provider, is_similar, similar_to,
               fetched_at, dedupe_key
             ) VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, NULL, NULL, ?9,
               'Ticketmaster', ?10, 'ticketmaster', ?11, ?12, 1234, ?13
             )",
            params![
                index,
                format!("artist-{index}"),
                artist,
                format!("2099-10-{index:02}T19:00:00"),
                format!("2099-10-{index:02}"),
                format!("Venue {index}"),
                format!("City {index}"),
                country,
                format!("https://tickets.example/{index}"),
                format!("https://events.example/{index}"),
                similar,
                (similar == 1).then_some("Lorna Shore"),
                format!("2099-10-{index:02}|city {index}|venue {index}"),
            ],
        )
        .unwrap();
    }
    reprise_core::library::settings::set_setting(
        &db,
        reprise_core::concerts::config::FILTER_COUNTRY_KEY,
        "DE",
    )
    .unwrap();
}

#[test]
fn json_applies_saved_filters_and_has_the_documented_shape() {
    let harness = Harness::new();
    seed_concerts(&harness);

    let output = harness.run(&["--json", "concerts", "list"]);
    assert_eq!(code(&output), 0);
    let value = parse_json(&output);
    assert_eq!(value["filter_applied"], true);
    assert_eq!(value["latest_fetch_at"], 1234);
    let events = value["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["date"], "2099-10-01");
    assert_eq!(events[0]["artist"], "Lorna Shore");
    assert_eq!(events[0]["distance_km"], serde_json::Value::Null);
    assert_eq!(events[0]["provider"], "ticketmaster");
    assert_eq!(events[0]["is_similar"], false);
    assert!(events[0].get("path").is_none());
}

#[test]
fn all_ignores_saved_filters_and_limit_caps_the_output() {
    let harness = Harness::new();
    seed_concerts(&harness);

    let output = harness.run(&["concerts", "list", "--all", "--json"]);
    assert_eq!(code(&output), 0);
    let value = parse_json(&output);
    assert_eq!(value["filter_applied"], false);
    assert_eq!(value["events"].as_array().unwrap().len(), 3);
    assert_eq!(value["events"][2]["is_similar"], true);

    let output = harness.run(&["concerts", "list", "--all", "--limit", "2", "--json"]);
    let value = parse_json(&output);
    assert_eq!(value["events"].as_array().unwrap().len(), 2);
}

#[test]
fn human_output_is_one_safe_line_per_event() {
    let harness = Harness::new();
    seed_concerts(&harness);

    let output = harness.run(&["concerts", "list"]);
    assert_eq!(code(&output), 0);
    assert_eq!(
        stdout(&output).trim(),
        "2099-10-01  Lorna Shore — Venue 1, City 1 (DE) · https://tickets.example/1"
    );
}

#[test]
fn empty_cache_returns_an_empty_success() {
    let harness = Harness::new();
    let output = harness.run(&["--json", "concerts", "list"]);
    assert_eq!(code(&output), 0);
    let value = parse_json(&output);
    assert_eq!(value["events"].as_array().unwrap().len(), 0);
    assert_eq!(value["latest_fetch_at"], serde_json::Value::Null);
}
