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
