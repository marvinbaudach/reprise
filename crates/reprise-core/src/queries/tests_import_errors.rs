//! Tests for Task 2.4's grouped import-error read/write queries
//! (`import_errors.rs`). Split into its own file rather than folded into
//! `tests_maintenance.rs` (already at ~708 lines) — same reasoning as
//! `tests_issues.rs`'s own module doc comment: a cohesive unit of coverage
//! for one cohesive unit of production code.

use super::*;
use crate::models::ImportErrorKind;

/// Inserts one `import_errors` row with an explicit `reason_kind` string
/// (not `ImportErrorKind::as_str()` — tests intentionally use the same
/// literal strings the scanner would write, so a drift between the
/// production `as_str()` mapping and what's actually stored would surface
/// here too).
fn insert_error(
    conn: &rusqlite::Connection,
    path: &str,
    reason_kind: &str,
    first_seen: i64,
    last_seen: i64,
) {
    conn.execute(
        "INSERT INTO import_errors (path, reason_kind, reason_detail, first_seen, last_seen) \
         VALUES (?1, ?2, 'boom', ?3, ?4)",
        rusqlite::params![path, reason_kind, first_seen, last_seen],
    )
    .unwrap();
}

fn insert_dismissed_error(
    conn: &rusqlite::Connection,
    path: &str,
    reason_kind: &str,
    first_seen: i64,
    last_seen: i64,
    dismissed_mtime: i64,
    dismissed_size: i64,
) {
    conn.execute(
        "INSERT INTO import_errors \
         (path, reason_kind, reason_detail, first_seen, last_seen, dismissed_mtime, dismissed_size) \
         VALUES (?1, ?2, 'boom', ?3, ?4, ?5, ?6)",
        rusqlite::params![path, reason_kind, first_seen, last_seen, dismissed_mtime, dismissed_size],
    )
    .unwrap();
}

/// A present (`PRESENT`), untagged track at `path` — the exact shape
/// `is_hint`'s `EXISTS` looks for (see the brief's "hint contract").
fn insert_untagged_present_track(conn: &rusqlite::Connection, path: &str) {
    conn.execute(
        "INSERT INTO tracks (path, title, artist, added_at, untagged) \
         VALUES (?1, 'stem', '', 0, 1)",
        rusqlite::params![path],
    )
    .unwrap();
}

#[test]
fn query_import_errors_grouped_orders_groups_by_kind_declaration_order() {
    let conn = crate::db::open_migrated(None).unwrap();
    // Inserted out of declaration order on purpose, so the result can only
    // be right if the query itself imposes the order, not insertion order.
    insert_error(&conn, "/x/io.flac", "io", 100, 100);
    insert_error(&conn, "/x/unknown.flac", "unknown", 100, 100);
    insert_error(&conn, "/x/tags.flac", "unreadable_tags", 100, 100);
    insert_error(&conn, "/x/perm.flac", "permission_denied", 100, 100);
    insert_error(&conn, "/x/fmt.flac", "unsupported_format", 100, 100);

    let groups = query_import_errors_grouped(&conn).unwrap();
    let kinds: Vec<ImportErrorKind> = groups.iter().map(|(kind, _)| *kind).collect();
    assert_eq!(
        kinds,
        vec![
            ImportErrorKind::UnreadableTags,
            ImportErrorKind::PermissionDenied,
            ImportErrorKind::UnsupportedFormat,
            ImportErrorKind::Io,
            ImportErrorKind::Unknown,
        ]
    );
    for (_, entries) in &groups {
        assert_eq!(entries.len(), 1);
    }
}

#[test]
fn query_import_errors_grouped_orders_rows_within_a_group_by_last_seen_desc_then_path() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/b.flac", "io", 100, 200);
    insert_error(&conn, "/x/a.flac", "io", 100, 200);
    insert_error(&conn, "/x/older.flac", "io", 100, 50);

    let groups = query_import_errors_grouped(&conn).unwrap();
    assert_eq!(groups.len(), 1);
    let (kind, entries) = &groups[0];
    assert_eq!(*kind, ImportErrorKind::Io);
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    // Same last_seen (200) sorts by path first; the older row (50) sorts last.
    assert_eq!(paths, vec!["/x/a.flac", "/x/b.flac", "/x/older.flac"]);
}

#[test]
fn query_import_errors_grouped_a_kind_with_no_rows_is_absent_not_empty() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/a.flac", "io", 100, 100);

    let groups = query_import_errors_grouped(&conn).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, ImportErrorKind::Io);
}

