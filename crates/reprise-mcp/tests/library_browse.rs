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

/// A library whose names exercise the Rust-side search filter: non-ASCII
/// mixed case, an album that only matches through its album artist, and five
/// "Band" artists for pagination.
fn search_db(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("reprise.db");
    let seeded = |title: &str, artist: &str, album: &str| SeedTrack {
        album: album.to_owned(),
        ..SeedTrack::simple(title, artist)
    };
    common::seed_tracks(
        &path,
        &[
            seeded("Schrei nach Liebe", "Die Ärzte", "Jazz ist anders"),
            seeded("Joga", "BJÖRK", "Homogenic"),
            seeded("One", "Band A", "Alpha"),
            seeded("Two", "Band B", "Beta"),
            seeded("Three", "Band C", "Gamma"),
            seeded("Four", "Band D", "Delta"),
            seeded("Guest Spot", "Guest Singer", "Mixtape"),
        ],
    );
    // The compilation track's own artist never matches "collective"; only its
    // album artist does.
    common::fixture_connection(&path)
        .execute(
            "UPDATE tracks SET album_artist = 'Band Collective' WHERE album = 'Mixtape'",
            [],
        )
        .expect("set album artist");
    path
}

fn names(result: &Value, list: &str, field: &str) -> Vec<String> {
    result[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row[field].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn search_folds_non_ascii_case_for_artists_and_albums() {
    let dir = TempDir::new().unwrap();
    let path = search_db(&dir);
    let mut client = McpClient::start(&path);

    for (needle, artist, album) in [
        ("ÄRZTE", "Die Ärzte", "Jazz ist anders"),
        ("björk", "BJÖRK", "Homogenic"),
    ] {
        let artists = structured_ok(&client.call_tool(
            "music_search_artists",
            json!({ "query": needle, "limit": 10, "offset": 0 }),
        ));
        assert_eq!(artists["total"], 1, "artists for {needle}");
        assert_eq!(names(&artists, "artists", "artist"), [artist]);

        let albums = structured_ok(&client.call_tool(
            "music_search_albums",
            json!({ "query": needle, "limit": 10, "offset": 0 }),
        ));
        assert_eq!(albums["total"], 1, "albums for {needle}");
        assert_eq!(names(&albums, "albums", "album"), [album]);
        assert_eq!(names(&albums, "albums", "album_artist"), [artist]);
    }
}

#[test]
fn album_search_matches_through_the_effective_album_artist() {
    let dir = TempDir::new().unwrap();
    let path = search_db(&dir);
    let mut client = McpClient::start(&path);

    let albums = structured_ok(&client.call_tool(
        "music_search_albums",
        json!({ "query": "collective", "limit": 10, "offset": 0 }),
    ));
    assert_eq!(albums["total"], 1);
    assert_eq!(names(&albums, "albums", "album"), ["Mixtape"]);
    assert_eq!(
        names(&albums, "albums", "album_artist"),
        ["Band Collective"]
    );

    // The track's own artist is not what the album search compares.
    let by_track_artist = structured_ok(&client.call_tool(
        "music_search_albums",
        json!({ "query": "guest singer", "limit": 10, "offset": 0 }),
    ));
    assert_eq!(by_track_artist["total"], 0);
}

#[test]
fn search_totals_and_pages_cross_the_page_boundary() {
    let dir = TempDir::new().unwrap();
    let path = search_db(&dir);
    let mut client = McpClient::start(&path);

    let artists = structured_ok(&client.call_tool(
        "music_search_artists",
        json!({ "query": "BAND", "limit": 2, "offset": 2 }),
    ));
    assert_eq!(artists["total"], 5);
    assert_eq!(artists["returned"], 2);
    assert_eq!(artists["has_more"], true);
    assert_eq!(
        names(&artists, "artists", "artist"),
        ["Band C", "Band Collective"]
    );
    let last_artists = structured_ok(&client.call_tool(
        "music_search_artists",
        json!({ "query": "BAND", "limit": 2, "offset": 4 }),
    ));
    assert_eq!(last_artists["returned"], 1);
    assert_eq!(last_artists["has_more"], false);
    assert_eq!(names(&last_artists, "artists", "artist"), ["Band D"]);

    let albums = structured_ok(&client.call_tool(
        "music_search_albums",
        json!({ "query": "BAND", "limit": 3, "offset": 1 }),
    ));
    assert_eq!(albums["total"], 5);
    assert_eq!(albums["returned"], 3);
    assert_eq!(albums["has_more"], true);
    assert_eq!(
        names(&albums, "albums", "album"),
        ["Beta", "Delta", "Gamma"]
    );
    let last_albums = structured_ok(&client.call_tool(
        "music_search_albums",
        json!({ "query": "BAND", "limit": 3, "offset": 4 }),
    ));
    assert_eq!(last_albums["returned"], 1);
    assert_eq!(last_albums["has_more"], false);
    assert_eq!(names(&last_albums, "albums", "album"), ["Mixtape"]);
}
