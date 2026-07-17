//! Vanish-marking test suite — split from `scanner_tests.rs` purely to keep
//! that file under the project's 800-line rule. `scanner.rs` declares this
//! via `#[cfg(test)] #[path = "scanner_vanished_tests.rs"] mod
//! vanished_tests;`, so these are still crate-private scanner tests.
//!
//! Task 1.5 folded the formerly-standalone `mark_vanished_under_root` into
//! `scan_folder` itself (see `scan_folder_inner`'s doc comment in
//! `scanner.rs`), so every test here that used to call `mark_vanished_under_
//! root(&conn, root)` as a second, separate step now calls `scan_folder(&mut
//! conn, root)` alone — the same call that walked the directory also
//! reconciles what it did NOT find. TDD per the task brief: the six tests
//! below `## Task 1.5 brief cases` were written before the fold/guard
//! existed, in the order the brief specifies.

use super::tests::{completed, fixture_copy, row_by_path};
use super::*;

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

fn missing_reason(conn: &Connection, id: i64) -> Option<String> {
    conn.query_row(
        "SELECT missing_reason FROM tracks WHERE id = ?1",
        [id],
        |r| r.get(0),
    )
    .unwrap()
}

/// `st_dev` of `path` (which must exist), via `lstat` — mirrors `mounts.rs`'
/// own test helper of the same shape, for tests that need to fabricate a
/// stored `device` value that either matches or deliberately doesn't match
/// a real directory's current device.
fn dev_of(path: &std::path::Path) -> i64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).unwrap().dev() as i64
}

/// Test-only: inserts a bare, non-missing track row at `path` with no audio
/// file backing it and no recorded `device` (`NULL`) — enough for the
/// candidate query and `starts_with`/`exists()` checks, which is all that
/// matters for tests that expect the row to fall OUTSIDE the scanned root
/// (the root guard's device-evidence check never runs on a candidate list
/// that's empty in the first place).
fn insert_raw_track(conn: &Connection, path: &std::path::Path) {
    conn.execute(
        "INSERT INTO tracks (path, added_at) VALUES (?1, 0)",
        [path.to_string_lossy().to_string()],
    )
    .unwrap();
}

/// Test-only: like [`insert_raw_track`], but with an explicit `device` —
/// needed by any test where the row is expected to be a genuine root-guard
/// candidate (i.e. it DOES fall under the scanned root and the walk finds
/// nothing else), since a `NULL`-device row can never satisfy the guard's
/// "some candidate confirms the root's device" evidence check on its own.
fn insert_raw_track_with_device(conn: &Connection, path: &std::path::Path, device: i64) {
    conn.execute(
        "INSERT INTO tracks (path, added_at, device) VALUES (?1, 0, ?2)",
        rusqlite::params![path.to_string_lossy().to_string(), device],
    )
    .unwrap();
}

/// Unwraps a `ScanOutcome` expected to be `RootUnavailable`, returning the
/// reported root — panics with the report otherwise.
fn root_unavailable(outcome: ScanOutcome) -> std::path::PathBuf {
    match outcome {
        ScanOutcome::RootUnavailable { root } => root,
        ScanOutcome::Completed(report) => {
            panic!("expected ScanOutcome::RootUnavailable, got Completed({report:?})")
        }
    }
}

// -- Pre-fold regression coverage, adapted to call scan_folder directly ----
//
// These pin the same behaviors the pre-1.5 standalone `mark_vanished_under_
// root` had its own test suite for; only the call shape changed (a single
// `scan_folder` now does what used to take two calls).

#[test]
fn scan_folder_leaves_a_present_file_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "a.flac");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    let report = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    let (id, ..) = row_by_path(&conn, &path);

    assert_eq!(report.vanished, 0);
    assert!(!is_missing(&conn, id));
}

