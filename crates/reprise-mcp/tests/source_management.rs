//! Capability-gated podcast/YouTube and radio source management.

mod common;

use common::{assert_no_leaks, set_bool_setting, structured_ok, tool_error_text, McpClient};
use serde_json::{json, Value};
use tempfile::TempDir;

const CAP_SOURCES_MANAGE: &str = "agent.capability.sources:manage";

fn fixture_db(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    path
}

fn resource_body(response: &Value) -> Value {
    let text = response["result"]["contents"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("expected text resource: {response}"));
    serde_json::from_str(text).unwrap()
}

#[test]
fn source_management_is_off_by_default_restart_gated_and_immediately_revocable() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let mut denied = McpClient::start(&path);

    let default_denial =
        denied.call_tool("music_manage_podcasts", json!({ "action": "unsupported" }));
    assert!(tool_error_text(&default_denial).contains("sources:manage"));

    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let restart_denial =
        denied.call_tool("music_manage_podcasts", json!({ "action": "unsupported" }));
    assert!(tool_error_text(&restart_denial).contains("sources:manage"));
    drop(denied);

    let mut granted = McpClient::start(&path);
    let unsupported =
        granted.call_tool("music_manage_podcasts", json!({ "action": "unsupported" }));
    assert!(tool_error_text(&unsupported).contains("unknown podcast action"));

    set_bool_setting(&path, CAP_SOURCES_MANAGE, false);
    let revoked = granted.call_tool("music_manage_podcasts", json!({ "action": "unsupported" }));
    assert!(tool_error_text(&revoked).contains("sources:manage"));
}

#[test]
fn lists_both_source_management_tools_with_object_input_schemas() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let mut client = McpClient::start(&path);
    let response = client.request("tools/list", json!({}));
    let tools = response["result"]["tools"].as_array().unwrap();

    for name in ["music_manage_podcasts", "music_manage_radio"] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing tool {name}: {response}"));
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
}

#[test]
fn adds_and_imports_an_rss_feed_through_the_real_mcp_boundary() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let fixtures = TempDir::new().unwrap();
    std::fs::write(
        fixtures
            .path()
            .join("feed-https___feeds.example.test_show.xml"),
        r#"<?xml version="1.0"?>
        <rss version="2.0"><channel>
          <title>German Metal Talks</title>
          <author>Host One</author>
          <item>
            <guid>episode-two</guid><title>Episode Two</title>
            <enclosure url="https://audio.example.test/two.mp3" type="audio/mpeg"/>
            <pubDate>Sat, 25 Jul 2026 10:00:00 +0000</pubDate>
          </item>
          <item>
            <guid>episode-one</guid><title>Episode One</title>
            <enclosure url="https://audio.example.test/one.mp3" type="audio/mpeg"/>
            <pubDate>Fri, 24 Jul 2026 10:00:00 +0000</pubDate>
          </item>
        </channel></rss>"#,
    )
    .unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_PODCASTS_FIXTURE_DIR", fixtures.path())]);

    let added = client.call_tool(
        "music_manage_podcasts",
        json!({
            "action": "add",
            "url": "https://feeds.example.test/show",
            "auto_download": false,
            "import_existing": true
        }),
    );
    let result = structured_ok(&added);
    assert_eq!(result["action"], "add");
    assert_eq!(result["kind"], "rss");
    assert_eq!(result["title"], "German Metal Talks");
    assert_eq!(result["episodes_affected"], 2);

    let cached = client.read_resource("reprise://podcasts");
    let body = resource_body(&cached);
    assert_eq!(body["subscription_total"], 1);
    assert_eq!(body["episode_total"], 2);
    assert_eq!(body["subscriptions"][0]["title"], "German Metal Talks");
    assert_eq!(body["episodes"][0]["title"], "Episode Two");
    assert_no_leaks(&serde_json::to_string(&added).unwrap());
    assert_no_leaks(&serde_json::to_string(&cached).unwrap());
}