#[test]
fn query_import_errors_grouped_empty_table_returns_empty_vec() {
    let conn = crate::db::open_migrated(None).unwrap();
    assert!(query_import_errors_grouped(&conn).unwrap().is_empty());
}

#[test]
fn is_hint_true_only_for_a_present_untagged_track_at_the_same_path() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/hint.flac", "unreadable_tags", 100, 100);
    insert_untagged_present_track(&conn, "/x/hint.flac");
    insert_error(&conn, "/x/no-track.flac", "unreadable_tags", 100, 100);

    let groups = query_import_errors_grouped(&conn).unwrap();
    let (_, entries) = &groups[0];
    let hint = entries.iter().find(|e| e.path == "/x/hint.flac").unwrap();
    let no_track = entries
        .iter()
        .find(|e| e.path == "/x/no-track.flac")
        .unwrap();
    assert!(hint.is_hint);
    assert!(!no_track.is_hint);
}

#[test]
fn is_hint_false_for_a_tagged_track_at_the_same_path() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/tagged.flac", "unreadable_tags", 100, 100);
    conn.execute(
        "INSERT INTO tracks (path, title, artist, added_at, untagged) \
         VALUES ('/x/tagged.flac', 'Real Title', 'Real Artist', 0, 0)",
        [],
    )
    .unwrap();

    let groups = query_import_errors_grouped(&conn).unwrap();
    assert!(!groups[0].1[0].is_hint);
}

#[test]
fn is_hint_false_for_an_untagged_track_that_is_itself_missing() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/gone.flac", "unreadable_tags", 100, 100);
    conn.execute(
        "INSERT INTO tracks (path, title, artist, added_at, untagged, missing_since, missing_reason) \
         VALUES ('/x/gone.flac', 'stem', '', 0, 1, 100, 'deleted')",
        [],
    )
    .unwrap();

    let groups = query_import_errors_grouped(&conn).unwrap();
    assert!(
        !groups[0].1[0].is_hint,
        "a missing track's untagged row must not count as a live hint"
    );
}

#[test]
fn query_import_errors_grouped_excludes_dismissed_rows() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/active.flac", "io", 100, 100);
    insert_dismissed_error(&conn, "/x/dismissed.flac", "io", 100, 100, 111, 222);

    let groups = query_import_errors_grouped(&conn).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].1.len(), 1);
    assert_eq!(groups[0].1[0].path, "/x/active.flac");
}

#[test]
fn query_dismissed_import_errors_returns_only_dismissed_rows() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/active.flac", "io", 100, 100);
    insert_dismissed_error(&conn, "/x/dismissed-a.flac", "io", 100, 200, 111, 222);
    insert_dismissed_error(
        &conn,
        "/x/dismissed-b.flac",
        "unreadable_tags",
        100,
        300,
        111,
        222,
    );

    let dismissed = query_dismissed_import_errors(&conn).unwrap();
    let paths: Vec<&str> = dismissed.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, vec!["/x/dismissed-b.flac", "/x/dismissed-a.flac"]);
}

#[test]
fn query_dismissed_import_errors_empty_when_nothing_dismissed() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/active.flac", "io", 100, 100);
    assert!(query_dismissed_import_errors(&conn).unwrap().is_empty());
}

#[test]
fn count_dismissed_import_errors_counts_only_dismissed_rows() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/active.flac", "io", 100, 100);
    insert_dismissed_error(&conn, "/x/dismissed-a.flac", "io", 100, 100, 1, 2);
    insert_dismissed_error(&conn, "/x/dismissed-b.flac", "io", 100, 100, 1, 2);

    assert_eq!(count_dismissed_import_errors(&conn).unwrap(), 2);
}

#[test]
fn dismiss_import_error_moves_a_row_from_active_to_dismissed() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/a.flac", "io", 100, 100);

    dismiss_import_error(&conn, "/x/a.flac", 555, 4096).unwrap();

    assert!(query_import_errors_grouped(&conn).unwrap().is_empty());
    let dismissed = query_dismissed_import_errors(&conn).unwrap();
    assert_eq!(dismissed.len(), 1);
    assert_eq!(dismissed[0].path, "/x/a.flac");
}

#[test]
fn dismiss_import_error_on_an_unknown_path_is_a_noop() {
    let conn = crate::db::open_migrated(None).unwrap();
    // No row at all — must not error.
    dismiss_import_error(&conn, "/x/never-existed.flac", 1, 2).unwrap();
    assert_eq!(count_dismissed_import_errors(&conn).unwrap(), 0);
}

