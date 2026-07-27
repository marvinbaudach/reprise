//! Device-sync MCP surface: discovery and fail-closed mutation.
#![cfg(feature = "mpris")]

mod common;

use common::{tool_error_text, McpClient};
use serde_json::{json, Value};
use tempfile::TempDir;

fn empty_db() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[]);
    (dir, path)
}

#[test]
fn tools_advertise_read_and_mutation_surfaces() {
    let (_dir, path) = empty_db();
    let mut client = McpClient::start(&path);
    let response = client.request("tools/list", json!({}));
    let names = response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert!(names.contains(&"music_get_device_sync_state"));
    assert!(names.contains(&"music_device_sync"));
}

#[test]
fn mutation_schema_exposes_multi_source_transfer_profile_configuration_only() {
    let (_dir, path) = empty_db();
    let mut client = McpClient::start(&path);
    let response = client.request("tools/list", json!({}));
    let tool = response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "music_device_sync")
        .unwrap();
    let properties = tool["inputSchema"]["properties"].as_object().unwrap();

    assert!(properties.contains_key("sources"));
    assert!(properties.contains_key("profile"));
    assert!(!properties.contains_key("quality_kbps"));
    assert!(!properties.contains_key("playlist_name"));
    assert!(!properties.contains_key("remove_unselected"));
    assert!(!properties.contains_key("bitrate_kbps"));
}

#[test]
fn mutation_is_fail_closed_before_any_bus_call() {
    let (_dir, path) = empty_db();
    let mut client = McpClient::start(&path);
    let response = client.call_tool(
        "music_device_sync",
        json!({
            "action": "start",
            "device_name": "Pixel"
        }),
    );
    let text = tool_error_text(&response);
    assert!(text.contains("device:sync"), "{text}");
}

#[test]
fn readme_documents_the_complete_playlist_mirroring_contract() {
    let readme = include_str!("../README.md");

    assert!(readme.contains("`action` = `configure`\\|`start`\\|`cancel`\\|`eject`"));
    assert!(readme.contains("`sources`"));
    assert!(readme.contains("`profile`"));
    assert!(readme.contains("`opus_160`\\|`mp3_256`\\|`original`"));
    assert!(readme.contains("deduplicated"));
    assert!(readme.contains("`last_synced_at`"));
    assert!(readme.contains("`access`"));
    assert!(!readme.contains("configure_playlist"));
    assert!(!readme.contains("bitrate_kbps"));
    assert!(!readme.contains("tracks_to_copy"));
    assert!(!readme.contains("bytes_to_copy"));
}
