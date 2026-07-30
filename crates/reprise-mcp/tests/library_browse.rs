//! Agent-facing artist/album discovery and playlist-content reads.

mod common;

use common::{assert_no_leaks, structured_ok, McpClient, SeedTrack};
use serde_json::{json, Value};
use tempfile::TempDir;

fn browse_db(dir: &TempDir) -> (std::path::PathBuf, Vec<i64>, i64) {
    let path = dir.path().join("reprise.db");
    let ids = common::seed_tracks(
        &path,
        &[
            SeedTrack {
                album: "Pain Remains".to_owned(),
                ..SeedTrack::simple("Welcome Back, O' Sleeping Dreamer", "Lorna Shore")
            },
            SeedTrack {
                album: "Pain Remains".to_owned(),
                ..SeedTrack::simple("Sun//Eater", "Lorna Shore")
            },
            SeedTrack {
                album: "Melancholy".to_owned(),
                ..SeedTrack::simple("Gravesinger", "Shadow of Intent")
            },
        ],
    );
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let playlist_id = reprise_core::library::playlists::create_with_tracks(
        &db,
        "Deathcore",
        &[ids[2], ids[0], ids[2]],
    )
    .unwrap();
    (path, ids, playlist_id)
}

#[test]
fn artist_search_filters_and_returns_path_free_summaries() {
    let dir = TempDir::new().unwrap();
    let (path, _, _) = browse_db(&dir);
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_search_artists",
        json!({ "query": "shore", "limit": 10, "offset": 0 }),
    );
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
    let result = structured_ok(&response);
    assert_eq!(result["total"], 1);
    assert_eq!(result["artists"][0]["artist"], "Lorna Shore");
    assert_eq!(result["artists"][0]["track_count"], 2);
    assert_eq!(result["artists"][0]["album_count"], 1);
    assert!(result["artists"][0].get("representative_path").is_none());
}

#[test]
fn album_search_filters_and_paginates_path_free_summaries() {
    let dir = TempDir::new().unwrap();
    let (path, _, _) = browse_db(&dir);
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_search_albums",
        json!({ "query": "pain", "limit": 1, "offset": 0 }),
    );
    assert_no_leaks(&serde_json::to_string(&response).unwrap());
    let result = structured_ok(&response);
    assert_eq!(result["total"], 1);
    assert_eq!(result["returned"], 1);
    assert_eq!(result["has_more"], false);
    assert_eq!(result["albums"][0]["album"], "Pain Remains");
    assert_eq!(result["albums"][0]["album_artist"], "Lorna Shore");
    assert_eq!(result["albums"][0]["track_count"], 2);
    assert!(result["albums"][0].get("representative_path").is_none());
}

#[test]
fn playlist_content_read_preserves_order_duplicates_and_pages() {
    let dir = TempDir::new().unwrap();
    let (path, ids, playlist_id) = browse_db(&dir);
    let mut client = McpClient::start(&path);

    let first = structured_ok(&client.call_tool(
        "music_get_playlist",
        json!({ "playlist_id": playlist_id, "limit": 2, "offset": 0 }),
    ));
    assert_eq!(first["playlist"]["name"], "Deathcore");
    assert_eq!(first["total"], 3);
    assert_eq!(first["returned"], 2);
    assert_eq!(first["has_more"], true);
    let first_ids: Vec<i64> = first["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|track| track["id"].as_i64())
        .collect();
    assert_eq!(first_ids, [ids[2], ids[0]]);

    let second_response = client.call_tool(
        "music_get_playlist",
        json!({ "playlist_id": playlist_id, "limit": 2, "offset": 2 }),
    );
    assert_no_leaks(&serde_json::to_string(&second_response).unwrap());
    let second = structured_ok(&second_response);
    assert_eq!(second["tracks"][0]["id"], ids[2]);
    assert_eq!(second["has_more"], false);
}

#[test]
fn playlist_content_read_rejects_an_unknown_playlist() {
    let dir = TempDir::new().unwrap();
    let (path, _, _) = browse_db(&dir);
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_get_playlist", json!({ "playlist_id": 999_999 }));
    let result = response["result"].as_object().expect("tool result");
    assert_eq!(result["isError"], Value::Bool(true));
    assert!(result["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("playlist does not exist"));
}
