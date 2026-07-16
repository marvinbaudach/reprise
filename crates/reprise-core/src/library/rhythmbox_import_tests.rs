use std::fs;

use tempfile::tempdir;

use super::*;

#[test]
fn prescan_counts_entries_and_classifies_skips() {
    let dir = tempdir().unwrap();
    let music_dir = dir.path().join("music");
    fs::create_dir_all(&music_dir).unwrap();
    let existing = music_dir.join("song.ogg");
    fs::write(&existing, b"fake").unwrap();
    let existing_uri = url::Url::from_file_path(&existing).unwrap();
    let missing_uri = url::Url::from_file_path(music_dir.join("gone.ogg")).unwrap();
    let outside_uri = url::Url::from_file_path(dir.path().join("elsewhere.ogg")).unwrap();
    let xml = format!(
        r#"<?xml version="1.0"?>
<rhythmdb version="2.0">
  <entry type="song"><location>{existing_uri}</location><rating>4</rating><play-count>10</play-count><first-seen>1700000000</first-seen><last-played>1700000500</last-played></entry>
  <entry type="song"><location>{missing_uri}</location><rating>3</rating></entry>
  <entry type="song"><location>{outside_uri}</location><play-count>5</play-count></entry>
  <entry type="podcast-post"><location>file:///podcast.ogg</location><rating>5</rating></entry>
</rhythmdb>"#
    );
    let rhythmdb = dir.path().join("rhythmdb.xml");
    fs::write(&rhythmdb, xml).unwrap();
    let playlists_path = dir.path().join("playlists.xml");
    fs::write(
        &playlists_path,
        r#"<?xml version="1.0"?>
<rhythmdb-playlists>
  <playlist name="Gym" type="static">
    <location>file:///a.ogg</location>
    <location>file:///b.ogg</location>
  </playlist>
</rhythmdb-playlists>"#,
    )
    .unwrap();

    let conn = Connection::open_in_memory().unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, 0, 0)",
        [existing.to_string_lossy()],
    )
    .unwrap();

    let library_root = music_dir.to_string_lossy().to_string();
    let result = prescan_rhythmdb(&rhythmdb, &playlists_path, &conn, Some(&library_root)).unwrap();

    assert_eq!(result.total_entries, 4);
    assert_eq!(result.song_entries, 3);
    assert_eq!(result.non_song_entries, 1);
    assert_eq!(result.rated_tracks, 2);
    assert_eq!(result.tracks_with_history, 2);
    assert_eq!(result.tracks_with_date_added, 1);
    assert_eq!(result.matched, 1);
    assert_eq!(result.outside_library, 1);
    assert_eq!(result.missing_on_disk, 1);
    assert_eq!(result.playlist_count, 1);
    assert_eq!(result.playlist_track_count, 2);
}

fn database(path: &Path, rating: i32, play_count: i64) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, ?2, ?3)",
        rusqlite::params![path.to_string_lossy(), rating, play_count],
    )
    .unwrap();
    conn
}

fn values(conn: &Connection) -> (i32, i64) {
    conn.query_row("SELECT rating, play_count FROM tracks", [], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })
    .unwrap()
}

#[test]
fn parser_keeps_only_songs_and_decodes_file_uris() {
    let dir = tempdir().unwrap();
    let track = dir.path().join("Artist & Album").join("Song 1.ogg");
    let uri = url::Url::from_file_path(&track).unwrap();
    let uri = quick_xml::escape::escape(uri.as_str());
    let xml = format!(
        r#"<?xml version="1.0"?>
<rhythmdb version="2.0">
  <entry type="song"><location>{uri}</location><rating>4</rating><play-count>17</play-count><first-seen>1700000000</first-seen><last-played>1700000500</last-played></entry>
  <entry type="podcast-post"><location>file:///ignored.ogg</location><rating>5</rating></entry>
</rhythmdb>"#
    );
    let path = dir.path().join("rhythmdb.xml");
    fs::write(&path, xml).unwrap();

    assert_eq!(
        parse_rhythmdb(&path).unwrap(),
        vec![RhythmboxTrackStats {
            path: track,
            rating: Some(4),
            play_count: Some(17),
            added_at: Some(1_700_000_000),
            last_played_at: Some(1_700_000_500),
        }]
    );
}

#[test]
fn parser_skips_invalid_entries_but_rejects_broken_xml() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("rhythmdb.xml");
    fs::write(
            &path,
            r#"<rhythmdb>
<entry type="song"><location>https://example.com/song.ogg</location></entry>
<entry type="song"><location>file:///valid.ogg</location><rating>99</rating><play-count>-2</play-count><first-seen>-1</first-seen><last-played>0</last-played></entry>
</rhythmdb>"#,
        )
        .unwrap();
    assert!(parse_rhythmdb(&path).unwrap().is_empty());

    fs::write(&path, "<rhythmdb><entry>").unwrap();
    assert!(parse_rhythmdb(&path).is_err());
}

