//! JSON-RPC drift protection. Canonical request/response frames are committed
//! under `tests/fixtures/` and replayed against the live server, so a bump of
//! the pinned `rmcp` SDK (or an accidental change to a tool's shape) shows up
//! here as a failing fixture rather than silently.

mod common;

use common::{set_bool_setting, structured_ok, McpClient, SeedTrack};
use serde_json::Value;
use tempfile::TempDir;

const CAP_PLAYLIST_CREATE: &str = "agent.capability.playlist:create";
const CREATE_REQUEST: &str = include_str!("fixtures/create_playlist_request.json");
const CREATE_RESPONSE: &str = include_str!("fixtures/create_playlist_response.json");

#[test]
fn create_playlist_request_matches_committed_fixture() {
    let expected: Value = serde_json::from_str(CREATE_RESPONSE).unwrap();
    let request: Value = serde_json::from_str(CREATE_REQUEST).unwrap();
    let arguments = request
        .get("params")
        .and_then(|params| params.get("arguments"))
        .cloned()
        .expect("fixture arguments");

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(
        &path,
        &[
            SeedTrack::simple("First", "Artist"),
            SeedTrack::simple("Second", "Artist"),
        ],
    );
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);

    let mut client = McpClient::start(&path);
    let response = client.call_tool("music_create_playlist", arguments);
    let structured = structured_ok(&response);

    assert_eq!(
        structured, expected,
        "create_playlist structured output drifted"
    );
}

#[test]
fn tool_schema_is_stable() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[SeedTrack::simple("Song", "Artist")]);
    let mut client = McpClient::start(&path);

    let response = client.request("tools/list", serde_json::json!({}));
    let tools = response
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(Value::as_array)
        .expect("tools array");

    let create = tools
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("music_create_playlist"))
        .expect("music_create_playlist present");

    let mut required: Vec<String> = create
        .get("inputSchema")
        .and_then(|schema| schema.get("required"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    required.sort();

    assert_eq!(
        required,
        vec!["name".to_string(), "track_ids".to_string()],
        "music_create_playlist required arguments drifted"
    );
}
