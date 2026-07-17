//! Test coverage for the 18a missing-file group queries (`query_missing_
//! groups`/`query_missing_rows`, Task 2.1) — split into its own file rather
//! than `tests_maintenance.rs` (already close to the project's 800-line
//! rule) purely for size, same reasoning as `tests.rs`'s own doc comment.
//!
//! Covers exactly the four behaviors the task brief calls out: two distinct
//! mount points among `unmounted` rows become two separate `Unavailable`
//! groups; the `unknown` reason forms its own group rather than joining
//! either `unmounted` or `deleted`; the `Deleted` group's count never
//! includes `unknown` rows; and `query_missing_rows` paginates via
//! `LIMIT`/`OFFSET` in `artist, album, track_no` order.

use super::*;
use crate::models::MissingReason;

/// Inserts one missing track row with every column `query_missing_groups`/
/// `query_missing_rows` reads or groups by. `id` doubles as the row's
/// `path`/`title` disambiguator so callers can seed many rows tersely
/// without naming each field.
#[allow(clippy::too_many_arguments)]
fn seed_missing_track(
    conn: &Connection,
    id: i64,
    artist: &str,
    album: &str,
    track_no: Option<i32>,
    reason: MissingReason,
    mount_point: Option<&str>,
) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at, \
         missing_since, missing_reason, mount_point) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 1, ?7, ?8)",
        rusqlite::params![
            id,
            format!("/music/{id}.flac"),
            format!("Track {id}"),
            artist,
            album,
            track_no,
            reason.as_str(),
            mount_point,
        ],
    )
    .unwrap();
}

/// Bullet 1 of the brief: 2 mount points among `unmounted` rows must become
/// 2 separate `Unavailable` groups, never one mixed card — and they come
/// back sorted by mount point (case-insensitively), matching the 18a "one
/// card per drive" requirement.
#[test]
fn two_unmounted_drives_produce_two_unavailable_groups_sorted_by_mount() {
    let conn = crate::db::open_migrated(None).unwrap();
    seed_missing_track(
        &conn,
        1,
        "A",
        "Alpha",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/usb-b"),
    );
    seed_missing_track(
        &conn,
        2,
        "B",
        "Beta",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/usb-a"),
    );
    seed_missing_track(
        &conn,
        3,
        "C",
        "Gamma",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/usb-a"),
    );

    let groups = query_missing_groups(&conn).unwrap();

    assert_eq!(
        groups,
        vec![
            MissingGroup {
                kind: MissingGroupKind::Unavailable {
                    mount_point: Some("/media/usb-a".to_string())
                },
                track_count: 2,
            },
            MissingGroup {
                kind: MissingGroupKind::Unavailable {
                    mount_point: Some("/media/usb-b".to_string())
                },
                track_count: 1,
            },
        ]
    );
}

/// Bullet 2: `unknown` rows must never be silently folded into an
/// `unmounted` mount group or into `Deleted` — they form their own
/// `Unavailable { mount_point: None }` group, ordered after every per-mount
/// group (18a card order: per-mount, then unknown, then deleted).
#[test]
fn unknown_reason_forms_its_own_actionless_group_after_unavailable_groups() {
    let conn = crate::db::open_migrated(None).unwrap();
    seed_missing_track(
        &conn,
        1,
        "A",
        "Alpha",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/usb"),
    );
    seed_missing_track(&conn, 2, "B", "Beta", Some(1), MissingReason::Unknown, None);
    seed_missing_track(
        &conn,
        3,
        "C",
        "Gamma",
        Some(1),
        MissingReason::Unknown,
        None,
    );

    let groups = query_missing_groups(&conn).unwrap();

    assert_eq!(
        groups,
        vec![
            MissingGroup {
                kind: MissingGroupKind::Unavailable {
                    mount_point: Some("/media/usb".to_string())
                },
                track_count: 1,
            },
            MissingGroup {
                kind: MissingGroupKind::Unavailable { mount_point: None },
                track_count: 2,
            },
        ]
    );
}

/// Bullet 3, and the load-bearing invariant from Beschluss 1: the `Deleted`
/// group's count is `missing_reason = 'deleted'` ONLY. A DB with both
/// `deleted` and `unknown` rows must report `Deleted`'s count as exactly
/// the `deleted` rows — never the two reasons' combined total — since that
/// count backs a bulk hard-delete action.
#[test]
fn deleted_group_count_never_includes_unknown_reason_rows() {
    let conn = crate::db::open_migrated(None).unwrap();
    seed_missing_track(
        &conn,
        1,
        "A",
        "Alpha",
        Some(1),
        MissingReason::Deleted,
        None,
    );
    seed_missing_track(&conn, 2, "B", "Beta", Some(1), MissingReason::Unknown, None);
    seed_missing_track(
        &conn,
        3,
        "C",
        "Gamma",
        Some(1),
        MissingReason::Unknown,
        None,
    );

    let groups = query_missing_groups(&conn).unwrap();

    assert_eq!(
        groups,
        vec![
            MissingGroup {
                kind: MissingGroupKind::Unavailable { mount_point: None },
                track_count: 2,
            },
            MissingGroup {
                kind: MissingGroupKind::Deleted,
                track_count: 1,
            },
        ]
    );
}

/// Bullet 4: `query_missing_rows` pages via `LIMIT`/`OFFSET` and orders by
/// `artist, album, track_no` (`COLLATE NOCASE` on the text columns) — the
/// same shape the rest of the library sorts by. Four `deleted` rows seeded
/// out of order; two 2-row pages must reconstruct the sorted sequence.
#[test]
fn missing_rows_are_paginated_and_ordered_by_artist_album_track_no() {
    let conn = crate::db::open_migrated(None).unwrap();
    seed_missing_track(
        &conn,
        1,
        "Zeta",
        "Album",
        Some(1),
        MissingReason::Deleted,
        None,
    );
    seed_missing_track(
        &conn,
        2,
        "Alpha",
        "Second",
        Some(2),
        MissingReason::Deleted,
        None,
    );
    seed_missing_track(
        &conn,
        3,
        "Alpha",
        "First",
        Some(1),
        MissingReason::Deleted,
        None,
    );
    seed_missing_track(
        &conn,
        4,
        "alpha",
        "First",
        Some(0),
        MissingReason::Deleted,
        None,
    );

    let first_page = query_missing_rows(&conn, &MissingGroupKind::Deleted, 0, 2).unwrap();
    let second_page = query_missing_rows(&conn, &MissingGroupKind::Deleted, 2, 2).unwrap();

    assert_eq!(
        first_page.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![4, 3]
    );
    assert_eq!(
        second_page.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![2, 1]
    );
}
