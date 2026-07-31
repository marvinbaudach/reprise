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

#[test]
fn cover_1_a_directory_shared_with_another_album_is_no_album_directory() {
    let library = TempDir::new().unwrap();
    let flat = library.path().join("all my music");
    let own = library.path().join("Album Artist - Album");
    let bonus = own.join("bonus disc");
    std::fs::create_dir_all(&flat).unwrap();
    std::fs::create_dir_all(&bonus).unwrap();

    let db = Db::open_in_memory().unwrap();
    let conn = db.conn();
    let rows = [
        // A flat library: the album's tracks share the folder with another
        // album, so there is no folder that belongs to this album alone.
        (flat.join("one.flac"), "Album", "Album Artist"),
        (flat.join("two.flac"), "Other Album", "Album Artist"),
        // A folder of its own stays a target even when a *sub*folder holds
        // something else.
        (own.join("three.flac"), "Album", "Album Artist"),
        (bonus.join("four.flac"), "Other Album", "Album Artist"),
    ];
    for (path, album, album_artist) in rows {
        conn.execute(
            "INSERT INTO tracks (path,title,album,artist,album_artist,added_at) \
             VALUES (?1,'Track',?2,?3,?4,0)",
            rusqlite::params![path.to_string_lossy(), album, "Track Artist", album_artist],
        )
        .unwrap();
    }

    assert_eq!(
        query_album_directories(&db, "Album", "Album Artist").unwrap(),
        vec![own]
    );
}

#[test]
fn cover_1_a_directory_name_with_like_wildcards_is_matched_literally() {
    let library = TempDir::new().unwrap();
    let wildcards = library.path().join("100% _live_");
    let decoy = library.path().join("1005 xlivex");
    std::fs::create_dir_all(&wildcards).unwrap();
    std::fs::create_dir_all(&decoy).unwrap();

    let db = Db::open_in_memory().unwrap();
    let conn = db.conn();
    for (path, album) in [
        (wildcards.join("one.flac"), "Album"),
        (decoy.join("two.flac"), "Other Album"),
    ] {
        conn.execute(
            "INSERT INTO tracks (path,title,album,artist,album_artist,added_at) \
             VALUES (?1,'Track',?2,'Track Artist','Album Artist',0)",
            rusqlite::params![path.to_string_lossy(), album],
        )
        .unwrap();
    }

    assert_eq!(
        query_album_directories(&db, "Album", "Album Artist").unwrap(),
        vec![wildcards]
    );
}
