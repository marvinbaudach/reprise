//! Every lookup that takes a path meets a file that holds several tracks. Each
//! one either means the whole file or names the track it wants; none returns
//! "the first row it happens to find".

use std::path::Path;

use crate::db::Db;

/// A file cut into three tracks whose ids run against their play order, and a
/// plain file beside it.
fn seeded() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, added_at,
                                 segment_index, segment_start_ms, segment_end_ms, duration_ms)
             VALUES (30, '/m/live.flac', 'One', 'Band', 'Live', 'Band', 1, 1, 0, 400, 400),
                    (20, '/m/live.flac', 'Two', 'Other', 'Live', 'Band', 1, 2, 400, 900, 500),
                    (10, '/m/live.flac', 'Three', 'Band', 'Live', 'Band', 1, 3, 900, 1160, 260),
                    (40, '/m/plain.flac', 'Plain', 'Solo', 'Single', '', 1, 0, NULL, NULL, 1160);",
        )
        .unwrap();
    db
}

#[test]
fn cue_6_a_path_lists_every_track_in_the_order_they_play() {
    let db = seeded();

    assert_eq!(
        crate::queries::track_ids_for_path(&db, "/m/live.flac").unwrap(),
        [30, 20, 10]
    );
    assert_eq!(
        crate::queries::track_ids_for_path(&db, "/m/plain.flac").unwrap(),
        [40]
    );
    assert!(crate::queries::track_ids_for_path(&db, "/m/none.flac")
        .unwrap()
        .is_empty());
}

#[test]
fn cue_6_an_m3u_path_resolves_to_the_first_track_not_the_lowest_id() {
    let db = seeded();

    assert_eq!(
        crate::queries::track_id_for_path(&db, "/m/live.flac").unwrap(),
        Some(30)
    );
    assert_eq!(
        crate::queries::track_id_for_path(&db, "/m/plain.flac").unwrap(),
        Some(40)
    );
}

#[test]
fn cue_6_the_stats_target_of_a_path_is_its_first_track() {
    let db = seeded();

    let target = crate::queries::query_stats_album_target_for_path(&db, "/m/live.flac")
        .unwrap()
        .unwrap();

    assert_eq!(target, (30, "Live".to_string(), "Band".to_string()));
}

#[test]
fn cue_6_a_promoted_render_is_the_whole_file_row_only() {
    let db = seeded();

    let registered = |path: &str| {
        crate::ai_promotion::registered_whole_file_track(db.conn(), Path::new(path)).unwrap()
    };

    assert_eq!(registered("/m/plain.flac"), Some(40));
    assert_eq!(registered("/m/live.flac"), None);
}

#[test]
fn cue_6_rhythmbox_ratings_reach_whole_files_only() {
    use crate::library::rhythmbox_import::{
        merge_stats, RhythmboxImportChoices, RhythmboxTrackStats,
    };
    let db = seeded();
    let stat = |path: &str| RhythmboxTrackStats {
        path: path.into(),
        rating: Some(5),
        play_count: Some(3),
        added_at: None,
        last_played_at: None,
    };
    let choices = RhythmboxImportChoices {
        ratings: true,
        play_counts_and_last_played: true,
        added_at: false,
    };

    let (summary, _) = merge_stats(
        &db,
        &[stat("/m/live.flac"), stat("/m/plain.flac")],
        choices,
        None,
    )
    .unwrap();

    assert_eq!((summary.matched, summary.skipped), (1, 1));
    let rated: i64 = db
        .conn()
        .query_row("SELECT count(*) FROM tracks WHERE rating = 5", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rated, 1, "no track of the CUE file was rated");
}

#[test]
fn cue_6_a_sidecar_is_registered_for_a_whole_file_only() {
    let db = seeded();

    crate::db_mobile_sync::register_sidecar(
        db.conn(),
        "/m/live.flac",
        Path::new("/m/live.reprise-analysis"),
    )
    .unwrap();
    crate::db_mobile_sync::register_sidecar(
        db.conn(),
        "/m/plain.flac",
        Path::new("/m/plain.reprise-analysis"),
    )
    .unwrap();

    let registered: Vec<i64> = db
        .conn()
        .prepare("SELECT track_id FROM track_analysis_sidecars")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(registered, [40]);
}

