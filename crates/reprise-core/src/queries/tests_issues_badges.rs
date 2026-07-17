//! Test coverage for Task 2.5's Missing-files sidebar badge counts
//! (`count_missing`/`count_new_missing`, `issues.rs`). Split out of
//! `tests_issues.rs` rather than appended to it — that file was already at
//! 712 lines (Task 2.1/2.2's own coverage) and this section's doc-comment
//! density would have pushed it past the project's 800-line rule; the
//! import-error half of this same task (`count_import_errors_active`/
//! `count_new_import_errors`) lives in `tests_import_errors.rs` instead,
//! right next to the hint machinery it must not double-count, for the same
//! "split by what's cohesive, not what fits" reasoning.

use super::*;

/// Inserts one missing track row with an explicit `missing_since`, for tests
/// that need to control the badge's "before/after `last_viewed`" boundary
/// precisely — unlike `tests_issues.rs`'s own `seed_missing_track`, which
/// hardcodes `missing_since = 1` because none of its callers care about the
/// exact value.
fn seed_missing_track_since(conn: &Connection, id: i64, missing_since: i64) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at, \
         missing_since, missing_reason) \
         VALUES (?1, ?2, ?3, 'Artist', 'Album', 1, 0, ?4, 'deleted')",
        rusqlite::params![
            id,
            format!("/music/{id}.flac"),
            format!("Track {id}"),
            missing_since
        ],
    )
    .unwrap();
}

/// A tombstoned (`removed_at` set) missing row — must never count toward
/// either `count_missing` or `count_new_missing`: the user already asked for
/// this row to be gone (Task 2.2's 10-second-undo remove), so it is neither
/// a visibility trigger for the ISSUES section nor a badge-worthy "new"
/// event.
fn seed_tombstoned_missing_track(conn: &Connection, id: i64, missing_since: i64, removed_at: i64) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at, \
         missing_since, missing_reason, removed_at) \
         VALUES (?1, ?2, ?3, 'Artist', 'Album', 1, 0, ?4, 'deleted', ?5)",
        rusqlite::params![
            id,
            format!("/music/{id}.flac"),
            format!("Track {id}"),
            missing_since,
            removed_at
        ],
    )
    .unwrap();
}

/// An ordinary, non-missing track row — the "must never count" control case
/// both tests below use.
fn seed_present_track(conn: &Connection, id: i64) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at) \
         VALUES (?1, ?2, ?3, 'Artist', 'Album', 1, 0)",
        rusqlite::params![id, format!("/music/{id}.flac"), format!("Track {id}")],
    )
    .unwrap();
}

/// `count_missing` is the ISSUES-section-visibility total, not the badge —
/// it must count every `MISSING` row regardless of how old or new it is,
/// while still excluding a tombstoned row (the user already dismissed that
/// one by removing it).
#[test]
fn count_missing_counts_every_missing_row_old_and_new_alike() {
    let conn = crate::db::open_migrated(None).unwrap();
    seed_missing_track_since(&conn, 1, 10); // old
    seed_missing_track_since(&conn, 2, 500); // new
    seed_tombstoned_missing_track(&conn, 3, 500, 600); // must not count
    seed_present_track(&conn, 4); // present, must not count

    assert_eq!(count_missing(&conn).unwrap(), 2);
}

#[test]
fn count_missing_is_zero_when_nothing_is_missing() {
    let conn = crate::db::open_migrated(None).unwrap();
    seed_present_track(&conn, 1);
    assert_eq!(count_missing(&conn).unwrap(), 0);
}

/// The badge itself: only rows that went missing strictly AFTER
/// `last_viewed` count as new — a row that was already missing the last
/// time the user opened the view must not keep re-badging it forever (see
/// `issues.rs`'s doc comment on why the badge counts "new since last view",
/// not the backlog total).
#[test]
fn count_new_missing_counts_only_rows_missing_since_after_last_viewed() {
    let conn = crate::db::open_migrated(None).unwrap();
    seed_missing_track_since(&conn, 1, 50); // before last_viewed — not new
    seed_missing_track_since(&conn, 2, 150); // after last_viewed — new
    seed_missing_track_since(&conn, 3, 200); // after last_viewed — new

    assert_eq!(count_new_missing(&conn, 100).unwrap(), 2);
}

/// Exact boundary: `missing_since == last_viewed` must NOT count as new —
/// the comparison is strictly `>`, matching a row the user's last view
/// would already have shown (a view recorded at exactly that second already
/// covers a row that went missing in that same second).
#[test]
fn count_new_missing_boundary_equal_to_last_viewed_does_not_count() {
    let conn = crate::db::open_migrated(None).unwrap();
    seed_missing_track_since(&conn, 1, 100);
    assert_eq!(count_new_missing(&conn, 100).unwrap(), 0);
}

/// `last_viewed = 0` is the "never viewed" sentinel (a missing settings key
/// reads back as `0` — see `library::settings::get_last_viewed_missing`'s
/// doc comment): with no prior view to compare against, every missing row
/// is new.
#[test]
fn count_new_missing_zero_last_viewed_treats_every_missing_row_as_new() {
    let conn = crate::db::open_migrated(None).unwrap();
    seed_missing_track_since(&conn, 1, 1);
    seed_missing_track_since(&conn, 2, 100_000);
    assert_eq!(count_new_missing(&conn, 0).unwrap(), 2);
}

/// A tombstoned row must never badge, even when its `missing_since` is well
/// after `last_viewed` — same reasoning as `count_missing`'s tombstone
/// exclusion above.
#[test]
fn count_new_missing_excludes_tombstoned_rows() {
    let conn = crate::db::open_migrated(None).unwrap();
    seed_tombstoned_missing_track(&conn, 1, 500, 600);
    assert_eq!(count_new_missing(&conn, 100).unwrap(), 0);
}