#[test]
fn restore_import_error_nulls_only_the_dismissed_columns() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/a.flac", "io", 100, 200);
    dismiss_import_error(&conn, "/x/a.flac", 555, 4096).unwrap();

    restore_import_error(&conn, "/x/a.flac").unwrap();

    assert_eq!(count_dismissed_import_errors(&conn).unwrap(), 0);
    let groups = query_import_errors_grouped(&conn).unwrap();
    assert_eq!(groups.len(), 1);
    let entry = &groups[0].1[0];
    assert_eq!(entry.path, "/x/a.flac");
    // Restore must not touch the episode's own history — only the
    // dismissed_* pair is nulled, per the brief's restore contract.
    assert_eq!(entry.first_seen, 100);
    assert_eq!(entry.last_seen, 200);
}

#[test]
fn restore_import_error_on_an_unknown_path_is_a_noop() {
    let conn = crate::db::open_migrated(None).unwrap();
    restore_import_error(&conn, "/x/never-existed.flac").unwrap();
}

#[test]
fn dismiss_all_import_errors_dismisses_every_active_row_it_can_stat() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/a.flac", "io", 100, 100);
    insert_error(&conn, "/x/b.flac", "unreadable_tags", 100, 100);

    let dismissed_count = dismiss_all_import_errors(&conn, &|_path| Some((123, 456))).unwrap();

    assert_eq!(dismissed_count, 2);
    assert!(query_import_errors_grouped(&conn).unwrap().is_empty());
    assert_eq!(count_dismissed_import_errors(&conn).unwrap(), 2);
}

#[test]
fn dismiss_all_import_errors_skips_a_path_that_fails_to_stat() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/present.flac", "io", 100, 100);
    insert_error(&conn, "/x/vanished.flac", "io", 100, 100);

    let dismissed_count = dismiss_all_import_errors(&conn, &|path| {
        if path == "/x/vanished.flac" {
            None
        } else {
            Some((123, 456))
        }
    })
    .unwrap();

    // Only the statable row counts as dismissed...
    assert_eq!(dismissed_count, 1);
    assert_eq!(count_dismissed_import_errors(&conn).unwrap(), 1);
    // ...and the un-statable row is left exactly as active as it was before
    // the call, not silently dismissed with bogus/NULL stat values.
    let groups = query_import_errors_grouped(&conn).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].1[0].path, "/x/vanished.flac");
}

#[test]
fn dismiss_all_import_errors_leaves_already_dismissed_rows_untouched() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_dismissed_error(&conn, "/x/a.flac", "io", 100, 100, 1, 2);

    let stat_calls = std::cell::RefCell::new(Vec::new());
    let dismissed_count = dismiss_all_import_errors(&conn, &|path| {
        stat_calls.borrow_mut().push(path.to_string());
        Some((999, 999))
    })
    .unwrap();

    assert_eq!(dismissed_count, 0);
    assert!(
        stat_calls.borrow().is_empty(),
        "an already-dismissed row must not even be stat-ed again"
    );
    // Original stat fingerprint is preserved, not overwritten.
    let dismissed = query_dismissed_import_errors(&conn).unwrap();
    assert_eq!(dismissed.len(), 1);
}

#[test]
fn dismiss_all_import_errors_is_a_noop_on_an_empty_table() {
    let conn = crate::db::open_migrated(None).unwrap();
    assert_eq!(
        dismiss_all_import_errors(&conn, &|_| Some((1, 2))).unwrap(),
        0
    );
}

// -- Badge counts (Task 2.5) -------------------------------------------------
//
// `count_import_errors_active` decides whether the sidebar's "Import
// errors" ISSUES row exists at all, so it counts every non-dismissed row —
// hints included, because a hint row must stay reachable for the user to
// act on (add real tags) even though it must never badge. `count_new_
// import_errors` is the badge itself: non-dismissed, non-hint rows whose
// `first_seen` is strictly after `last_viewed`.

#[test]
fn count_import_errors_active_includes_hints_but_excludes_dismissed() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/plain.flac", "io", 100, 100);
    insert_error(&conn, "/x/hint.flac", "unreadable_tags", 100, 100);
    insert_untagged_present_track(&conn, "/x/hint.flac");
    insert_dismissed_error(&conn, "/x/dismissed.flac", "io", 100, 100, 1, 2);

    // The View-visibility count includes the hint (the row must stay
    // reachable) but not the dismissed row.
    assert_eq!(count_import_errors_active(&conn).unwrap(), 2);
}