#[test]
fn merge_preserves_local_rating_and_never_decreases_play_count() {
    let path = PathBuf::from("/music/song.ogg");
    let mut conn = database(&path, 5, 20);
    let (summary, _) = merge_stats(
        &mut conn,
        &[RhythmboxTrackStats {
            path,
            rating: Some(3),
            play_count: Some(12),
            added_at: None,
            last_played_at: None,
        }],
        RhythmboxImportChoices {
            ratings: true,
            play_counts_and_last_played: true,
            added_at: false,
        },
        None,
    )
    .unwrap();

    assert_eq!(values(&conn), (5, 20));
    assert_eq!(summary.matched, 1);
    assert_eq!(summary.ratings_imported, 0);
    assert_eq!(summary.play_counts_raised, 0);
}

#[test]
fn merge_imports_missing_rating_and_higher_count_idempotently() {
    let path = PathBuf::from("/music/song.ogg");
    let mut conn = database(&path, 0, 2);
    let imported = [RhythmboxTrackStats {
        path,
        rating: Some(4),
        play_count: Some(11),
        added_at: None,
        last_played_at: None,
    }];
    let choices = RhythmboxImportChoices {
        ratings: true,
        play_counts_and_last_played: true,
        added_at: false,
    };

    let (first, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
    let (second, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();

    assert_eq!(values(&conn), (4, 11));
    assert_eq!((first.ratings_imported, first.play_counts_raised), (1, 1));
    assert_eq!((second.ratings_imported, second.play_counts_raised), (0, 0));
}

#[test]
fn merge_respects_choices_and_counts_unmatched_entries() {
    let path = PathBuf::from("/music/song.ogg");
    let mut conn = database(&path, 0, 1);
    let (summary, _) = merge_stats(
        &mut conn,
        &[
            RhythmboxTrackStats {
                path,
                rating: Some(5),
                play_count: Some(8),
                added_at: None,
                last_played_at: None,
            },
            RhythmboxTrackStats {
                path: PathBuf::from("/music/missing.ogg"),
                rating: Some(3),
                play_count: Some(4),
                added_at: None,
                last_played_at: None,
            },
        ],
        RhythmboxImportChoices {
            ratings: false,
            play_counts_and_last_played: true,
            added_at: false,
        },
        None,
    )
    .unwrap();

    assert_eq!(values(&conn), (0, 8));
    assert_eq!(summary.parsed, 2);
    assert_eq!(summary.matched, 1);
    assert_eq!(summary.skipped, 1);
}

#[test]
fn merge_imports_only_an_older_positive_date_added_idempotently() {
    let path = PathBuf::from("/music/song.ogg");
    let mut conn = database(&path, 0, 0);
    conn.execute("UPDATE tracks SET added_at=200", []).unwrap();
    let imported = [RhythmboxTrackStats {
        path,
        rating: None,
        play_count: None,
        added_at: Some(100),
        last_played_at: None,
    }];
    let choices = RhythmboxImportChoices {
        ratings: false,
        play_counts_and_last_played: false,
        added_at: true,
    };

    let (first, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
    let (second, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
    let (newer, _) = merge_stats(
        &mut conn,
        &[RhythmboxTrackStats {
            path: PathBuf::from("/music/song.ogg"),
            rating: None,
            play_count: None,
            added_at: Some(300),
            last_played_at: None,
        }],
        choices,
        None,
    )
    .unwrap();
    let added_at = conn
        .query_row("SELECT added_at FROM tracks", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();

    assert_eq!(added_at, 100);
    assert_eq!(first.dates_imported, 1);
    assert_eq!(second.dates_imported, 0);
    assert_eq!(newer.dates_imported, 0);

    let missing_path = PathBuf::from("/music/without-date.ogg");
    let mut missing_conn = database(&missing_path, 0, 0);
    let (missing, _) = merge_stats(
        &mut missing_conn,
        &[RhythmboxTrackStats {
            path: missing_path,
            rating: None,
            play_count: None,
            added_at: Some(100),
            last_played_at: None,
        }],
        choices,
        None,
    )
    .unwrap();
    let imported_missing = missing_conn
        .query_row("SELECT added_at FROM tracks", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    assert_eq!(imported_missing, 100);
    assert_eq!(missing.dates_imported, 1);
}

#[test]
fn merge_imports_only_a_newer_positive_last_played_idempotently() {
    let path = PathBuf::from("/music/song.ogg");
    let mut conn = database(&path, 0, 0);
    conn.execute("UPDATE tracks SET last_played_at=100", [])
        .unwrap();
    let imported = [RhythmboxTrackStats {
        path,
        rating: None,
        play_count: None,
        added_at: None,
        last_played_at: Some(200),
    }];
    let choices = RhythmboxImportChoices {
        ratings: false,
        play_counts_and_last_played: true,
        added_at: false,
    };

    let (first, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
    let (second, _) = merge_stats(&mut conn, &imported, choices, None).unwrap();
    let (older, _) = merge_stats(
        &mut conn,
        &[RhythmboxTrackStats {
            path: PathBuf::from("/music/song.ogg"),
            rating: None,
            play_count: None,
            added_at: None,
            last_played_at: Some(50),
        }],
        choices,
        None,
    )
    .unwrap();
    let last_played_at = conn
        .query_row("SELECT last_played_at FROM tracks", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .unwrap();

    assert_eq!(last_played_at, Some(200));
    assert_eq!(first.last_played_imported, 1);
    assert_eq!(second.last_played_imported, 0);
    assert_eq!(older.last_played_imported, 0);

    let missing_path = PathBuf::from("/music/never-played.ogg");
    let mut missing_conn = database(&missing_path, 0, 0);
    let (missing, _) = merge_stats(
        &mut missing_conn,
        &[RhythmboxTrackStats {
            path: missing_path,
            rating: None,
            play_count: None,
            added_at: None,
            last_played_at: Some(200),
        }],
        choices,
        None,
    )
    .unwrap();
    let imported_missing = missing_conn
        .query_row("SELECT last_played_at FROM tracks", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .unwrap();
    assert_eq!(imported_missing, Some(200));
    assert_eq!(missing.last_played_imported, 1);
}

#[test]
fn merge_returns_rollback_and_undo_restores_original_values() {
    let path = PathBuf::from("/music/song.ogg");
    let mut conn = database(&path, 3, 5);
    conn.execute("UPDATE tracks SET added_at = 100, last_played_at = 200", [])
        .unwrap();

    let (summary, rollback) = merge_stats(
        &mut conn,
        &[RhythmboxTrackStats {
            path: path.clone(),
            rating: Some(5),
            play_count: Some(20),
            added_at: Some(50),
            last_played_at: Some(300),
        }],
        RhythmboxImportChoices {
            ratings: true,
            play_counts_and_last_played: true,
            added_at: true,
        },
        None,
    )
    .unwrap();

    // Verify import took effect
    assert_eq!(summary.play_counts_raised, 1);
    assert_eq!(summary.dates_imported, 1);
    assert_eq!(summary.last_played_imported, 1);
    assert_eq!(values(&conn), (3, 20)); // rating unchanged (was already set)

    // Undo
    let restored = undo_rhythmbox_import(&mut conn, &rollback).unwrap();
    assert_eq!(restored, 1);
    assert_eq!(values(&conn), (3, 5));
    let (added_at, last_played) = conn
        .query_row("SELECT added_at, last_played_at FROM tracks", [], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?))
        })
        .unwrap();
    assert_eq!(added_at, 100);
    assert_eq!(last_played, Some(200));
}

#[test]
fn merge_calls_progress_for_each_track() {
    let path1 = PathBuf::from("/music/a.ogg");
    let path2 = PathBuf::from("/music/b.ogg");
    let conn_raw = Connection::open_in_memory().unwrap();
    crate::db::migrate(&conn_raw).unwrap();
    conn_raw
        .execute(
            "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, 0, 0)",
            [path1.to_string_lossy()],
        )
        .unwrap();
    conn_raw
        .execute(
            "INSERT INTO tracks (path, added_at, rating, play_count) VALUES (?1, 0, 0, 0)",
            [path2.to_string_lossy()],
        )
        .unwrap();
    let mut conn = conn_raw;

    let progress = std::cell::Cell::new(0usize);
    let (_, _) = merge_stats(
        &mut conn,
        &[
            RhythmboxTrackStats {
                path: path1,
                rating: Some(4),
                play_count: None,
                added_at: None,
                last_played_at: None,
            },
            RhythmboxTrackStats {
                path: path2,
                rating: Some(3),
                play_count: None,
                added_at: None,
                last_played_at: None,
            },
        ],
        RhythmboxImportChoices {
            ratings: true,
            play_counts_and_last_played: false,
            added_at: false,
        },
        Some(&|n| {
            progress.set(n);
        }),
    )
    .unwrap();

    assert_eq!(progress.get(), 2);
}
