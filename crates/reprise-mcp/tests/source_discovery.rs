//! `music_search_sources` (Block H-A): capability-gated online discovery for
//! podcasts (RSS), YouTube channels, and radio stations. Mirrors the GNOME
//! add dialogs (`SRC-6`/`SRC-9`) through the real MCP wire boundary.

mod common;

use common::{assert_no_leaks, set_bool_setting, structured_ok, tool_error_text, McpClient};
use serde_json::json;
use tempfile::TempDir;

const CAP_SOURCES_MANAGE: &str = "agent.capability.sources:manage";

fn fixture_db(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    path
}

#[test]
fn lists_the_discovery_tool_with_an_object_input_schema() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let mut client = McpClient::start(&path);
    let response = client.request("tools/list", json!({}));
    let tools = response["result"]["tools"].as_array().unwrap();
    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "music_search_sources")
        .unwrap_or_else(|| panic!("missing music_search_sources: {response}"));
    assert_eq!(tool["inputSchema"]["type"], "object");
}

#[test]
fn discovery_is_gated_like_the_mutations_off_by_default_restart_gated_and_revocable() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let mut denied = McpClient::start(&path);

    let default_denial = denied.call_tool(
        "music_search_sources",
        json!({ "provider": "rss", "query": "metal" }),
    );
    assert!(tool_error_text(&default_denial).contains("sources:manage"));

    // A mid-session grant needs a restart, exactly like the mutation tools.
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let restart_denial = denied.call_tool(
        "music_search_sources",
        json!({ "provider": "rss", "query": "metal" }),
    );
    assert!(tool_error_text(&restart_denial).contains("sources:manage"));
    drop(denied);

    let mut granted = McpClient::start(&path);
    let unknown_provider = granted.call_tool(
        "music_search_sources",
        json!({ "provider": "spotify", "query": "metal" }),
    );
    assert!(tool_error_text(&unknown_provider).contains("unknown provider"));

    // Revocation is live: the next call is refused immediately, no restart.
    set_bool_setting(&path, CAP_SOURCES_MANAGE, false);
    let revoked = granted.call_tool(
        "music_search_sources",
        json!({ "provider": "rss", "query": "metal" }),
    );
    assert!(tool_error_text(&revoked).contains("sources:manage"));
}

#[test]
fn rss_search_hides_an_already_subscribed_feed_and_keeps_a_meaningful_query_string() {
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://feeds.example.test/show".into(),
            title: "Already subscribed".into(),
            author: None,
            image_url: None,
            auto_download: true,
        },
        100,
    )
    .unwrap();
    drop(conn);

    let fixtures = TempDir::new().unwrap();
    std::fs::write(
        fixtures.path().join("itunes-search-metal.json"),
        r#"{"results":[
          {"collectionName":"Already subscribed","feedUrl":"https://feeds.example.test/show","trackCount":9},
          {"collectionName":"New Show","artistName":"New Host","feedUrl":"https://feeds.example.test/other-show?edition=international","trackCount":12}
        ]}"#,
    )
    .unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_PODCASTS_FIXTURE_DIR", fixtures.path())]);

    let response = client.call_tool(
        "music_search_sources",
        json!({ "provider": "rss", "query": "metal" }),
    );
    let result = structured_ok(&response);
    assert_eq!(result["provider"], "rss");
    assert_eq!(result["total"], 1);
    let candidates = result["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["kind"], "rss");
    assert_eq!(candidates[0]["title"], "New Show");
    assert_eq!(candidates[0]["author"], "New Host");
    assert_eq!(candidates[0]["episode_count"], 12);
    // The query string is a meaningful part of the feed's identity, not a
    // signed token, so it survives the leak guard.
    assert_eq!(
        candidates[0]["url"],
        "https://feeds.example.test/other-show?edition=international"
    );
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}

#[test]
fn youtube_search_omits_the_subscriber_count_only_when_the_channel_hides_it() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let fake_ytdlp = dir.path().join("yt-dlp");
    std::fs::write(
        &fake_ytdlp,
        r#"#!/bin/sh
printf '%s\n' '{"entries":[
  {"id":"v1","title":"Vid A","channel_id":"UC-visible","channel":"Visible Channel","channel_follower_count":62400},
  {"id":"v2","title":"Vid B","channel_id":"UC-hidden","channel":"Hidden Channel"}
]}'
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake_ytdlp, std::fs::Permissions::from_mode(0o755)).unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    // YouTube is a peer module, independent of Podcasts (issue #96) — it
    // must be explicitly enabled, same as any other network module.
    set_bool_setting(&path, "module.youtube.enabled", true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_YTDLP_BIN", fake_ytdlp.as_path())]);

    let response = client.call_tool(
        "music_search_sources",
        json!({ "provider": "youtube", "query": "rust audio" }),
    );
    let result = structured_ok(&response);
    let candidates = result["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);

    let visible = candidates
        .iter()
        .find(|candidate| candidate["title"] == "Visible Channel")
        .expect("visible channel candidate");
    assert_eq!(visible["subscriber_count"], 62_400);

    let hidden = candidates
        .iter()
        .find(|candidate| candidate["title"] == "Hidden Channel")
        .expect("hidden channel candidate");
    assert!(
        hidden.get("subscriber_count").is_none(),
        "a hidden subscriber count must be omitted entirely, never rendered as zero: {hidden}"
    );
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}

#[test]
fn radio_search_hides_an_existing_favorite() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    reprise_core::radio::station::add_or_restore(
        &conn,
        &reprise_core::radio::station::NewStation {
            uuid: Some("existing-uuid".into()),
            name: "Existing Station".into(),
            stream_url: "https://radio.example.test/existing".into(),
            homepage: None,
            favicon_url: None,
            genre: None,
            codec: None,
            bitrate_kbps: None,
            country_code: None,
            votes: None,
        },
        100,
    )
    .unwrap();
    drop(conn);

    let fixtures = TempDir::new().unwrap();
    std::fs::write(
        fixtures.path().join("servers.json"),
        r#"[{"name":"fixture.radio-browser.test"}]"#,
    )
    .unwrap();
    std::fs::write(
        fixtures.path().join("search-jazz.json"),
        r#"[
          {"stationuuid":"existing-uuid","name":"Existing Station","url_resolved":"https://radio.example.test/existing","votes":5},
          {"stationuuid":"new-uuid","name":"New Station","url_resolved":"https://radio.example.test/new","votes":9,"countrycode":"us","tags":"jazz"}
        ]"#,
    )
    .unwrap();
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let mut client =
        McpClient::start_with_env(&path, &[("REPRISE_RADIO_FIXTURE_DIR", fixtures.path())]);

    let response = client.call_tool(
        "music_search_sources",
        json!({ "provider": "radio", "query": "jazz" }),
    );
    let result = structured_ok(&response);
    assert_eq!(result["provider"], "radio");
    let candidates = result["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["kind"], "radio");
    assert_eq!(candidates[0]["name"], "New Station");
    assert_eq!(candidates[0]["url"], "https://radio.example.test/new");
    assert_eq!(candidates[0]["country_code"], "US");
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
}

#[test]
fn an_empty_query_is_a_caller_visible_error() {
    let dir = TempDir::new().unwrap();
    let path = fixture_db(&dir);
    set_bool_setting(&path, CAP_SOURCES_MANAGE, true);
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_search_sources",
        json!({ "provider": "rss", "query": "   " }),
    );
    assert!(tool_error_text(&response).contains("query is required"));
}
