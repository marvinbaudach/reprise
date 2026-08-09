//! Regressions for missing rows whose last location cannot be named.
//!
//! These live beside `tests_issues.rs` because that established group-query
//! suite is already too close to the repository's 800-line code-file limit.

use super::*;
use crate::models::MissingReason;

fn seed_missing_track(
    conn: &Connection,
    id: i64,
    reason: MissingReason,
    mount_point: Option<&str>,
) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at, \
         missing_since, missing_reason, mount_point) \
         VALUES (?1, ?2, ?3, 'Artist', 'Album', 1, 0, 1, ?4, ?5)",
        rusqlite::params![
            id,
            format!("/music/{id}.flac"),
            format!("Track {id}"),
            reason.as_str(),
            mount_point,
        ],
    )
    .unwrap();
}

fn removed_at(conn: &Connection, id: i64) -> Option<i64> {
    conn.query_row("SELECT removed_at FROM tracks WHERE id = ?1", [id], |row| {
        row.get(0)
    })
    .unwrap()
}

#[test]
fn unknown_and_mountless_unmounted_rows_form_one_unlocatable_group() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_missing_track(conn, 1, MissingReason::Unknown, None);
    seed_missing_track(conn, 2, MissingReason::Unmounted, None);
    seed_missing_track(conn, 3, MissingReason::Unmounted, Some("/media/NAS"));

    assert_eq!(
        query_missing_groups(&db).unwrap(),
        vec![
            MissingGroup {
                kind: MissingGroupKind::Unavailable {
                    mount_point: "/media/NAS".into(),
                },
                track_count: 1,
            },
            MissingGroup {
                kind: MissingGroupKind::Unlocatable,
                track_count: 2,
            },
        ]
    );
}

#[test]
fn unlocatable_rows_include_both_reasons_while_unavailable_stays_mount_exact() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_missing_track(conn, 1, MissingReason::Unknown, None);
    seed_missing_track(conn, 2, MissingReason::Unmounted, None);
    seed_missing_track(conn, 3, MissingReason::Unmounted, Some("/media/A"));
    seed_missing_track(conn, 4, MissingReason::Unmounted, Some("/media/B"));

    let unlocatable = query_missing_rows(&db, &MissingGroupKind::Unlocatable, 0, 100).unwrap();
    assert_eq!(
        unlocatable.iter().map(|track| track.id).collect::<Vec<_>>(),
        vec![1, 2]
    );

    let unavailable = query_missing_rows(
        &db,
        &MissingGroupKind::Unavailable {
            mount_point: "/media/B".into(),
        },
        0,
        100,
    )
    .unwrap();
    assert_eq!(
        unavailable.iter().map(|track| track.id).collect::<Vec<_>>(),
        vec![4]
    );
}

#[test]
fn unlocatable_cleanup_rechecks_current_state_and_hides_tombstones() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_missing_track(conn, 1, MissingReason::Unknown, None);
    seed_missing_track(conn, 2, MissingReason::Unmounted, None);
    let rendered_ids = vec![1, 2];

    conn.execute(
        "UPDATE tracks SET missing_since = NULL, missing_reason = NULL WHERE id = 1",
        [],
    )
    .unwrap();

    assert_eq!(
        tombstone_still_missing(&db, &MissingGroupKind::Unlocatable, &rendered_ids, 100,).unwrap(),
        vec![2]
    );
    assert_eq!(removed_at(conn, 1), None);
    assert_eq!(removed_at(conn, 2), Some(100));
    assert_eq!(count_missing(&db).unwrap(), 0);
}

#[test]
fn deleted_cleanup_never_tombstones_unlocatable_rows() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    seed_missing_track(conn, 1, MissingReason::Unknown, None);
    seed_missing_track(conn, 2, MissingReason::Unmounted, None);
    seed_missing_track(conn, 3, MissingReason::Deleted, None);

    assert_eq!(
        tombstone_still_missing(&db, &MissingGroupKind::Deleted, &[1, 2, 3], 200).unwrap(),
        vec![3]
    );
    assert_eq!(removed_at(conn, 1), None);
    assert_eq!(removed_at(conn, 2), None);
    assert_eq!(removed_at(conn, 3), Some(200));
}
