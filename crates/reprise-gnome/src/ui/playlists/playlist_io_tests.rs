use super::*;
use reprise_core::models::Track;

fn seeded_conn() -> Rc<Db> {
    Rc::new(crate::test_db::open().unwrap())
}

#[test]
fn playlist_name_from_file_uses_stem() {
    let name = playlist_name_from_file(Path::new("/x/Road Trip.m3u"));
    assert_eq!(name, "Road Trip");
}

#[test]
fn playlist_name_from_file_falls_back_when_stem_missing() {
    // A path with no filename component at all (`Path::file_stem`
    // returns `None`, unlike a dotfile such as ".m3u" — which Rust
    // treats as a bare filename with no extension, so its "stem" is the
    // whole ".m3u" string, not a missing one).
    let name = playlist_name_from_file(Path::new("/"));
    assert_eq!(
        name,
        strings::text(strings::IMPORTED_PLAYLIST_FALLBACK_NAME)
    );
}

#[test]
fn display_name_uses_artist_and_title() {
    let mut track = sample_track();
    track.artist = "Some Artist".to_string();
    track.title = "Some Title".to_string();
    assert_eq!(display_name(&track), "Some Artist - Some Title");
}

#[test]
fn display_name_falls_back_to_title_only_when_artist_blank() {
    let mut track = sample_track();
    track.artist = "  ".to_string();
    track.title = "Some Title".to_string();
    assert_eq!(display_name(&track), "Some Title");
}

fn sample_track() -> Track {
    Track {
        segment: None,
        id: 1,
        path: "/x/a.flac".to_string(),
        title: String::new(),
        artist: String::new(),
        album: String::new(),
        album_artist: String::new(),
        year: None,
        track_no: None,
        genre: String::new(),
        duration_ms: 0,
        bitrate_kbps: None,
        rating: 0,
        play_count: 0,
        last_played_at: None,
        added_at: 0,
        file_mtime: 0,
        missing_since: None,
        missing_reason: None,
        untagged: false,
        file_size: 0,
        device: None,
        inode: None,
        playlist_position: None,
        is_ai: false,
    }
}

