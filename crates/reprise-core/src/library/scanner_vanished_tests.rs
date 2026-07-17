//! `mark_vanished_under_root`'s test suite — split from `scanner_tests.rs`
//! purely to keep that file under the project's 800-line rule. `scanner.rs`
//! declares this via `#[cfg(test)] #[path = "scanner_vanished_tests.rs"]
//! mod vanished_tests;`, so these are still crate-private scanner tests,
//! unchanged (a pure move, not a rewrite). Shared fixture helpers stay in
//! `scanner_tests.rs` and are imported from `super::tests`.

use super::tests::{fixture_copy, row_by_path};
use super::*;

// -- mark_vanished_under_root (Stage 3 Task 8 — folder watcher) ------------
//
// TDD per the task brief: these tests were written before `mark_vanished_
// under_root` existed. See that function's doc comment in `scanner.rs` for
// the component-wise (not string/LIKE) prefix check and why the watcher
// always runs this *after* an incremental `scan_folder(root)`.

/// Reads `missing_since` and reports presence via `Track::is_missing`'s own
/// rule (`Some(_)` means missing) — the direct-SQL equivalent of that
/// method, for tests that only have a bare connection and id, not a `Track`.
fn is_missing(conn: &Connection, id: i64) -> bool {
    let missing_since: Option<i64> = conn
        .query_row(
            "SELECT missing_since FROM tracks WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    missing_since.is_some()
}

#[test]
fn mark_vanished_under_root_leaves_a_present_file_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "a.flac");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    scan_folder(&mut conn, tmp.path()).unwrap();
    let (id, ..) = row_by_path(&conn, &path);

    let marked = mark_vanished_under_root(&conn, tmp.path()).unwrap();

    assert_eq!(marked, 0);
    assert!(!is_missing(&conn, id));
}

#[test]
fn mark_vanished_under_root_marks_a_deleted_file_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "a.flac");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    scan_folder(&mut conn, tmp.path()).unwrap();
    let (id, ..) = row_by_path(&conn, &path);

    std::fs::remove_file(&path).unwrap();
    let marked = mark_vanished_under_root(&conn, tmp.path()).unwrap();

    assert_eq!(marked, 1);
    assert!(is_missing(&conn, id));
}

/// A track whose path lives outside `root` must never be touched, even if
/// its own file has also vanished — that track belongs to some other
/// watcher/root (the future multi-folder-library guarantee), which is
/// responsible for marking it missing itself.
#[test]
fn mark_vanished_under_root_ignores_a_track_outside_root_even_if_its_file_is_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let watched_root = tmp.path().join("watched");
    let other_root = tmp.path().join("other");
    std::fs::create_dir(&watched_root).unwrap();
    std::fs::create_dir(&other_root).unwrap();

    let watched_path = fixture_copy(&watched_root, "in-root.flac");
    let other_path = fixture_copy(&other_root, "outside-root.flac");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    scan_folder(&mut conn, &watched_root).unwrap();
    scan_folder(&mut conn, &other_root).unwrap();
    let (watched_id, ..) = row_by_path(&conn, &watched_path);
    let (other_id, ..) = row_by_path(&conn, &other_path);

    // Both files vanish, but only `watched_root` is passed in.
    std::fs::remove_file(&watched_path).unwrap();
    std::fs::remove_file(&other_path).unwrap();

    let marked = mark_vanished_under_root(&conn, &watched_root).unwrap();

    assert_eq!(marked, 1, "only the in-root track is marked");
    assert!(is_missing(&conn, watched_id));
    assert!(
        !is_missing(&conn, other_id),
        "a track outside the watched root must never be touched"
    );
}

/// An already-missing track must not be recounted (and its `missing_since`,
/// already set, is left as-is) — the watcher only wants to know how many
/// tracks were *newly* marked on this pass.
#[test]
fn mark_vanished_under_root_does_not_recount_an_already_missing_track() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "a.flac");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    scan_folder(&mut conn, tmp.path()).unwrap();
    let (id, ..) = row_by_path(&conn, &path);

    std::fs::remove_file(&path).unwrap();
    let first = mark_vanished_under_root(&conn, tmp.path()).unwrap();
    assert_eq!(first, 1);

    // Second pass: the same track is already missing (missing_since set),
    // so it must not be counted again even though its file is still gone.
    let second = mark_vanished_under_root(&conn, tmp.path()).unwrap();
    assert_eq!(second, 0);
    assert!(is_missing(&conn, id));
}

