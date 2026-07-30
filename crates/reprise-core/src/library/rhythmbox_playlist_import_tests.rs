use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use super::*;

#[test]
fn playlist_parser_keeps_static_order_and_decodes_locations() {
    let dir = tempdir().unwrap();
    let first = dir.path().join("One & Only.ogg");
    let second = dir.path().join("Two.ogg");
    let first_url = url::Url::from_file_path(&first).unwrap();
    let first_uri = quick_xml::escape::escape(first_url.as_str());
    let second_uri = url::Url::from_file_path(&second).unwrap();
    let xml = format!(
        r#"<rhythmdb-playlists>
<playlist name="Favorites &amp; More" type="static">
  <location>{first_uri}</location><location>{second_uri}</location>
</playlist>
<playlist name="Automatic" type="automatic"><location>{second_uri}</location></playlist>
</rhythmdb-playlists>"#
    );
    let path = dir.path().join("playlists.xml");
    fs::write(&path, xml).unwrap();

    assert_eq!(
        parse_playlists(&path).unwrap(),
        vec![RhythmboxPlaylist {
            name: "Favorites & More".to_string(),
            paths: vec![first, second],
        }]
    );
}

#[test]
fn playlist_parser_rejects_truncated_xml() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("playlists.xml");
    fs::write(
        &path,
        r#"<rhythmdb-playlists><playlist name="Broken" type="static">"#,
    )
    .unwrap();

    assert!(matches!(
        parse_playlists(&path),
        Err(RhythmboxImportError::UnexpectedEof)
    ));
}

fn playlist_database() -> crate::db::Db {
    let conn = crate::db::Db::open_in_memory().unwrap();
    for id in 1..=2 {
        conn.conn()
            .execute(
                "INSERT INTO tracks (id, path, added_at) VALUES (?1, ?2, 0)",
                rusqlite::params![id, format!("/music/{id}.ogg")],
            )
            .unwrap();
    }
    conn
}

fn playlist_track_ids(conn: &crate::db::Db, name: &str) -> Vec<i64> {
    conn.conn()
        .prepare(
            "SELECT pt.track_id FROM playlist_tracks pt JOIN playlists p ON p.id=pt.playlist_id \
         WHERE p.name=?1 ORDER BY pt.position",
        )
        .unwrap()
        .query_map([name], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn playlist_merge_creates_only_nonempty_matched_playlists() {
    let conn = playlist_database();
    let summary = merge_playlists(
        &conn,
        &[
            RhythmboxPlaylist {
                name: "Road Trip".to_string(),
                paths: vec![
                    PathBuf::from("/music/2.ogg"),
                    PathBuf::from("/music/missing.ogg"),
                    PathBuf::from("/music/1.ogg"),
                ],
            },
            RhythmboxPlaylist {
                name: "Unavailable".to_string(),
                paths: vec![PathBuf::from("/music/missing.ogg")],
            },
        ],
    )
    .unwrap();

    assert_eq!(playlist_track_ids(&conn, "Road Trip"), vec![2, 1]);
    assert!(playlist_track_ids(&conn, "Unavailable").is_empty());
    assert_eq!(summary.parsed, 2);
    assert_eq!(summary.imported, 1);
    assert_eq!(summary.tracks_added, 2);
    assert_eq!(summary.skipped_tracks, 2);
}

#[test]
fn playlist_merge_extends_same_name_without_duplicates_and_is_idempotent() {
    let conn = playlist_database();
    let playlist_id = crate::library::playlists::create(&conn, "Favorites").unwrap();
    crate::library::playlists::add_tracks(&conn, playlist_id, &[1]).unwrap();
    let imported = [RhythmboxPlaylist {
        name: "Favorites".to_string(),
        paths: vec![
            PathBuf::from("/music/1.ogg"),
            PathBuf::from("/music/2.ogg"),
            PathBuf::from("/music/2.ogg"),
        ],
    }];

    let first = merge_playlists(&conn, &imported).unwrap();
    let second = merge_playlists(&conn, &imported).unwrap();

    assert_eq!(playlist_track_ids(&conn, "Favorites"), vec![1, 2]);
    assert_eq!(first.tracks_added, 1);
    assert_eq!(second.tracks_added, 0);
}
