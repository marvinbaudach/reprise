use tempfile::TempDir;

use super::*;

#[test]
fn cover_1_album_directories_come_only_from_matching_live_track_paths() {
    let library = TempDir::new().unwrap();
    let first = library.path().join("disc-one");
    let second = library.path().join("disc-two");
    let other = library.path().join("other");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::create_dir_all(&other).unwrap();

    let db = Db::open_in_memory().unwrap();
    let conn = db.conn();
    let rows = [
        (
            first.join("one.flac"),
            " Album ",
            "Track Artist",
            " Album Artist ",
            None,
        ),
        (
            first.join("two.flac"),
            "album",
            "Track Artist",
            "album artist",
            None,
        ),
        (second.join("three.flac"), "ALBUM", "Album Artist", "", None),
        (other.join("other.flac"), "Other", "Album Artist", "", None),
        (
            other.join("missing.flac"),
            "Album",
            "Album Artist",
            "",
            Some(1_i64),
        ),
    ];
    for (path, album, artist, album_artist, missing_since) in rows {
        conn.execute(
            "INSERT INTO tracks \
             (path,title,album,artist,album_artist,added_at,missing_since) \
             VALUES (?1,'Track',?2,?3,?4,0,?5)",
            rusqlite::params![
                path.to_string_lossy(),
                album,
                artist,
                album_artist,
                missing_since
            ],
        )
        .unwrap();
    }

    assert_eq!(
        query_album_directories(&db, "album", "album artist").unwrap(),
        vec![first, second]
    );
}