#[test]
fn import_playlist_matches_exact_absolute_paths_and_counts_unmatched() {
    let conn = seeded_conn();
    {
        let c = &conn;
        crate::test_db::connection(c)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
             VALUES (1, '/music/a.flac', 'A', 'Artist A', 3000, 0)",
                [],
            )
            .unwrap();
        crate::test_db::connection(c)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
             VALUES (2, '/music/b.flac', 'B', 'Artist B', 4000, 0)",
                [],
            )
            .unwrap();
    }

    let dir = std::env::temp_dir().join(format!("reprise-m3u-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let m3u_path = dir.join("My Mix.m3u");
    std::fs::write(
        &m3u_path,
        "#EXTM3U\n/music/a.flac\n/music/nowhere.flac\n/music/b.flac\n",
    )
    .unwrap();

    let outcome = import_playlist(&conn, &m3u_path).unwrap();
    assert_eq!(outcome.name, "My Mix");
    assert_eq!(outcome.total, 3);
    assert_eq!(outcome.matched, 2);

    let track_ids: Vec<i64> = crate::test_db::connection(&conn)
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(rusqlite::params![outcome.playlist_id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(track_ids, vec![1, 2]);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn import_playlist_resolves_relative_paths_against_m3u_directory() {
    let conn = seeded_conn();
    let dir = std::env::temp_dir().join(format!("reprise-m3u-test-rel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let track_path = dir.join("song.flac");
    {
        let c = &conn;
        crate::test_db::connection(c)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
             VALUES (1, ?1, 'S', 'Art', 1000, 0)",
                rusqlite::params![track_path.to_string_lossy().to_string()],
            )
            .unwrap();
    }
    let m3u_path = dir.join("rel.m3u");
    std::fs::write(&m3u_path, "#EXTM3U\nsong.flac\n").unwrap();

    let outcome = import_playlist(&conn, &m3u_path).unwrap();
    assert_eq!(outcome.matched, 1);
    assert_eq!(outcome.total, 1);

    std::fs::remove_dir_all(&dir).ok();
}

/// TDD regression for the "0-of-N-matched should not create a playlist"
/// finding: an all-bogus `.m3u` file (no path line matches any library
/// track) must not leave an empty, unremovable playlist behind.
#[test]
fn import_playlist_zero_matched_creates_no_playlist() {
    let conn = seeded_conn();
    {
        let c = &conn;
        crate::test_db::connection(c)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
             VALUES (1, '/music/a.flac', 'A', 'Artist A', 3000, 0)",
                [],
            )
            .unwrap();
    }

    let dir = std::env::temp_dir().join(format!("reprise-m3u-test-zero-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let m3u_path = dir.join("Bogus.m3u");
    std::fs::write(
        &m3u_path,
        "#EXTM3U\n/music/nowhere.flac\n/music/also-nowhere.flac\n",
    )
    .unwrap();

    let before_count: i64 = crate::test_db::connection(&conn)
        .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
        .unwrap();

    let outcome = import_playlist(&conn, &m3u_path).unwrap();
    assert_eq!(outcome.matched, 0);
    assert_eq!(outcome.total, 2);
    assert_eq!(outcome.playlist_id, None);

    let after_count: i64 = crate::test_db::connection(&conn)
        .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        before_count, after_count,
        "zero-matched import must not create a playlist row"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// TDD regression for the "non-UTF-8 import" robustness gap: a `.m3u`
/// file with one valid path line plus a trailing invalid-UTF-8 byte must
/// still import successfully (lossy-decode, not panic/error) and still
/// match the valid line.
#[test]
fn import_playlist_handles_non_utf8_bytes_via_lossy_decode() {
    let conn = seeded_conn();
    {
        let c = &conn;
        crate::test_db::connection(c)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
             VALUES (1, '/music/a.flac', 'A', 'Artist A', 3000, 0)",
                [],
            )
            .unwrap();
    }

    let dir = std::env::temp_dir().join(format!("reprise-m3u-test-nonutf8-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let m3u_path = dir.join("bad-encoding.m3u");
    // Valid ASCII header + path line, then a lone 0xFF byte (invalid as
    // any UTF-8 sequence) on its own line — simulates a filesystem/tag
    // encoding glitch in one entry without corrupting the whole file.
    let mut bytes = b"#EXTM3U\n/music/a.flac\n".to_vec();
    bytes.push(0xFF);
    bytes.push(b'\n');
    std::fs::write(&m3u_path, &bytes).unwrap();

    let outcome = import_playlist(&conn, &m3u_path).unwrap();
    assert_eq!(outcome.matched, 1, "the valid path line should still match");
    assert!(outcome.playlist_id.is_some());

    std::fs::remove_dir_all(&dir).ok();
}

/// TDD regression for the "file-read error" robustness gap: a path that
/// doesn't exist on disk must return `Err(ImportError::Io(_))`, not
/// panic.
#[test]
fn import_playlist_nonexistent_path_returns_io_error() {
    let conn = seeded_conn();
    let path = std::env::temp_dir().join(format!(
        "reprise-m3u-does-not-exist-{}.m3u",
        std::process::id()
    ));

    let result = import_playlist(&conn, &path);
    assert!(matches!(result, Err(ImportError::Io(_))));
}

/// Same robustness gap, other failure shape: a path that exists but is a
/// directory (not a regular file) must also return `Err(ImportError::
/// Io(_))`, not panic — `std::fs::read` fails on a directory with an
/// "Is a directory" `io::Error`.
#[test]
fn import_playlist_directory_path_returns_io_error() {
    let conn = seeded_conn();
    let dir = std::env::temp_dir().join(format!("reprise-m3u-test-dirpath-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let result = import_playlist(&conn, &dir);
    assert!(matches!(result, Err(ImportError::Io(_))));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn export_playlist_writes_absolute_paths_and_extinf() {
    let conn = seeded_conn();
    let playlist_id = {
        let c = &conn;
        crate::test_db::connection(c)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
             VALUES (1, '/music/a.flac', 'Title A', 'Artist A', 125000, 0)",
                [],
            )
            .unwrap();
        let id = playlists::create(c, "Exported").unwrap();
        playlists::add_tracks(c, id, &[1]).unwrap();
        id
    };

    let dir = std::env::temp_dir().join(format!("reprise-m3u-test-exp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out_path = dir.join("out.m3u");

    let count = export_playlist(&conn, playlist_id, &out_path).unwrap();
    assert_eq!(count, 1);

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(content.starts_with("#EXTM3U\n"));
    assert!(content.contains("#EXTINF:125,Artist A - Title A"));
    assert!(content.contains("/music/a.flac"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cue_6a_an_m3u_naming_a_cue_file_imports_its_tracks_once_per_run_of_lines() {
    let conn = seeded_conn();
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, added_at, segment_index) \
             VALUES (7, '/music/live.flac', 'Two', 0, 2), (6, '/music/live.flac', 'One', 0, 1), \
                    (8, '/music/plain.flac', 'Plain', 0, 0);",
        )
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let m3u_path = dir.path().join("Live.m3u");
    std::fs::write(
        &m3u_path,
        "#EXTM3U\n/music/live.flac\n/music/live.flac\n/music/plain.flac\n",
    )
    .unwrap();

    let outcome = import_playlist(&conn, &m3u_path).unwrap();

    assert_eq!((outcome.matched, outcome.total), (3, 3));
    let track_ids: Vec<i64> = crate::test_db::connection(&conn)
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map(rusqlite::params![outcome.playlist_id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        track_ids,
        [6, 7, 8],
        "an exported album, one line per track, comes back as the album"
    );
}
