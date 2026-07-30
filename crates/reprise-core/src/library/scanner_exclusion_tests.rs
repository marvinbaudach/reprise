//! Scan exclusion lifecycle for deliberate Remove-from-Library actions.

use super::*;

#[test]
fn browse_7_removed_file_stays_excluded_across_rename_until_reset() {
    let root = tempfile::tempdir().unwrap();
    let original = super::tests::fixture_copy(root.path(), "excluded.flac");
    let conn = crate::db::Db::open_in_memory().unwrap();
    assert_eq!(
        super::tests::completed(scan_folder(&conn, root.path()).unwrap()).added,
        1
    );
    let track_id: i64 = conn
        .conn()
        .query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [original.to_string_lossy().to_string()],
            |row| row.get(0),
        )
        .unwrap();

    let removed =
        crate::queries::exclude_tracks_matching_paths(&conn, &[(track_id, original.clone())], 100)
            .unwrap();
    assert_eq!(removed, vec![track_id]);
    assert_eq!(super::tests::row_count(conn.conn()), 0);

    let renamed = root.path().join("renamed.flac");
    std::fs::rename(&original, &renamed).unwrap();
    let excluded = super::tests::completed(scan_folder(&conn, root.path()).unwrap());
    assert_eq!((excluded.excluded, excluded.added), (1, 0));
    assert_eq!(super::tests::row_count(conn.conn()), 0);

    assert_eq!(crate::library::exclusions::clear(&conn).unwrap(), 1);
    let restored = super::tests::completed(scan_folder(&conn, root.path()).unwrap());
    assert_eq!((restored.excluded, restored.added), (0, 1));
    assert_eq!(super::tests::row_count(conn.conn()), 1);
}

#[test]
fn browse_7_replacement_at_the_old_path_is_a_new_catalog_object() {
    let root = tempfile::tempdir().unwrap();
    let original = super::tests::fixture_copy(root.path(), "track.flac");
    let conn = crate::db::Db::open_in_memory().unwrap();
    super::tests::completed(scan_folder(&conn, root.path()).unwrap());
    let track_id: i64 = conn
        .conn()
        .query_row("SELECT id FROM tracks", [], |row| row.get(0))
        .unwrap();
    crate::queries::exclude_tracks_matching_paths(&conn, &[(track_id, original.clone())], 100)
        .unwrap();

    std::fs::rename(&original, root.path().join("excluded-object.flac")).unwrap();
    super::tests::fixture_copy(root.path(), "track.flac");

    let report = super::tests::completed(scan_folder(&conn, root.path()).unwrap());
    assert_eq!((report.excluded, report.added), (1, 1));
    assert_eq!(super::tests::row_count(conn.conn()), 1);

    let replacement_id: i64 = conn
        .conn()
        .query_row("SELECT id FROM tracks", [], |row| row.get(0))
        .unwrap();
    crate::queries::exclude_tracks_matching_paths(&conn, &[(replacement_id, original)], 200)
        .unwrap();
    let both_excluded = super::tests::completed(scan_folder(&conn, root.path()).unwrap());
    assert_eq!((both_excluded.excluded, both_excluded.added), (2, 0));
    assert_eq!(crate::library::exclusions::count(&conn).unwrap(), 2);
}