#[test]
fn cue_6_the_device_path_of_a_file_reaches_each_of_its_tracks() {
    let db = seeded();

    crate::db_mobile_sync::register_device_path(db.conn(), "/m/live.flac", "Music/live.flac")
        .unwrap();

    for id in [30, 20, 10] {
        assert_eq!(
            crate::db_mobile_sync::device_path_for_track(&db, id).unwrap(),
            Some("Music/live.flac".to_string())
        );
    }
}

#[test]
fn cue_7_locating_one_missing_track_moves_the_file_with_all_of_its_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let new_path = dir.path().join("moved.flac");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac"),
        &new_path,
    )
    .unwrap();
    let db = seeded();
    db.conn()
        .execute(
            "UPDATE tracks SET missing_since = 5, missing_reason = 'deleted' WHERE path = '/m/live.flac'",
            [],
        )
        .unwrap();
    let target = crate::library::relink::RelinkTarget {
        track_id: 20,
        old_path: "/m/live.flac".into(),
    };

    let mismatch = crate::library::relink::probe_relink(&db, &target, &new_path).unwrap();
    crate::library::relink::relink_track(&db, &target, &new_path).unwrap();

    assert!(
        mismatch.is_none(),
        "the file's length is the sheet's last end"
    );
    let rows: Vec<(i64, String, Option<i64>, i64)> = db
        .conn()
        .prepare("SELECT id, title, missing_since, segment_index FROM tracks WHERE path = ?1 ORDER BY segment_index")
        .unwrap()
        .query_map([new_path.to_string_lossy().to_string()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows,
        [
            (30, "One".to_string(), None, 1),
            (20, "Two".to_string(), None, 2),
            (10, "Three".to_string(), None, 3),
        ],
        "every track came along and none took the file's tags"
    );
}

/// `seeded`'s CUE file gone from its place and found again at `new_path`, with
/// the identity of the file there.
fn seeded_and_moved_to(new_path: &Path) -> Db {
    use std::os::unix::fs::MetadataExt;
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac"),
        new_path,
    )
    .unwrap();
    let metadata = std::fs::metadata(new_path).unwrap();
    let db = seeded();
    db.conn()
        .execute(
            "UPDATE tracks SET missing_since = 5, missing_reason = 'deleted', device = ?1, inode = ?2 \
             WHERE path = '/m/live.flac'",
            rusqlite::params![metadata.dev() as i64, metadata.ino() as i64],
        )
        .unwrap();
    db
}

fn targets(ids: &[i64]) -> Vec<crate::library::relink::RelinkTarget> {
    ids.iter()
        .map(|id| crate::library::relink::RelinkTarget {
            track_id: *id,
            old_path: "/m/live.flac".into(),
        })
        .collect()
}

#[test]
fn cue_7_locating_a_folder_counts_every_track_the_file_brought_back() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded_and_moved_to(&dir.path().join("moved.flac"));

    let report = crate::library::relink::relink_from_folder(
        &db,
        dir.path(),
        &targets(&[30, 20, 10]),
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();

    assert_eq!((report.relinked, report.group_size), (3, 3));
}

#[test]
fn cue_7_locating_a_cue_track_leaves_a_sibling_removed_from_the_library_removed() {
    let dir = tempfile::tempdir().unwrap();
    let new_path = dir.path().join("moved.flac");
    let db = seeded_and_moved_to(&new_path);
    db.conn()
        .execute("UPDATE tracks SET removed_at = 7 WHERE id = 10", [])
        .unwrap();

    crate::library::relink::relink_track(&db, &targets(&[20])[0], &new_path).unwrap();

    let removed: (String, Option<i64>) = db
        .conn()
        .query_row("SELECT path, removed_at FROM tracks WHERE id = 10", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(
        removed,
        (new_path.to_string_lossy().into_owned(), Some(7)),
        "it moves with its file and stays removed"
    );
}
