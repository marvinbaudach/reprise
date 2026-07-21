//! D19 leak matrix: every response shape is asserted to carry only allow-list
//! metadata — never a path, XDG/cache/db path, lyric, serial or credential.
//! The fixtures deliberately embed obvious markers (`/home/`, `.flac`,
//! `secret`) so any leak is caught.

mod common;

use common::{assert_no_leaks, set_bool_setting, McpClient, SeedTrack};
use serde_json::{json, Value};
use tempfile::TempDir;

const CAP_PLAYLIST_CREATE: &str = "agent.capability.playlist:create";

fn revealing_db(dir: &TempDir) -> (std::path::PathBuf, Vec<i64>) {
    let path = dir.path().join("reprise.db");
    let ids = common::seed_tracks(
        &path,
        &[
            SeedTrack {
                path: "/home/marvin/Music/secret-folder/track-one.flac".to_string(),
                title: "Track One".to_string(),
                artist: "An Artist".to_string(),
                album: "An Album".to_string(),
                genre: "Jazz".to_string(),
                year: Some(1999),
                duration_ms: 210_000,
                rating: 4,
            },
            SeedTrack {
                path: "/home/marvin/Music/secret-folder/track-two.flac".to_string(),
                title: "Track Two".to_string(),
                artist: "An Artist".to_string(),
                album: "An Album".to_string(),
                genre: "Jazz".to_string(),
                year: Some(2001),
                duration_ms: 240_000,
                rating: 5,
            },
        ],
    );
    (path, ids)
}

fn raw(response: &Value) -> String {
    serde_json::to_string(response).unwrap()
}

#[test]
fn search_response_has_no_leaks() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = revealing_db(&dir);
    let mut client = McpClient::start(&path);
    let response = client.call_tool("music_search_tracks", json!({ "query": "" }));
    assert_no_leaks(&raw(&response));
}

#[test]
fn summary_response_has_no_leaks() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = revealing_db(&dir);
    let mut client = McpClient::start(&path);
    let response = client.read_resource("reprise://library/summary");
    assert_no_leaks(&raw(&response));
}

#[test]
fn playlists_response_has_no_leaks() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = revealing_db(&dir);
    let mut conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    reprise_core::library::playlists::create_with_tracks(&mut conn, "Secret Mix", &ids).unwrap();
    drop(conn);

    let mut client = McpClient::start(&path);
    let response = client.read_resource("reprise://playlists");
    assert_no_leaks(&raw(&response));
}

#[test]
fn create_playlist_response_has_no_leaks() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = revealing_db(&dir);
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);
    let mut client = McpClient::start(&path);
    let response = client.call_tool(
        "music_create_playlist",
        json!({ "name": "Fresh Mix", "track_ids": ids }),
    );
    assert_no_leaks(&raw(&response));
}
