//! Tests for the P3b facades: the by-id source-path lookup the instrumental
//! worker resolves through, and the FIL-7 AI-exclude `COUNT(*)`. Split from
//! `tests.rs` purely to keep every file under the project's 800-line rule.

use super::*;

#[test]
fn track_source_path_returns_the_absolute_path_or_none() {
    // The focused by-id path lookup the instrumental worker resolves a job's
    // source_track_id through (P3b).
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
         VALUES (1, '/music/song.flac', 'S', 'A', 1, 1, 1)",
        [],
    )
    .unwrap();
    assert_eq!(
        track_source_path(&db, 1).unwrap(),
        Some(std::path::PathBuf::from("/music/song.flac"))
    );
    assert_eq!(
        track_source_path(&db, 999).unwrap(),
        None,
        "a missing row is None, not an error"
    );
}

fn insert_track_at(db: &crate::db::Db, id: i64, path: &str) {
    db.conn()
        .execute(
            "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
             VALUES (?1, ?2, 'S', 'A', 1, 1, 1)",
            rusqlite::params![id, path],
        )
        .unwrap();
}

#[test]
fn track_source_paths_resolves_present_ids_and_omits_missing_ones() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_track_at(&db, 1, "/music/a.flac");
    insert_track_at(&db, 2, "/music/b.flac");

    let paths = track_source_paths(&db, &[2, 999, 1]).unwrap();

    assert_eq!(paths.len(), 2, "the missing id has no entry");
    assert_eq!(paths[&1], std::path::PathBuf::from("/music/a.flac"));
    assert_eq!(paths[&2], std::path::PathBuf::from("/music/b.flac"));
    assert!(track_source_paths(&db, &[]).unwrap().is_empty());
}

#[test]
fn track_source_paths_tolerates_duplicate_ids() {
    let db = crate::db::Db::open_in_memory().unwrap();
    insert_track_at(&db, 7, "/music/seven.flac");

    let paths = track_source_paths(&db, &[7, 7, 8, 7]).unwrap();

    assert_eq!(paths.len(), 1);
    assert_eq!(paths[&7], std::path::PathBuf::from("/music/seven.flac"));
}

#[test]
fn track_source_paths_resolves_more_ids_than_one_chunk_holds() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let count = i64::try_from(queue::SOURCE_PATH_CHUNK).unwrap() * 2 + 17;
    for id in 1..=count {
        insert_track_at(&db, id, &format!("/music/{id}.flac"));
    }
    // One id past the last row stays missing; one id repeats across chunks.
    let mut ids: Vec<i64> = (1..=count + 1).collect();
    ids.push(1);

    let paths = track_source_paths(&db, &ids).unwrap();

    assert_eq!(paths.len(), usize::try_from(count).unwrap());
    for id in [1, count / 2, count] {
        assert_eq!(
            paths[&id],
            std::path::PathBuf::from(format!("/music/{id}.flac"))
        );
    }
    assert!(!paths.contains_key(&(count + 1)));
}

#[test]
fn fil_7_count_browsed_ai_excludes_ai_tracks_via_count_star() {
    // The cheap COUNT(*) variant that replaces the QUEUE_LIMIT-capped
    // ids.len() fallback: with exclude_ai it counts only non-AI Library
    // tracks; without it, every present track.
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
           VALUES (1, '/a.flac', 'Original', 'A', 1, 1, 1);
         INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
           VALUES (2, '/b.flac', 'Instrumental', 'A', 1, 1, 1);
         INSERT INTO track_provenance (track_id, kind, ai, created_at) \
           VALUES (2, 'vocals-removed', 1, 0);",
    )
    .unwrap();
    let browse = BrowseFilter::default();

    let all = query_track_count(
        &db,
        &TrackViewQuery::new(&ViewSource::Library)
            .with_browse(&browse)
            .with_exclude_ai(false),
    )
    .unwrap();
    assert_eq!(all, 2, "without the filter both present tracks count");
    let non_ai = query_track_count(
        &db,
        &TrackViewQuery::new(&ViewSource::Library)
            .with_browse(&browse)
            .with_exclude_ai(true),
    )
    .unwrap();
    assert_eq!(non_ai, 1, "the AI instrumental is excluded from the count");

    // The COUNT(*) agrees with the AI-filtered id list it replaces.
    let ids = query_track_ids(
        &db,
        &TrackViewQuery::new(&ViewSource::Library)
            .with_browse(&browse)
            .with_exclude_ai(true),
        test_sort("title", "asc"),
    )
    .unwrap();
    assert_eq!(
        non_ai as usize,
        ids.len(),
        "count matches the AI-filtered id list length"
    );
}