#[test]
fn count_import_errors_active_is_zero_on_an_empty_table() {
    let conn = crate::db::open_migrated(None).unwrap();
    assert_eq!(count_import_errors_active(&conn).unwrap(), 0);
}

#[test]
fn count_new_import_errors_counts_only_first_seen_after_last_viewed() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/old.flac", "io", 50, 50); // before last_viewed
    insert_error(&conn, "/x/new-a.flac", "io", 150, 150); // after
    insert_error(&conn, "/x/new-b.flac", "unreadable_tags", 200, 200); // after

    assert_eq!(count_new_import_errors(&conn, 100).unwrap(), 2);
}

#[test]
fn count_new_import_errors_boundary_equal_to_last_viewed_does_not_count() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/a.flac", "io", 100, 100);
    assert_eq!(count_new_import_errors(&conn, 100).unwrap(), 0);
}

/// A hint must NEVER badge, even when it is freshly seen — the app already
/// solved this file (stem-derived title, `tracks.untagged = 1`); it is
/// asking for tags, not for help. `count_import_errors_active` (the row-
/// visibility count) includes it; the badge must not.
#[test]
fn count_new_import_errors_excludes_hints_even_when_freshly_seen() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/hint.flac", "unreadable_tags", 200, 200);
    insert_untagged_present_track(&conn, "/x/hint.flac");
    insert_error(&conn, "/x/real.flac", "unreadable_tags", 200, 200);

    assert_eq!(
        count_new_import_errors(&conn, 100).unwrap(),
        1,
        "only the non-hint row may badge"
    );
}

#[test]
fn count_new_import_errors_excludes_dismissed_rows_even_when_freshly_seen() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_dismissed_error(&conn, "/x/dismissed.flac", "io", 200, 200, 1, 2);
    assert_eq!(count_new_import_errors(&conn, 100).unwrap(), 0);
}

#[test]
fn count_new_import_errors_zero_last_viewed_treats_every_active_non_hint_row_as_new() {
    let conn = crate::db::open_migrated(None).unwrap();
    insert_error(&conn, "/x/a.flac", "io", 1, 1);
    insert_error(&conn, "/x/b.flac", "io", 100_000, 100_000);
    assert_eq!(count_new_import_errors(&conn, 0).unwrap(), 2);
}

/// The subtlest interaction in the badge design (see the task brief): when
/// a DISMISSED row's underlying file changes on disk, `library::import_
/// errors::check_dismissed` starts a NEW episode — it nulls `dismissed_*`
/// and resets `first_seen = now`/`seen_count = 0` (see that function's own
/// doc comment). This test simulates exactly that reactivation via direct
/// SQL (mirroring `check_dismissed`'s own `UPDATE`, not calling it — that
/// function lives outside this task's file ownership and takes a
/// `Transaction`, which would be a heavier, less direct fixture for what is
/// purely a `count_new_import_errors` query test) to prove the badge query
/// gets the "changed file re-badges" behavior for free, with no special
/// casing: a reactivated row's `first_seen` is fresh, so a `last_viewed`
/// from BEFORE the reactivation (even one that is AFTER the original,
/// dismissed episode's `first_seen`) must still count it as new.
#[test]
fn count_new_import_errors_recounts_a_reactivated_episode_as_new() {
    let conn = crate::db::open_migrated(None).unwrap();
    // Original episode: seen and dismissed long ago.
    insert_dismissed_error(&conn, "/x/changed.flac", "io", 10, 10, 111, 222);
    // The user last viewed the Import errors list well after that original
    // episode — a naive `first_seen > last_viewed` on the STALE first_seen
    // (10) would correctly stay silent here, but the row is about to become
    // new again for a different reason.
    let last_viewed = 500;

    // The file changed on disk; the scanner's `check_dismissed` reactivates
    // the episode: clears the dismissal, restarts `first_seen`/`seen_count`.
    conn.execute(
        "UPDATE import_errors SET dismissed_mtime = NULL, dismissed_size = NULL, \
         first_seen = ?2, seen_count = 0 WHERE path = ?1",
        rusqlite::params!["/x/changed.flac", 900],
    )
    .unwrap();

    assert_eq!(
        count_new_import_errors(&conn, last_viewed).unwrap(),
        1,
        "a reactivated episode (fresh first_seen) must badge again, even \
         though the user viewed the list after the ORIGINAL episode started"
    );
}