#[cfg(unix)]
#[test]
fn adds_and_imports_a_youtube_channel_through_ytdlp() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let fake_ytdlp = dir.path().join("yt-dlp");
    std::fs::write(
        &fake_ytdlp,
        r#"#!/bin/sh
printf '%s\n' '{"title":"HOLLOW FALLEN","entries":[{"id":"video-two","title":"Video Two","duration":222},{"id":"video-one","title":"Video One","duration":111}]}'
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake_ytdlp, std::fs::Permissions::from_mode(0o755)).unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    set_bool_setting(&path, "online-sources-enabled", true);
    // YouTube is a peer module, independent of Podcasts (issue #96).
    set_bool_setting(&path, "module.youtube.enabled", true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_YTDLP_BIN", fake_ytdlp.as_path())]);

    let added = client.call_tool(
        "music_manage_podcasts",
        json!({
            "action": "add",
            "url": "https://www.youtube.com/@HOLLOWFALLEN",
            "import_existing": true
        }),
    );
    let result = structured_ok(&added);
    assert_eq!(result["kind"], "youtube");
    assert_eq!(result["title"], "HOLLOW FALLEN");
    assert_eq!(result["episodes_affected"], 2);

    let cached = client.read_resource("reprise://podcasts");
    let body = resource_body(&cached);
    assert_eq!(body["subscription_total"], 1);
    assert_eq!(body["episode_total"], 2);
    assert_eq!(body["subscriptions"][0]["kind"], "youtube");
    let titles = body["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|episode| episode["title"].as_str())
        .collect::<Vec<_>>();
    assert!(titles.contains(&"Video One"));
    assert!(titles.contains(&"Video Two"));
    assert_no_leaks(&serde_json::to_string(&added).unwrap());
}

#[test]
fn rad_8_adds_a_radio_favorite_with_the_favicon_from_an_exact_stream_match() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let fixtures = TempDir::new().unwrap();
    let stream_url = "https://radio.example.test/live";
    std::fs::write(
        fixtures.path().join("servers.json"),
        r#"[{"name":"fixture.radio-browser.test"}]"#,
    )
    .unwrap();
    std::fs::write(
        fixtures
            .path()
            .join("byurl-https___radio.example.test_live.json"),
        r#"[{"stationuuid":"station-one","name":"Metal One",
             "url_resolved":"https://radio.example.test/live",
             "favicon":"https://images.example.test/metal-one.ico","votes":12}]"#,
    )
    .unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    set_bool_setting(&path, "online-sources-enabled", true);
    set_bool_setting(&path, "module.radio.enabled", true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_RADIO_FIXTURE_DIR", fixtures.path())]);

    let added = structured_ok(&client.call_tool(
        "music_manage_radio",
        json!({
            "action": "add",
            "url": stream_url,
            "name": "Metal One"
        }),
    ));

    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let station = reprise_core::radio::station::get(&db, added["id"].as_i64().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        station.favicon_url.as_deref(),
        Some("https://images.example.test/metal-one.ico")
    );
}

