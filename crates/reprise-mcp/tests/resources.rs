//! Resource discovery and reads for library, source, and upcoming-concert
//! metadata, plus their leak-matrix negatives.

mod common;

use common::{assert_no_leaks, McpClient, SeedTrack};
use serde_json::Value;
use tempfile::TempDir;

const SUMMARY_URI: &str = "reprise://library/summary";
const PLAYLISTS_URI: &str = "reprise://playlists";
const CONCERTS_URI: &str = "reprise://concerts";
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
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
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
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    reprise_core::library::playlists::create(&conn, "Favourites").unwrap();
    drop(conn);

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
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    let subscription_id = store::add_or_restore(
        &conn,
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
        &conn,
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
    .episode_id;
    store::save_position(&conn, episode_id, 12_000).unwrap();
    store::set_downloaded_path(
        &conn,
        episode_id,
        Some("/home/marvin/.local/share/reprise/podcasts/secret.mp3"),
    )
    .unwrap();
    drop(conn);

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
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    let station_id = reprise_core::radio::station::add_or_restore(
        &conn,
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
    drop(conn);

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
