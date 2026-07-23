//! `music_create_playlist`: the write path and its capability gate — happy
//! path (+ change_log verification), fail-closed refusal, immediate
//! revocation, and the input limits.

mod common;

use common::{
    assert_no_leaks, set_bool_setting, structured_ok, tool_error_text, McpClient, SeedTrack,
};
use serde_json::{json, Value};
use tempfile::TempDir;

const CAP_PLAYLIST_CREATE: &str = "agent.capability.playlist:create";

fn db_with_tracks(dir: &TempDir, count: usize) -> (std::path::PathBuf, Vec<i64>) {
    let path = dir.path().join("reprise.db");
    let tracks: Vec<SeedTrack> = (0..count)
        .map(|i| SeedTrack::simple(&format!("Track{i}"), "Artist"))
        .collect();
    let ids = common::seed_tracks(&path, &tracks);
    (path, ids)
}

/// Reads the change_log through the core facade to prove the write recorded a
/// cross-process event in the same transaction.
fn playlist_create_events(path: &std::path::Path) -> usize {
    let conn = reprise_core::db::open_migrated(Some(path)).unwrap();
    reprise_core::events::read_since(&conn, 0, None)
        .unwrap()
        .into_iter()
        .filter(|change| change.entity == "playlist" && change.operation == "create")
        .count()
}

#[test]
fn create_playlist_happy_path_records_change_log() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = db_with_tracks(&dir, 3);
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_create_playlist",
        json!({ "name": "Roadtrip", "track_ids": ids }),
    );
    let structured = structured_ok(&response);

    assert_eq!(
        structured.get("name").and_then(Value::as_str),
        Some("Roadtrip")
    );
    assert_eq!(
        structured.get("track_count").and_then(Value::as_u64),
        Some(3)
    );
    assert!(structured
        .get("playlist_id")
        .and_then(Value::as_i64)
        .is_some());
    assert_no_leaks(&serde_json::to_string(&response).unwrap());

    // The playlist and its change_log row are committed atomically by the
    // core facade — a running app would see this live.
    assert_eq!(
        playlist_create_events(&path),
        1,
        "one create event expected"
    );
}

#[test]
fn create_playlist_refused_when_capability_off_by_default() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = db_with_tracks(&dir, 2);
    // No capability set — fail-closed off.
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_create_playlist",
        json!({ "name": "Nope", "track_ids": ids }),
    );

    let text = tool_error_text(&response);
    assert!(
        text.contains("playlist:create"),
        "refusal should name the capability: {text}"
    );
    // The refused write left no playlist and no event behind.
    assert_eq!(playlist_create_events(&path), 0);
}

#[test]
fn revocation_takes_effect_immediately_mid_session() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = db_with_tracks(&dir, 2);
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);
    let mut client = McpClient::start(&path);

    // Granted at startup: first write succeeds.
    let granted = client.call_tool(
        "music_create_playlist",
        json!({ "name": "First", "track_ids": ids.clone() }),
    );
    structured_ok(&granted);

    // Revoke while the server keeps running — the next write must refuse
    // because the capability is re-read on every write call.
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, false);
    let revoked = client.call_tool(
        "music_create_playlist",
        json!({ "name": "Second", "track_ids": ids }),
    );
    let text = tool_error_text(&revoked);
    assert!(
        text.contains("playlist:create"),
        "revocation should refuse: {text}"
    );

    assert_eq!(
        playlist_create_events(&path),
        1,
        "only the first write committed"
    );
}

#[test]
fn create_playlist_rejects_empty_name() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = db_with_tracks(&dir, 1);
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_create_playlist",
        json!({ "name": "   ", "track_ids": ids }),
    );
    let text = tool_error_text(&response);
    assert!(text.contains("empty"), "should reject empty name: {text}");
    assert_eq!(playlist_create_events(&path), 0);
}

#[test]
fn create_playlist_rejects_more_than_500_ids() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = db_with_tracks(&dir, 1);
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);
    let mut client = McpClient::start(&path);

    let too_many: Vec<i64> = (1..=501).collect();
    let response = client.call_tool(
        "music_create_playlist",
        json!({ "name": "Huge", "track_ids": too_many }),
    );
    let text = tool_error_text(&response);
    assert!(text.contains("too many"), "should reject > 500 ids: {text}");
    assert_eq!(playlist_create_events(&path), 0);
}

#[test]
fn create_playlist_rejects_unknown_track_id() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = db_with_tracks(&dir, 1);
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_create_playlist",
        json!({ "name": "Ghosts", "track_ids": [999_999] }),
    );
    let text = tool_error_text(&response);
    assert!(
        text.contains("not present"),
        "should reject ids that are not present: {text}"
    );
    assert!(
        text.contains("999999"),
        "should name the offending id: {text}"
    );
    assert_eq!(playlist_create_events(&path), 0);
}

#[test]
fn create_playlist_rejects_a_present_but_missing_track_id() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = db_with_tracks(&dir, 2);
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);
    // Mark the first track missing (file gone) — the row still exists, so a
    // plain foreign-key check would accept it; PRESENT semantics must reject it.
    {
        let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
        conn.execute(
            "UPDATE tracks SET missing_since = 1 WHERE id = ?1",
            [ids[0]],
        )
        .unwrap();
    }
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_create_playlist",
        json!({ "name": "Half", "track_ids": ids.clone() }),
    );
    let text = tool_error_text(&response);
    assert!(
        text.contains("not present"),
        "should reject a present-but-missing id: {text}"
    );
    assert!(
        text.contains(&ids[0].to_string()),
        "should name the offending id: {text}"
    );
    assert_eq!(playlist_create_events(&path), 0);
}

#[test]
fn create_playlist_allows_empty_track_list() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = db_with_tracks(&dir, 1);
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_create_playlist",
        json!({ "name": "Empty", "track_ids": [] }),
    );
    let structured = structured_ok(&response);
    assert_eq!(
        structured.get("track_count").and_then(Value::as_u64),
        Some(0)
    );
}