#[test]
fn rad_8_a_failed_radio_favicon_lookup_never_blocks_adding_the_station() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let fixtures = TempDir::new().unwrap();
    std::fs::write(
        fixtures.path().join("servers.json"),
        r#"[{"name":"fixture.radio-browser.test"}]"#,
    )
    .unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    set_bool_setting(&path, "online-sources-enabled", true);
    set_bool_setting(&path, "module.radio.enabled", true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_RADIO_FIXTURE_DIR", fixtures.path())]);

    let added = structured_ok(&client.call_tool(
        "music_manage_radio",
        json!({
            "action": "add",
            "url": "https://radio.example.test/no-directory-entry",
            "name": "Unlisted Radio"
        }),
    ));

    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let station = reprise_core::radio::station::get(&db, added["id"].as_i64().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(station.name, "Unlisted Radio");
    assert_eq!(station.favicon_url, None);
}

#[test]
fn malformed_radio_browser_mirror_never_panics_radio_add() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let fixtures = TempDir::new().unwrap();
    std::fs::write(
        fixtures.path().join("servers.json"),
        r#"[{"name":"https://["}]"#,
    )
    .unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    set_bool_setting(&path, "online-sources-enabled", true);
    set_bool_setting(&path, "module.radio.enabled", true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_RADIO_FIXTURE_DIR", fixtures.path())]);

    let added = structured_ok(&client.call_tool(
        "music_manage_radio",
        json!({
            "action": "add",
            "url": "https://radio.example.test/live",
            "name": "Malformed Mirror Radio",
            "uuid": "station-one"
        }),
    ));

    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let station = reprise_core::radio::station::get(&db, added["id"].as_i64().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(station.name, "Malformed Mirror Radio");
    assert_eq!(station.favicon_url, None);
}

#[test]
fn edits_and_removes_a_subscription_without_deleting_downloads() {
    use reprise_core::podcasts::feed::ParsedEpisode;
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let download = dir.path().join("kept-episode.mp3");
    std::fs::write(&download, b"audio").unwrap();
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let subscription_id = store::add_or_restore(
        &db,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://feeds.example.test/show".into(),
            title: "Before".into(),
            author: None,
            image_url: None,
            auto_download: true,
        },
        100,
    )
    .unwrap();
    let episode_id = store::upsert_episode(
        &db,
        subscription_id,
        &ParsedEpisode {
            guid: "one".into(),
            title: "One".into(),
            image_url: None,
            audio_url: "https://audio.example.test/one.mp3".into(),
            page_url: None,
            published_at: Some(200),
            duration_secs: None,
        },
        210,
    )
    .unwrap()
    .expect("episode should be imported")
    .episode_id;
    store::set_downloaded_path(&db, episode_id, download.to_str()).unwrap();
    drop(db);
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let mut client = McpClient::start(&path);

    let edited = structured_ok(&client.call_tool(
        "music_manage_podcasts",
        json!({
            "action": "edit",
            "subscription_id": subscription_id,
            "title": "After",
            "auto_download": false
        }),
    ));
    assert_eq!(edited["action"], "edit");
    assert_eq!(edited["title"], "After");
    let body = resource_body(&client.read_resource("reprise://podcasts"));
    assert_eq!(body["subscriptions"][0]["title"], "After");
    assert_eq!(body["subscriptions"][0]["auto_download"], false);

    let removed = structured_ok(&client.call_tool(
        "music_manage_podcasts",
        json!({ "action": "remove", "subscription_id": subscription_id }),
    ));
    assert_eq!(removed["action"], "remove");
    let body = resource_body(&client.read_resource("reprise://podcasts"));
    assert_eq!(body["subscription_total"], 0);
    assert_eq!(body["episode_total"], 0);
    assert!(download.is_file(), "unsubscribe must keep downloaded media");
}

#[test]
fn refreshes_cached_rss_subscriptions_on_explicit_request() {
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    store::add_or_restore(
        &db,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://feeds.example.test/refresh".into(),
            title: "Before Refresh".into(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        100,
    )
    .unwrap();
    drop(db);
    let fixtures = TempDir::new().unwrap();
    std::fs::write(
        fixtures
            .path()
            .join("feed-https___feeds.example.test_refresh.xml"),
        r#"<?xml version="1.0"?>
        <rss version="2.0"><channel>
          <title>After Refresh</title>
          <item>
            <guid>new-episode</guid><title>New Episode</title>
            <enclosure url="https://audio.example.test/new.mp3" type="audio/mpeg"/>
          </item>
        </channel></rss>"#,
    )
    .unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    set_bool_setting(&path, "online-sources-enabled", true);
    // The refresh pipeline gates RSS on the Podcasts module (`NET-1a`) —
    // unlike "add", which is a one-off explicit action.
    set_bool_setting(&path, "module.podcasts.enabled", true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_PODCASTS_FIXTURE_DIR", fixtures.path())]);

    let refreshed =
        structured_ok(&client.call_tool("music_manage_podcasts", json!({ "action": "refresh" })));
    assert_eq!(refreshed["action"], "refresh");
    assert_eq!(refreshed["attempted"], 1);
    assert_eq!(refreshed["refreshed"], 1);
    assert_eq!(refreshed["episodes_inserted"], 1);

    let body = resource_body(&client.read_resource("reprise://podcasts"));
    assert_eq!(body["subscriptions"][0]["title"], "After Refresh");
    assert_eq!(body["episodes"][0]["title"], "New Episode");
}

#[test]
fn adds_edits_and_removes_a_radio_favorite_without_echoing_its_stream_url() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let mut client = McpClient::start(&path);

    let added_response = client.call_tool(
        "music_manage_radio",
        json!({
            "action": "add",
            "url": "https://radio.example.test/live?token=secret",
            "name": "Metal One",
            "homepage": "https://radio.example.test/",
            "favicon_url": "https://images.example.test/metal-one.png",
            "genre": "Metal",
            "codec": "MP3",
            "bitrate_kbps": 192,
            "country_code": "de"
        }),
    );
    let added = structured_ok(&added_response);
    let station_id = added["id"].as_i64().unwrap();
    assert_eq!(added["action"], "add");
    assert_eq!(added["name"], "Metal One");
    assert_eq!(added["country_code"], "DE");
    assert_no_leaks(&serde_json::to_string(&added_response).unwrap());
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let stored = reprise_core::radio::station::get(&db, station_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.homepage.as_deref(),
        Some("https://radio.example.test/")
    );
    assert_eq!(
        stored.favicon_url.as_deref(),
        Some("https://images.example.test/metal-one.png")
    );
    drop(db);

    let edited = structured_ok(&client.call_tool(
        "music_manage_radio",
        json!({
            "action": "edit",
            "station_id": station_id,
            "name": "Metal Two",
            "genre": "Doom",
            "url": "https://radio.example.test/new?token=secret"
        }),
    ));
    assert_eq!(edited["action"], "edit");
    assert_eq!(edited["name"], "Metal Two");
    assert_eq!(edited["genre"], "Doom");
    let body = resource_body(&client.read_resource("reprise://radio"));
    assert_eq!(body["stations"][0]["name"], "Metal Two");
    assert_eq!(body["stations"][0]["genre"], "Doom");

    let removed = structured_ok(&client.call_tool(
        "music_manage_radio",
        json!({ "action": "remove", "station_id": station_id }),
    ));
    assert_eq!(removed["action"], "remove");
    assert_eq!(
        resource_body(&client.read_resource("reprise://radio"))["station_total"],
        0
    );
}

#[test]
fn adds_a_url_only_radio_favorite_from_icy_metadata() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let fixtures = TempDir::new().unwrap();
    let stream_url = "https://radio.example.test/live";
    std::fs::write(
        fixtures
            .path()
            .join("stream-https___radio.example.test_live.headers.json"),
        r#"[["content-type","audio/mpeg"],["icy-name","Local Metal"],["icy-genre","Metal"],["icy-br","128"]]"#,
    )
    .unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_RADIO_FIXTURE_DIR", fixtures.path())]);

    let response = client.call_tool(
        "music_manage_radio",
        json!({ "action": "add", "url": stream_url }),
    );
    let added = structured_ok(&response);
    assert_eq!(added["name"], "Local Metal");
    assert_eq!(added["genre"], "Metal");
    assert_eq!(added["codec"], "MP3");
    assert_eq!(added["bitrate_kbps"], 128);
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}

#[test]
fn resolves_a_pls_radio_favorite_before_probing_and_storing_it() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let fixtures = TempDir::new().unwrap();
    let playlist_url = "https://radio.example.test/metal.pls";
    let stream_url = "https://radio.example.test/live";
    std::fs::write(
        fixtures
            .path()
            .join("stream-https___radio.example.test_metal.pls.body"),
        format!("[playlist]\nNumberOfEntries=1\nFile1={stream_url}\nVersion=2\n"),
    )
    .unwrap();
    std::fs::write(
        fixtures
            .path()
            .join("stream-https___radio.example.test_live.headers.json"),
        r#"[["content-type","audio/mpeg"],["icy-name","Playlist Metal"],["icy-genre","Metal"],["icy-br","192"]]"#,
    )
    .unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_RADIO_FIXTURE_DIR", fixtures.path())]);

    let added = structured_ok(&client.call_tool(
        "music_manage_radio",
        json!({ "action": "add", "url": playlist_url }),
    ));
    assert_eq!(added["name"], "Playlist Metal");
    assert_eq!(added["bitrate_kbps"], 192);

    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let station = reprise_core::radio::station::get(&db, added["id"].as_i64().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(station.stream_url, stream_url);
}

#[test]
fn source_management_rejects_unsafe_urls_and_unknown_ids() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let mut client = McpClient::start(&path);

    let bad_feed = client.call_tool(
        "music_manage_podcasts",
        json!({ "action": "add", "url": "file:///home/marvin/secret.xml" }),
    );
    assert!(tool_error_text(&bad_feed).contains("HTTP RSS feed or YouTube"));
    assert_no_leaks(&serde_json::to_string(&bad_feed).unwrap());

    let bad_radio = client.call_tool(
        "music_manage_radio",
        json!({ "action": "add", "url": "file:///home/marvin/secret", "name": "Bad" }),
    );
    assert!(tool_error_text(&bad_radio).contains("HTTP or HTTPS"));
    assert_no_leaks(&serde_json::to_string(&bad_radio).unwrap());

    let unknown = client.call_tool(
        "music_manage_radio",
        json!({ "action": "remove", "station_id": 999_999 }),
    );
    assert!(tool_error_text(&unknown).contains("does not exist"));
}
