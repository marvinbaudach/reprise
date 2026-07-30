//! Resource discovery and reads for library, source, concerts, and releases
//! metadata, plus their leak-matrix negatives.

mod common;

use common::{assert_no_leaks, McpClient, SeedTrack};
use serde_json::Value;
use tempfile::TempDir;

const SUMMARY_URI: &str = "reprise://library/summary";
const PLAYLISTS_URI: &str = "reprise://playlists";
const CONCERTS_URI: &str = "reprise://concerts";
const CONCERTS_ALL_URI: &str = "reprise://concerts/all";
const RELEASES_URI: &str = "reprise://releases";
const PODCASTS_URI: &str = "reprise://podcasts";
const RADIO_URI: &str = "reprise://radio";

fn fixture_db(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("reprise.db");
    common::seed_tracks(
        &path,
        &[
            SeedTrack::simple("Alpha", "Artist One"),
            SeedTrack::simple("Beta", "Artist One"),
            SeedTrack::simple("Gamma", "Artist Two"),
        ],
    );
    path
}

/// Reads the JSON body out of a `resources/read` response's first text content.
fn resource_body(response: &Value) -> Value {
    let text = response
        .get("result")
        .and_then(|result| result.get("contents"))
        .and_then(Value::as_array)
        .and_then(|contents| contents.first())
        .and_then(|content| content.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("expected text resource content: {response}"));
    serde_json::from_str(text).expect("resource body is JSON")
}

#[test]
fn lists_all_resources() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let mut client = McpClient::start(&path);

    let response = client.request("resources/list", serde_json::json!({}));
    let resources = response
        .get("result")
        .and_then(|result| result.get("resources"))
        .and_then(Value::as_array)
        .expect("resources array");
    let uris: Vec<&str> = resources
        .iter()
        .filter_map(|resource| resource.get("uri").and_then(Value::as_str))
        .collect();

    assert!(
        uris.contains(&SUMMARY_URI),
        "missing summary resource: {uris:?}"
    );
    assert!(
        uris.contains(&PLAYLISTS_URI),
        "missing playlists resource: {uris:?}"
    );
    assert!(
        uris.contains(&CONCERTS_URI),
        "missing concerts resource: {uris:?}"
    );
    assert!(
        uris.contains(&CONCERTS_ALL_URI),
        "missing complete concerts resource: {uris:?}"
    );
    assert!(
        uris.contains(&RELEASES_URI),
        "missing releases resource: {uris:?}"
    );
    assert!(
        uris.contains(&PODCASTS_URI),
        "missing podcasts resource: {uris:?}"
    );
    assert!(
        uris.contains(&RADIO_URI),
        "missing radio resource: {uris:?}"
    );
}

#[test]
fn reads_filtered_concerts_without_paths() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let conn = common::fixture_connection(&path);
    conn.execute(
        "INSERT INTO concert_artists (
           artist_key, artist_name, last_attempt_at, last_outcome, events_found
         ) VALUES ('artist', 'Artist One', 4321, 'ok', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO concert_events (
           artist_key, artist_name, starts_at, date_key, venue, city, country,
           ticket_url, ticket_source, event_url, provider, fetched_at, dedupe_key
         ) VALUES (
           'artist', 'Artist One', '2099-10-17T19:00:00', '2099-10-17',
           'Zenith', 'Munich', 'DE', 'https://tickets.example/1',
           'Ticketmaster', 'https://events.example/1', 'ticketmaster', 4321,
           '2099-10-17|munich|zenith'
         )",
        [],
    )
    .unwrap();
    drop(conn);
    drop(db);
    let mut client = McpClient::start(&path);

    let response = client.read_resource(CONCERTS_URI);
    let body = resource_body(&response);
    assert_eq!(body["latest_fetch_at"], 4321);
    assert_eq!(body["filter_applied"], true);
    assert_eq!(body["events"][0]["artist"], "Artist One");
    assert_eq!(body["events"][0]["date"], "2099-10-17");
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}

