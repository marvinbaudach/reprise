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

    let all =
        query_track_count_browsed_ai(&db, &ViewSource::Library, "", &browse, &[], false).unwrap();
    assert_eq!(all, 2, "without the filter both present tracks count");
    let non_ai =
        query_track_count_browsed_ai(&db, &ViewSource::Library, "", &browse, &[], true).unwrap();
    assert_eq!(non_ai, 1, "the AI instrumental is excluded from the count");

    // The COUNT(*) agrees with the AI-filtered id list it replaces.
    let ids = query_track_ids_browsed_ai(
        &db,
        &ViewSource::Library,
        "title",
        "asc",
        "",
        &browse,
        &[],
        true,
    )
    .unwrap();
    assert_eq!(
        non_ai as usize,
        ids.len(),
        "count matches the AI-filtered id list length"
    );
}
