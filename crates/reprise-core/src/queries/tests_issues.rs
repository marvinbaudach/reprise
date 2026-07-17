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

/// Finding 1 (Important): `query_missing_rows` for a specific mount point
/// must return ONLY that mount's rows, never other mounts, never `unknown`
/// rows, never `deleted` rows. The per-mount branch (the `mount_filter` SQL
/// clause and 4th bound parameter in `query_missing_rows`) has no direct
/// test; seeding two distinct mount points and filtering for one must
/// return exactly one row per mount, and querying the other mount must
/// return its own different row — proving the isolation that the 18a "one
/// card per drive" feature depends on.
#[test]
fn per_mount_rows_query_isolates_to_the_requested_mount_point() {
    let conn = crate::db::open_migrated(None).unwrap();
    // Two rows on mount A
    seed_missing_track(
        &conn,
        1,
        "Alice",
        "AlbumA",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/nas-a"),
    );
    seed_missing_track(
        &conn,
        2,
        "Bob",
        "AlbumB",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/nas-a"),
    );
    // One row on mount B (different mount point)
    seed_missing_track(
        &conn,
        3,
        "Charlie",
        "AlbumC",
        Some(1),
        MissingReason::Unmounted,
        Some("/media/nas-b"),
    );
    // One `unknown` row (no mount point)
    seed_missing_track(
        &conn,
        4,
        "Dave",
        "AlbumD",
        Some(1),
        MissingReason::Unknown,
        None,
    );
    // One `deleted` row
    seed_missing_track(
        &conn,
        5,
        "Eve",
        "AlbumE",
        Some(1),
        MissingReason::Deleted,
        None,
    );

    // Query for mount A only; must return exactly the two rows on mount A
    let mount_a_rows = query_missing_rows(
        &conn,
        &MissingGroupKind::Unavailable {
            mount_point: Some("/media/nas-a".into()),
        },
        0,
        100,
    )
    .unwrap();
    assert_eq!(
        mount_a_rows.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![1, 2]
    );

    // Query for mount B; must return only the one row on mount B
    let mount_b_rows = query_missing_rows(
        &conn,
        &MissingGroupKind::Unavailable {
            mount_point: Some("/media/nas-b".into()),
        },
        0,
        100,
    )
    .unwrap();
    assert_eq!(
        mount_b_rows.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![3]
    );
}

/// Finding 2 (Minor): present tracks (missing_since IS NULL, i.e., not in
/// any missing group) must not appear in either `query_missing_groups` or
/// `query_missing_rows`. This is a sanity check on the `MISSING` predicate
/// constant being correct and consistently applied. The test seeds one
/// present track alongside missing tracks, then verifies that neither query
/// returns it.
#[test]
fn present_tracks_are_excluded_from_missing_queries() {
    let conn = crate::db::open_migrated(None).unwrap();
    // One present track (missing_since = NULL, the default)
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        rusqlite::params![
            99,
            "/music/present.flac",
            "Present Track",
            "Present",
            "Album",
            1
        ],
    )
    .unwrap();
    // One missing (deleted) track for comparison
    seed_missing_track(
        &conn,
        1,
        "Missing",
        "Album",
        Some(1),
        MissingReason::Deleted,
        None,
    );

    // `query_missing_groups` must include only the deleted row's group
    let groups = query_missing_groups(&conn).unwrap();
    assert_eq!(
        groups,
        vec![MissingGroup {
            kind: MissingGroupKind::Deleted,
            track_count: 1,
        }]
    );

    // `query_missing_rows` for Deleted must return only the missing row
    let rows = query_missing_rows(&conn, &MissingGroupKind::Deleted, 0, 100).unwrap();
    assert_eq!(rows.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1]);
}

/// Finding 3 (Minor): when there are no missing tracks at all,
/// `query_missing_groups` must return an empty `Vec`. This is the state that
/// makes the sidebar's ISSUES section disappear, so it is important to
/// pin explicitly.
#[test]
fn empty_missing_groups_when_no_missing_tracks() {
    let conn = crate::db::open_migrated(None).unwrap();
    // Insert one present track only
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        rusqlite::params![1, "/music/track.flac", "Track", "Artist", "Album", 1],
    )
    .unwrap();

    let groups = query_missing_groups(&conn).unwrap();
    assert!(groups.is_empty());
}