#[test]
fn reads_every_stored_concert_field_without_credentials_or_paths() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let conn = common::fixture_connection(&path);
    reprise_core::library::settings::set_setting(
        &db,
        reprise_core::concerts::config::TICKETMASTER_API_KEY,
        "must-never-leave-the-database",
    )
    .unwrap();
    reprise_core::library::settings::set_setting(
        &db,
        reprise_core::location::LOCATION_NAME_KEY,
        "Zurich",
    )
    .unwrap();
    reprise_core::library::settings::set_setting(
        &db,
        reprise_core::location::LOCATION_LAT_KEY,
        "47.3769",
    )
    .unwrap();
    reprise_core::library::settings::set_setting(
        &db,
        reprise_core::location::LOCATION_LON_KEY,
        "8.5417",
    )
    .unwrap();
    reprise_core::library::settings::set_setting(
        &db,
        reprise_core::concerts::config::FILTER_COUNTRY_KEY,
        "DE",
    )
    .unwrap();
    reprise_core::library::settings::set_setting(
        &db,
        reprise_core::concerts::config::FILTER_HORIZON_KEY,
        "next_6_months",
    )
    .unwrap();
    reprise_core::library::settings::set_bool(
        &db,
        reprise_core::concerts::config::FILTER_INCLUDE_SIMILAR_KEY,
        true,
    )
    .unwrap();
    reprise_core::library::settings::set_bool(
        &db,
        reprise_core::concerts::config::SIMILAR_ENABLED_KEY,
        true,
    )
    .unwrap();
    reprise_core::library::settings::set_setting(
        &db,
        reprise_core::concerts::config::SIMILAR_COUNT_KEY,
        "25",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO concert_events (
           id, artist_key, artist_name, starts_at, date_key, venue, city,
           region, country, latitude, longitude, ticket_url, ticket_source,
           event_url, provider, is_similar, similar_to, fetched_at, seen_at,
           dedupe_key
         ) VALUES (
           77, 'artist-one', 'Artist One', '2025-10-17T19:00:00',
           '2025-10-17', 'Zenith', 'Munich', 'Bavaria', 'DE', 48.1351,
           11.5820, 'https://tickets.example/77', 'Ticketmaster',
           'https://events.example/77', 'ticketmaster', 1, 'Seed Artist',
           4321, 5432, '2025-10-17|munich|zenith'
         )",
        [],
    )
    .unwrap();
    drop(conn);
    drop(db);
    let mut client = McpClient::start(&path);

    let response = client.read_resource(CONCERTS_ALL_URI);
    let body = resource_body(&response);
    let event = &body["events"][0];
    assert_eq!(event["id"], 77);
    assert_eq!(event["artist_key"], "artist-one");
    assert_eq!(event["artist_name"], "Artist One");
    assert_eq!(event["starts_at"], "2025-10-17T19:00:00");
    assert_eq!(event["date_key"], "2025-10-17");
    assert_eq!(event["venue"], "Zenith");
    assert_eq!(event["city"], "Munich");
    assert_eq!(event["region"], "Bavaria");
    assert_eq!(event["country"], "DE");
    assert_eq!(event["latitude"], 48.1351);
    assert_eq!(event["longitude"], 11.5820);
    assert_eq!(event["ticket_url"], "https://tickets.example/77");
    assert_eq!(event["ticket_source"], "Ticketmaster");
    assert_eq!(event["event_url"], "https://events.example/77");
    assert_eq!(event["provider"], "ticketmaster");
    assert_eq!(event["is_similar"], true);
    assert_eq!(event["similar_to"], "Seed Artist");
    assert_eq!(event["fetched_at"], 4321);
    assert_eq!(event["seen_at"], 5432);
    assert_eq!(event["dedupe_key"], "2025-10-17|munich|zenith");
    assert_eq!(body["location"]["name"], "Zurich");
    assert_eq!(body["location"]["latitude"], 47.3769);
    assert_eq!(body["location"]["longitude"], 8.5417);
    assert_eq!(body["filter"]["radius_km"], 1000.0);
    assert_eq!(body["filter"]["country"], "DE");
    assert_eq!(body["filter"]["horizon"], "next_6_months");
    assert_eq!(body["filter"]["include_similar"], true);
    assert_eq!(body["window_days"], 90);
    assert_eq!(body["similar_artists"]["enabled"], true);
    assert_eq!(body["similar_artists"]["count"], 25);
    assert_eq!(body["providers"]["ticketmaster"], true);
    assert_eq!(body["providers"]["bandsintown"], false);
    let raw = serde_json::to_string(&response).unwrap();
    assert!(!raw.contains("must-never-leave-the-database"));
    assert_no_leaks(&raw);
}

