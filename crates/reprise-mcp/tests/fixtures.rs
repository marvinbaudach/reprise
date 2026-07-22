//! JSON-RPC drift protection. Canonical request/response frames are committed
//! under `tests/fixtures/` and replayed against the live server, so a bump of
//! the pinned `rmcp` SDK (or an accidental change to a tool's shape) shows up
//! here as a failing fixture rather than silently.

mod common;

use common::{set_bool_setting, structured_ok, McpClient, SeedTrack, CAP_AI_CREATE};
use serde_json::Value;
use tempfile::TempDir;

const CAP_PLAYLIST_CREATE: &str = "agent.capability.playlist:create";
const CREATE_REQUEST: &str = include_str!("fixtures/create_playlist_request.json");
const CREATE_RESPONSE: &str = include_str!("fixtures/create_playlist_response.json");
const CREATE_INSTRUMENTAL_REQUEST: &str = include_str!("fixtures/create_instrumental_request.json");
const CREATE_INSTRUMENTAL_RESPONSE: &str =
    include_str!("fixtures/create_instrumental_response.json");
const JOB_STATUS_REQUEST: &str = include_str!("fixtures/job_status_request.json");
const JOB_STATUS_RESPONSE: &str = include_str!("fixtures/job_status_response.json");

/// The random per-invocation `batch_id` is normalized to a placeholder so the
/// rest of the response can be diffed against a committed fixture.
fn normalize_batch(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        if object.contains_key("batch_id") {
            object.insert("batch_id".to_string(), Value::String("<batch>".to_string()));
        }
    }
    value
}

fn tool_arguments(request: &str) -> Value {
    let request: Value = serde_json::from_str(request).unwrap();
    request
        .get("params")
        .and_then(|params| params.get("arguments"))
        .cloned()
        .expect("fixture arguments")
}

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

#[test]
fn create_instrumental_request_matches_committed_fixture() {
    let expected: Value = serde_json::from_str(CREATE_INSTRUMENTAL_RESPONSE).unwrap();
    let arguments = tool_arguments(CREATE_INSTRUMENTAL_REQUEST);

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(
        &path,
        &[
            SeedTrack::simple("First", "Artist"),
            SeedTrack::simple("Second", "Artist"),
        ],
    );
    set_bool_setting(&path, CAP_AI_CREATE, true);

    let mut client = McpClient::start(&path);
    let response = client.call_tool("music_create_instrumental", arguments);
    let structured = structured_ok(&response);

    assert_eq!(
        normalize_batch(structured),
        expected,
        "create_instrumental structured output drifted"
    );
}

#[test]
fn job_status_request_matches_committed_fixture() {
    let expected: Value = serde_json::from_str(JOB_STATUS_RESPONSE).unwrap();
    let arguments = tool_arguments(JOB_STATUS_REQUEST);

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[SeedTrack::simple("Song", "Artist")]);
    // Enqueue one queued job (id 1) with a fixed clock so `created_at` is
    // deterministic and the whole response can be diffed.
    {
        let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
        let staging = reprise_core::ai_staging::StagingStore::new(dir.path().join("staging"));
        reprise_core::ai_jobs::enqueue_instrumental(&conn, &staging, 1, "test@1", 1000).unwrap();
    }

    let mut client = McpClient::start(&path);
    let response = client.call_tool("music_get_job_status", arguments);
    let structured = structured_ok(&response);

    assert_eq!(structured, expected, "job_status structured output drifted");
}

#[test]
fn ai_tool_schemas_are_stable() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[SeedTrack::simple("Song", "Artist")]);
    let mut client = McpClient::start(&path);

    let response = client.request("tools/list", serde_json::json!({}));
    let tools = response
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(Value::as_array)
        .expect("tools array")
        .clone();

    let required = |name: &str| -> Vec<String> {
        let tool = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
            .unwrap_or_else(|| panic!("{name} present"));
        let mut required: Vec<String> = tool
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
        required
    };

    // `save` (create) and both status args are defaulted, so only `track_ids`
    // is required for create; job status has no required argument.
    assert_eq!(
        required("music_create_instrumental"),
        vec!["track_ids".to_string()],
        "music_create_instrumental required arguments drifted"
    );
    assert_eq!(
        required("music_get_job_status"),
        Vec::<String>::new(),
        "music_get_job_status required arguments drifted"
    );
}