/// A track whose path lives outside `root` must never be touched, even if
/// its own file has also vanished — that track belongs to some other
/// scan/watch root (the future multi-folder-library guarantee), which is
/// responsible for marking it missing itself.
#[test]
fn scan_folder_ignores_a_track_outside_root_even_if_its_file_is_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let watched_root = tmp.path().join("watched");
    let other_root = tmp.path().join("other");
    std::fs::create_dir(&watched_root).unwrap();
    std::fs::create_dir(&other_root).unwrap();

    let watched_path = fixture_copy(&watched_root, "in-root.flac");
    let other_path = fixture_copy(&other_root, "outside-root.flac");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    completed(scan_folder(&mut conn, &watched_root).unwrap());
    completed(scan_folder(&mut conn, &other_root).unwrap());
    let (watched_id, ..) = row_by_path(&conn, &watched_path);
    let (other_id, ..) = row_by_path(&conn, &other_path);

    // Both files vanish, but only `watched_root` is scanned again.
    std::fs::remove_file(&watched_path).unwrap();
    std::fs::remove_file(&other_path).unwrap();

    let report = completed(scan_folder(&mut conn, &watched_root).unwrap());

    assert_eq!(report.vanished, 1, "only the in-root track is marked");
    assert!(is_missing(&conn, watched_id));
    assert!(
        !is_missing(&conn, other_id),
        "a track outside the watched root must never be touched"
    );
}

/// An already-missing track must not be recounted (and its `missing_since`,
/// already set, is left as-is) — a scan only wants to know how many tracks
/// were *newly* marked on this pass.
#[test]
fn scan_folder_does_not_recount_an_already_missing_track() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "a.flac");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    completed(scan_folder(&mut conn, tmp.path()).unwrap());
    let (id, ..) = row_by_path(&conn, &path);

    std::fs::remove_file(&path).unwrap();
    let first = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(first.vanished, 1);

    // Second scan: the same track is already missing (missing_since set),
    // so it must not be counted again even though its file is still gone.
    let second = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(second.vanished, 0);
    assert!(is_missing(&conn, id));
}

/// A reconcile of `<base>/music` must not touch a row under the sibling root
/// `<base>/music2`, which shares a bare *string* prefix but not a path
/// *component* prefix. Regression net for the SQL prefilter: its pattern is
/// `<root>/%`, so it can never match `<base>/music2/...`; the authoritative
/// component-wise `starts_with` also rejects it.
#[test]
fn scan_folder_ignores_sibling_root_with_common_string_prefix() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("music");
    let sibling = base.path().join("music2");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();

    let conn_holder = crate::db::open(None).unwrap();
    crate::db::migrate(&conn_holder).unwrap();
    let mut conn = conn_holder;
    // A non-missing row whose file never existed, under the sibling root.
    insert_raw_track(&conn, &sibling.join("gone.flac"));

    let report = completed(scan_folder(&mut conn, &root).unwrap());

    assert_eq!(report.vanished, 0);
    assert_eq!(
        missing_count(&conn),
        0,
        "sibling-root row must not be marked"
    );
}

/// A root containing `_` (LIKE's single-char wildcard) must not widen the
/// candidate set: a scan of `<base>/a_b` must not match a row under
/// `<base>/axb`. Regression net that the prefilter escapes LIKE
/// metacharacters (`ESCAPE '\'`) so `_` matches only a literal underscore;
/// the component-wise `starts_with` re-filter also rejects `axb`.
#[test]
fn scan_folder_treats_like_metacharacters_in_root_literally() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("a_b");
    let decoy = base.path().join("axb");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&decoy).unwrap();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    insert_raw_track(&conn, &decoy.join("gone.flac"));

    completed(scan_folder(&mut conn, &root).unwrap());

    assert_eq!(missing_count(&conn), 0);
}

/// The false-*negative* direction of the metacharacter case (the
/// data-integrity-critical one): a genuinely vanished file that really lives
/// under a root containing a LIKE metacharacter (`_`) MUST still be marked
/// missing. The row is seeded with `root`'s own real device so the root
/// guard's evidence check (see `## Task 1.5 brief cases` below) sees proof
/// the root is reachable and doesn't suppress marking — that guard is an
/// orthogonal concern from what this test actually pins (LIKE-escaping),
/// and giving it a real device keeps the two from interfering.
#[test]
fn scan_folder_still_marks_in_root_file_when_root_has_like_metacharacter() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("a_b");
    std::fs::create_dir_all(&root).unwrap();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    // A non-missing row under the metacharacter root whose file never
    // exists, with `root`'s real device so the root guard sees proof.
    let gone = root.join("gone.flac");
    insert_raw_track_with_device(&conn, &gone, dev_of(&root));
    let (id, ..) = row_by_path(&conn, &gone);

    let report = completed(scan_folder(&mut conn, &root).unwrap());

    assert_eq!(
        report.vanished, 1,
        "vanished in-root file must be marked missing"
    );
    assert!(is_missing(&conn, id));
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