#[test]
fn reads_every_stored_release_field_including_hidden_history() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let conn = common::fixture_connection(&path);
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, seen_at, hidden, fallback_accent,
           first_seen, hidden_at, announce_url, track_count
         ) VALUES (
           'release-group-1', 'Artist One', 'artist-mbid-1',
           'Artist One Album', 'Album', '2025-08-15', 1000, 1100, 1,
           '#123456', 900, 1200, 'https://musicbrainz.example/release-group-1',
           2
         )",
        [],
    )
    .unwrap();
    drop(conn);
    drop(db);
    let mut client = McpClient::start(&path);

    let response = client.read_resource(RELEASES_URI);
    let body = resource_body(&response);
    let release = &body["releases"][0];
    assert_eq!(release["release_group_mbid"], "release-group-1");
    assert_eq!(release["artist_name"], "Artist One");
    assert_eq!(release["artist_mbid"], "artist-mbid-1");
    assert_eq!(release["title"], "Artist One Album");
    assert_eq!(release["release_type"], "Album");
    assert_eq!(release["first_release_date"], "2025-08-15");
    assert_eq!(release["fetched_at"], 1000);
    assert_eq!(release["seen_at"], 1100);
    assert_eq!(release["hidden"], true);
    assert_eq!(release["fallback_accent"], "#123456");
    assert_eq!(release["first_seen"], 900);
    assert_eq!(release["hidden_at"], 1200);
    assert_eq!(
        release["announce_url"],
        "https://musicbrainz.example/release-group-1"
    );
    assert_eq!(release["track_count"], 2);
    assert_eq!(release["local_track_count"], 2);
    assert_eq!(release["library_presence"], "complete");
    assert_eq!(release["history_status"], "hidden");
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}

#[test]
fn reads_library_summary() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let mut client = McpClient::start(&path);

    let response = client.read_resource(SUMMARY_URI);
    let body = resource_body(&response);

    assert_eq!(body.get("track_count").and_then(Value::as_i64), Some(3));
    assert_eq!(body.get("artist_count").and_then(Value::as_i64), Some(2));
    assert_eq!(body.get("album_count").and_then(Value::as_i64), Some(2));
    assert_eq!(
        body.get("total_duration_ms").and_then(Value::as_i64),
        Some(540_000)
    );
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}

#[test]
fn reads_playlists_resource() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    // Create a playlist through the core facade so the resource has content.
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    reprise_core::library::playlists::create(&db, "Favourites").unwrap();
    drop(db);

    let mut client = McpClient::start(&path);
    let response = client.read_resource(PLAYLISTS_URI);
    let body = resource_body(&response);
    let playlists = body.get("playlists").and_then(Value::as_array).unwrap();

    assert_eq!(playlists.len(), 1);
    assert_eq!(
        playlists[0].get("name").and_then(Value::as_str),
        Some("Favourites")
    );
    assert_eq!(
        playlists[0].get("track_count").and_then(Value::as_i64),
        Some(0)
    );
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}

