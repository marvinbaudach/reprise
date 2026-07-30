//! Safe manual-playlist writes: rename and append only.

mod common;

use common::{set_bool_setting, structured_ok, tool_error_text, McpClient, SeedTrack};
use serde_json::json;
use tempfile::TempDir;

const CAP_PLAYLIST_MANAGE: &str = "agent.capability.playlist:manage";

fn fixture(dir: &TempDir) -> (std::path::PathBuf, Vec<i64>, i64) {
    let path = dir.path().join("reprise.db");
    let ids = common::seed_tracks(
        &path,
        &[
            SeedTrack::simple("One", "Artist"),
            SeedTrack::simple("Two", "Artist"),
            SeedTrack::simple("Three", "Artist"),
        ],
    );
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let playlist_id =
        reprise_core::library::playlists::create_with_tracks(&db, "Before", &[ids[0]]).unwrap();
    (path, ids, playlist_id)
}

#[test]
fn rename_and_add_tracks_are_capability_gated_and_recorded() {
    let dir = TempDir::new().unwrap();
    let (path, ids, playlist_id) = fixture(&dir);
    set_bool_setting(&path, CAP_PLAYLIST_MANAGE, true);
    let mut client = McpClient::start(&path);

    let renamed = structured_ok(&client.call_tool(
        "music_update_playlist",
        json!({ "action": "rename", "playlist_id": playlist_id, "name": "After" }),
    ));
    assert_eq!(renamed["name"], "After");
    assert_eq!(renamed["track_count"], 1);
    assert_eq!(renamed["affected"], 1);

    let appended = structured_ok(&client.call_tool(
        "music_update_playlist",
        json!({
            "action": "add_tracks",
            "playlist_id": playlist_id,
            "track_ids": [ids[1], ids[2], ids[1]]
        }),
    ));
    assert_eq!(appended["name"], "After");
    assert_eq!(appended["track_count"], 4);
    assert_eq!(appended["affected"], 3);

    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    assert_eq!(
        reprise_core::library::playlists::track_ids(&db, playlist_id).unwrap(),
        [ids[0], ids[1], ids[2], ids[1]]
    );
    let operations: Vec<String> = reprise_core::events::read_since(&db, 0, None)
        .unwrap()
        .into_iter()
        .filter(|change| change.entity == "playlist")
        .map(|change| change.operation)
        .collect();
    assert!(operations.ends_with(&["rename".to_owned(), "add".to_owned()]));
}

#[test]
fn playlist_manage_is_off_by_default_and_revocation_is_immediate() {
    let dir = TempDir::new().unwrap();
    let (path, ids, playlist_id) = fixture(&dir);
    let mut denied_client = McpClient::start(&path);
    let denied = denied_client.call_tool(
        "music_update_playlist",
        json!({ "action": "rename", "playlist_id": playlist_id, "name": "Denied" }),
    );
    assert!(tool_error_text(&denied).contains("playlist:manage"));
    drop(denied_client);

    set_bool_setting(&path, CAP_PLAYLIST_MANAGE, true);
    let mut granted_client = McpClient::start(&path);
    structured_ok(&granted_client.call_tool(
        "music_update_playlist",
        json!({ "action": "add_tracks", "playlist_id": playlist_id, "track_ids": [ids[1]] }),
    ));
    set_bool_setting(&path, CAP_PLAYLIST_MANAGE, false);
    let revoked = granted_client.call_tool(
        "music_update_playlist",
        json!({ "action": "rename", "playlist_id": playlist_id, "name": "Revoked" }),
    );
    assert!(tool_error_text(&revoked).contains("playlist:manage"));
}

#[test]
fn update_rejects_unknown_playlists_and_absent_tracks() {
    let dir = TempDir::new().unwrap();
    let (path, _, playlist_id) = fixture(&dir);
    set_bool_setting(&path, CAP_PLAYLIST_MANAGE, true);
    let mut client = McpClient::start(&path);

    let unknown = client.call_tool(
        "music_update_playlist",
        json!({ "action": "rename", "playlist_id": 999_999, "name": "Ghost" }),
    );
    assert!(tool_error_text(&unknown).contains("playlist does not exist"));

    let absent = client.call_tool(
        "music_update_playlist",
        json!({
            "action": "add_tracks",
            "playlist_id": playlist_id,
            "track_ids": [999_999]
        }),
    );
    assert!(tool_error_text(&absent).contains("not present"));
}

#[test]
fn update_validates_nonempty_names_and_track_lists() {
    let dir = TempDir::new().unwrap();
    let (path, _, playlist_id) = fixture(&dir);
    set_bool_setting(&path, CAP_PLAYLIST_MANAGE, true);
    let mut client = McpClient::start(&path);

    let blank = client.call_tool(
        "music_update_playlist",
        json!({ "action": "rename", "playlist_id": playlist_id, "name": "  " }),
    );
    assert!(tool_error_text(&blank).contains("must not be empty"));

    let empty = client.call_tool(
        "music_update_playlist",
        json!({ "action": "add_tracks", "playlist_id": playlist_id, "track_ids": [] }),
    );
    assert!(tool_error_text(&empty).contains("at least one track id"));
}