// -- Task 1.5 brief cases ----------------------------------------------
//
// The six cases the task brief specifies, in the brief's own order. See
// `scan_folder_inner`'s doc comment in `scanner.rs` for the fold and root
// guard these exercise.

/// Brief case 1 ("Faltung"): deleting a file and calling `scan_folder` once
/// — no separate mark call — must set `missing_since`, classify the reason
/// as `deleted` (the root's device matches, proving a real deletion, not an
/// absent mount), and report it in `ScanReport::vanished`.
#[test]
fn scan_folder_folds_a_deleted_file_into_the_same_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "a.flac");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    completed(scan_folder(&mut conn, tmp.path()).unwrap());
    let (id, ..) = row_by_path(&conn, &path);

    std::fs::remove_file(&path).unwrap();
    let report = completed(scan_folder(&mut conn, tmp.path()).unwrap());

    assert_eq!(report.vanished, 1);
    assert!(is_missing(&conn, id));
    assert_eq!(missing_reason(&conn, id).as_deref(), Some("deleted"));
}

/// Brief case 2 (atomicity/ordering): a move and an unrelated deletion
/// discovered in the SAME scan must reconcile correctly — the moved file's
/// row must never transiently look missing, because move detection (during
/// the walk) and the mark phase (right after it) now share one transaction.
/// This is exactly the ordering the old two-call design needed three
/// paragraphs of doc-comment discipline to get right; folding makes it
/// structurally impossible to get wrong.
#[test]
fn scan_folder_move_and_delete_in_the_same_scan_never_marks_the_moved_row_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let moved_path = fixture_copy(tmp.path(), "moved.flac");
    let gone_path = fixture_copy(tmp.path(), "gone.flac");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r1.added, 2);
    let (moved_id, ..) = row_by_path(&conn, &moved_path);
    let (gone_id, ..) = row_by_path(&conn, &gone_path);

    let new_dir = tmp.path().join("new_subdir");
    std::fs::create_dir(&new_dir).unwrap();
    let new_path = new_dir.join("moved.flac");
    std::fs::rename(&moved_path, &new_path).unwrap();
    std::fs::remove_file(&gone_path).unwrap();

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());

    assert_eq!(r2.moved, 1, "the renamed file must be recognized as a move");
    assert_eq!(r2.vanished, 1, "only the genuinely deleted file is marked");
    assert!(
        !is_missing(&conn, moved_id),
        "the moved row must never be marked missing"
    );
    assert!(is_missing(&conn, gone_id));
    let (still_moved_id, ..) = row_by_path(&conn, &new_path);
    assert_eq!(still_moved_id, moved_id, "the moved row kept its identity");
}

/// Brief case 3 (Root-Guard a): a root that doesn't exist on disk at all
/// must short-circuit to `RootUnavailable` before any walk and without
/// touching the database — no track gets marked, and no `import_errors` row
/// is written for the root itself.
#[test]
fn scan_folder_root_guard_a_nonexistent_root_reports_root_unavailable() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("does-not-exist");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    // A present row the DB believes lives under `root`, seeded directly
    // (never actually scanned, since `root` never existed) so the test can
    // prove the guard leaves it completely untouched.
    let phantom = root.join("phantom.flac");
    insert_raw_track(&conn, &phantom);
    let (id, ..) = row_by_path(&conn, &phantom);

    let outcome = scan_folder(&mut conn, &root).unwrap();

    assert_eq!(root_unavailable(outcome), root);
    assert!(
        !is_missing(&conn, id),
        "root-guard case (a) must mark nothing"
    );
    let import_error_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM import_errors WHERE path = ?1",
            [root.to_string_lossy().to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        import_error_count, 0,
        "a nonexistent root must never itself become an import_errors row"
    );
}