#[test]
fn reads_cached_podcasts_without_source_or_download_paths() {
    use reprise_core::podcasts::feed::ParsedEpisode;
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let subscription_id = store::add_or_restore(
        &db,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://feeds.example.test/private?token=secret".into(),
            title: "German Metal Talks".into(),
            author: Some("Host One".into()),
            image_url: Some("https://images.example.test/private?token=secret".into()),
            auto_download: true,
        },
        100,
    )
    .unwrap();
    let episode_id = store::upsert_episode(
        &db,
        subscription_id,
        &ParsedEpisode {
            guid: "episode-one".into(),
            title: "Episode One".into(),
            audio_url: "https://audio.example.test/private?token=secret".into(),
            page_url: Some("https://pages.example.test/private?token=secret".into()),
            published_at: Some(200),
            duration_secs: Some(1_800),
        },
        210,
    )
    .unwrap()
    .expect("episode should be imported")
    .episode_id;
    store::save_position(&db, episode_id, 12_000).unwrap();
    store::set_downloaded_path(
        &db,
        episode_id,
        Some("/home/marvin/.local/share/reprise/podcasts/secret.mp3"),
    )
    .unwrap();
    drop(db);

    let mut client = McpClient::start(&path);
    let response = client.read_resource(PODCASTS_URI);
    let body = resource_body(&response);

    assert_eq!(body["subscription_total"], 1);
    assert_eq!(body["episode_total"], 1);
    assert_eq!(body["subscriptions"][0]["id"], subscription_id);
    assert_eq!(body["subscriptions"][0]["kind"], "rss");
    assert_eq!(body["subscriptions"][0]["title"], "German Metal Talks");
    assert_eq!(body["episodes"][0]["id"], episode_id);
    assert_eq!(body["episodes"][0]["downloaded"], true);
    assert_eq!(body["episodes"][0]["played"], false);
    assert_eq!(body["episodes"][0]["position_ms"], 12_000);
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}

#[test]
fn reads_cached_radio_favorites_without_stream_urls() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let station_id = reprise_core::radio::station::add_or_restore(
        &db,
        &reprise_core::radio::station::NewStation {
            uuid: Some("metal-one".into()),
            name: "Metal One".into(),
            stream_url: "https://radio.example.test/live?token=secret".into(),
            homepage: Some("https://radio.example.test/private?token=secret".into()),
            favicon_url: Some("https://radio.example.test/icon?token=secret".into()),
            genre: Some("Metal".into()),
            codec: Some("MP3".into()),
            bitrate_kbps: Some(192),
            country_code: Some("DE".into()),
            votes: Some(42),
        },
        300,
    )
    .unwrap();
    drop(db);

    let mut client = McpClient::start(&path);
    let response = client.read_resource(RADIO_URI);
    let body = resource_body(&response);

    assert_eq!(body["station_total"], 1);
    assert_eq!(body["stations"][0]["id"], station_id);
    assert_eq!(body["stations"][0]["name"], "Metal One");
    assert_eq!(body["stations"][0]["genre"], "Metal");
    assert_eq!(body["stations"][0]["codec"], "MP3");
    assert_eq!(body["stations"][0]["bitrate_kbps"], 192);
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}

#[test]
fn unknown_resource_is_a_protocol_error() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let mut client = McpClient::start(&path);

    let response = client.read_resource("reprise://does/not/exist");
    assert!(
        response.get("result").is_none(),
        "should not succeed: {response}"
    );
    assert!(
        response.get("error").is_some(),
        "should be a protocol error: {response}"
    );
}

#[test]
fn summary_reflects_empty_library_without_leaking() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    let mut client = McpClient::start(&path);

    let response = client.read_resource(SUMMARY_URI);
    let body = resource_body(&response);
    assert_eq!(body.get("track_count").and_then(Value::as_i64), Some(0));
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}