/// Test-only: inserts a bare, non-missing track row at `path` with no audio
/// file backing it. Enough for `mark_vanished_under_root`, whose candidate
/// query and prefix/`exists()` checks only read `id`/`path` — the file never
/// having existed means `Path::exists()` is `false`, so the ONLY thing that
/// keeps such a row from being marked is the under-root membership test.
fn insert_raw_track(conn: &Connection, path: &std::path::Path) {
    conn.execute(
        "INSERT INTO tracks (path, added_at) VALUES (?1, 0)",
        [path.to_string_lossy().to_string()],
    )
    .unwrap();
}

/// Uses `queries::MISSING` directly rather than a hand-copied literal, so
/// this assertion can never silently drift from the predicate it mirrors.
fn missing_count(conn: &Connection) -> i64 {
    conn.query_row(
        &format!(
            "SELECT count(*) FROM tracks WHERE {}",
            crate::queries::MISSING
        ),
        [],
        |r| r.get(0),
    )
    .unwrap()
}

/// A reconcile of `<base>/music` must not touch a row under the sibling root
/// `<base>/music2`, which shares a bare *string* prefix but not a path
/// *component* prefix. Regression net for the SQL prefilter: its pattern is
/// `<root>/%`, so it can never match `<base>/music2/...`; the authoritative
/// component-wise `starts_with` also rejects it. Green before and after the
/// prefilter lands (this is a perf refactor, not a behavior change).
#[test]
fn mark_vanished_ignores_sibling_root_with_common_string_prefix() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("music");
    let sibling = base.path().join("music2");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();

    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    // A non-missing row whose file never existed, under the sibling root.
    insert_raw_track(&conn, &sibling.join("gone.flac"));

    let marked = mark_vanished_under_root(&conn, &root).unwrap();

    assert_eq!(marked, 0);
    assert_eq!(
        missing_count(&conn),
        0,
        "sibling-root row must not be marked"
    );
}

/// A root containing `_` (LIKE's single-char wildcard) must not widen the
/// candidate set: a reconcile of `<base>/a_b` must not match a row under
/// `<base>/axb`. Regression net that the prefilter escapes LIKE
/// metacharacters (`ESCAPE '\'`) so `_` matches only a literal underscore;
/// the component-wise `starts_with` re-filter also rejects `axb`. Green
/// before and after the prefilter lands.
#[test]
fn mark_vanished_treats_like_metacharacters_in_root_literally() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("a_b");
    let decoy = base.path().join("axb");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&decoy).unwrap();

    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    insert_raw_track(&conn, &decoy.join("gone.flac"));

    mark_vanished_under_root(&conn, &root).unwrap();

    assert_eq!(missing_count(&conn), 0);
}

/// The false-*negative* direction of the metacharacter case (the
/// data-integrity-critical one): a genuinely vanished file that really lives
/// under a root containing a LIKE metacharacter (`_`) MUST still be marked
/// missing. If the prefilter escaped the root wrongly — or not at all — the
/// escaped pattern `a\_b/%` could fail to match the row's literal
/// `<base>/a_b/gone.flac` path and silently leave a phantom track behind.
/// This complements `mark_vanished_treats_like_metacharacters_in_root_literally`
/// (which only guards the must-*not*-mark direction).
#[test]
fn mark_vanished_still_marks_in_root_file_when_root_has_like_metacharacter() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("a_b");
    std::fs::create_dir_all(&root).unwrap();

    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    // A non-missing row under the metacharacter root whose file never exists.
    let gone = root.join("gone.flac");
    insert_raw_track(&conn, &gone);
    let (id, ..) = row_by_path(&conn, &gone);

    let marked = mark_vanished_under_root(&conn, &root).unwrap();

    assert_eq!(marked, 1, "vanished in-root file must be marked missing");
    assert!(is_missing(&conn, id));
}