/// Brief case 4 (Root-Guard b): the root directory exists but is empty, and
/// every present track the DB expects under it has a recorded `device` that
/// does NOT match the root's real, current device (fabricated as `real_dev +
/// 99_999`, mirroring `mounts.rs`'s own test convention for a guaranteed
/// non-collision). No evidence the root is reachable → `RootUnavailable`,
/// nothing marked.
#[test]
fn scan_folder_root_guard_b_empty_root_with_mismatched_device_reports_root_unavailable() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let real_dev = dev_of(&root);
    let phantom = root.join("phantom.flac");
    insert_raw_track_with_device(&conn, &phantom, real_dev + 99_999);
    let (id, ..) = row_by_path(&conn, &phantom);

    let outcome = scan_folder(&mut conn, &root).unwrap();

    assert_eq!(root_unavailable(outcome), root);
    assert!(
        !is_missing(&conn, id),
        "root-guard case (b) must mark nothing even though the walk found nothing"
    );
}

/// Fix-pass regression (widened root-guard evidence): the guard must still
/// trip when EVERY candidate row under `root` is already flagged missing
/// (`missing_since` set) and none of their recorded devices match the root's
/// real, current device. Before the fix, the guard reused the `PRESENT`-only
/// mark-phase candidate list for its own evidence check — a `missing_since
/// IS NOT NULL` row is excluded from `PRESENT`, so an all-already-missing
/// root produced an empty candidate list, `!candidates.is_empty()` was
/// false, and the guard silently never tripped: a scan of a root whose mount
/// point now has a *different* filesystem swapped underneath it reported
/// `Completed`/`vanished == 0` instead of `RootUnavailable`, hiding the
/// "your library folder is unreachable" signal the guard exists to surface.
#[test]
fn scan_folder_root_guard_widened_evidence_trips_when_only_already_missing_rows_remain() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let real_dev = dev_of(&root);
    let phantom = root.join("phantom.flac");
    insert_raw_track_with_device(&conn, &phantom, real_dev + 99_999);
    let (id, ..) = row_by_path(&conn, &phantom);
    // Already flagged missing by some earlier reconcile — excluded from the
    // `PRESENT`-filtered mark-phase candidate list, but must still count as
    // root-guard evidence: an already-missing, not-yet-tombstoned row is
    // real proof this root once had tracks, and its mismatching device is
    // real proof the mount underneath it has since changed.
    conn.execute(
        "UPDATE tracks SET missing_since = 1, missing_reason = 'deleted' WHERE id = ?1",
        [id],
    )
    .unwrap();

    let outcome = scan_folder(&mut conn, &root).unwrap();

    assert_eq!(root_unavailable(outcome), root);
}

/// Brief case 5 (Root-Guard c): the root directory exists but is empty, and
/// the present track the DB expects under it has a recorded `device` that
/// DOES match the root's real, current device — proof the root's own
/// filesystem is genuinely reachable, so this is a real, provable deletion.
/// `Completed`, and the track is marked `deleted`.
#[test]
fn scan_folder_root_guard_c_empty_root_with_matching_device_marks_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let real_dev = dev_of(&root);
    let phantom = root.join("phantom.flac");
    insert_raw_track_with_device(&conn, &phantom, real_dev);
    let (id, ..) = row_by_path(&conn, &phantom);

    let report = completed(scan_folder(&mut conn, &root).unwrap());

    assert_eq!(report.vanished, 1);
    assert!(is_missing(&conn, id));
    assert_eq!(missing_reason(&conn, id).as_deref(), Some("deleted"));
}

/// Brief case 6 (single-file Retry no-op guarantee): `scan_folder` called
/// with a *file* path (the import-errors panel's "Retry" — see `ui::
/// import_errors_view`) must complete normally with `vanished == 0`. The
/// mark phase's `LIKE '<file>/%'` prefilter can never match the file's own
/// literal path (only paths genuinely nested *under* it), so this falls out
/// of the existing fold with no special case, exactly as the task brief
/// requires.
#[test]
fn scan_folder_on_a_single_file_root_is_a_vanish_mark_no_op() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fixture_copy(tmp.path(), "a.flac");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    completed(scan_folder(&mut conn, tmp.path()).unwrap());

    let report = completed(scan_folder(&mut conn, &path).unwrap());

    assert_eq!(report.vanished, 0);
    assert_eq!(
        report.skipped_unchanged, 1,
        "the file itself was rescanned, unchanged"
    );
}
