//! `music_search_tracks`: tool discovery, pagination, filtering, and the
//! leak-matrix negative on the result shape.

mod common;

use common::{assert_no_leaks, structured_ok, McpClient, SeedTrack};
use serde_json::{json, Value};
use tempfile::TempDir;

fn five_track_db(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("reprise.db");
    common::seed_tracks(
        &path,
        &[
            SeedTrack::simple("Aaa", "Artist"),
            SeedTrack::simple("Bbb", "Artist"),
            SeedTrack::simple("Ccc", "Artist"),
            SeedTrack::simple("Ddd", "Artist"),
            SeedTrack::simple("Eee", "Artist"),
        ],
    );
    path
}

fn track_titles(structured: &Value) -> Vec<String> {
    structured
        .get("tracks")
        .and_then(Value::as_array)
        .expect("tracks array")
        .iter()
        .filter_map(|track| track.get("title").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

#[test]
fn tool_discovery_lists_the_expected_tools() {
    let dir = TempDir::new().unwrap();
    let path = five_track_db(&dir);
    let mut client = McpClient::start(&path);

    let response = client.request("tools/list", json!({}));
    let tools = response
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(Value::as_array)
        .expect("tools array");
    let mut names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect();
    names.sort_unstable();

    // `music_create_instrumental` is listed even though `ai:create` is off by
    // default: tools stay listed-but-refused (Beschluss 7). No rename/delete
    // surface exists in the MCP (Beschluss 2). The playback tools only exist
    // under the `mpris` feature.
    let mut expected = vec![
        "music_create_instrumental",
        "music_create_playlist",
        "music_get_playlist",
        "music_get_job_status",
        "music_search_albums",
        "music_search_artists",
        "music_search_tracks",
        "music_update_playlist",
    ];
    if cfg!(feature = "mpris") {
        expected.extend(["music_play", "music_playback_control"]);
    }
    expected.sort_unstable();
    assert_eq!(names, expected);
    for tool in tools {
        let schema = tool.get("inputSchema");
        assert!(
            schema.is_some(),
            "each tool must publish an inputSchema: {tool}"
        );
    }
}

#[test]
fn search_returns_all_present_tracks() {
    let dir = TempDir::new().unwrap();
    let path = five_track_db(&dir);
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_search_tracks", json!({ "query": "" }));
    let structured = structured_ok(&response);

    assert_eq!(structured.get("total").and_then(Value::as_i64), Some(5));
    assert_eq!(structured.get("returned").and_then(Value::as_u64), Some(5));
    assert_eq!(
        structured.get("has_more").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(track_titles(&structured).len(), 5);
}

#[test]
fn search_paginates_with_limit_and_offset() {
    let dir = TempDir::new().unwrap();
    let path = five_track_db(&dir);
    let mut client = McpClient::start(&path);

    let first = structured_ok(&client.call_tool(
        "music_search_tracks",
        json!({ "query": "", "limit": 2, "offset": 0 }),
    ));
    assert_eq!(first.get("total").and_then(Value::as_i64), Some(5));
    assert_eq!(track_titles(&first), ["Aaa", "Bbb"]);
    assert_eq!(first.get("has_more").and_then(Value::as_bool), Some(true));

    let second = structured_ok(&client.call_tool(
        "music_search_tracks",
        json!({ "query": "", "limit": 2, "offset": 2 }),
    ));
    assert_eq!(track_titles(&second), ["Ccc", "Ddd"]);
    assert_eq!(second.get("has_more").and_then(Value::as_bool), Some(true));

    let third = structured_ok(&client.call_tool(
        "music_search_tracks",
        json!({ "query": "", "limit": 2, "offset": 4 }),
    ));
    assert_eq!(track_titles(&third), ["Eee"]);
    assert_eq!(third.get("has_more").and_then(Value::as_bool), Some(false));
}

#[test]
fn search_filters_by_query() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    common::seed_tracks(
        &path,
        &[
            SeedTrack::simple("Blue in Green", "Miles Davis"),
            SeedTrack::simple("So What", "Miles Davis"),
            SeedTrack::simple("Giant Steps", "John Coltrane"),
        ],
    );
    let mut client = McpClient::start(&path);

    let structured =
        structured_ok(&client.call_tool("music_search_tracks", json!({ "query": "coltrane" })));
    assert_eq!(structured.get("total").and_then(Value::as_i64), Some(1));
    assert_eq!(track_titles(&structured), ["Giant Steps"]);
}

#[test]
fn search_result_never_leaks_paths() {
    let dir = TempDir::new().unwrap();
    let path = five_track_db(&dir);
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_search_tracks", json!({ "query": "" }));
    // Sanity: the fixtures use /music/... paths, so a leak would be obvious.
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
    // Positive: the allowed metadata is present.
    let structured = structured_ok(&response);
    let first = &structured.get("tracks").and_then(Value::as_array).unwrap()[0];
    for field in [
        "id",
        "title",
        "artist",
        "album",
        "genre",
        "rating",
        "duration_ms",
    ] {
        assert!(first.get(field).is_some(), "missing allowed field {field}");
    }
    assert!(first.get("path").is_none(), "path must never appear");
}
