//! Task 1.9's test suite: tombstone resurrection (the evidence rule,
//! Beschluss 7/12) across all three arms — fast-path restore, the ON
//! CONFLICT upsert, and move detection's `apply_file_identity` — plus
//! `ScanReport::healed`. Split into its own file purely to keep `scanner_
//! tests.rs` itself under the project's 800-line rule, same rationale as
//! every other `_tests.rs` sibling here — `scanner.rs` declares this via
//! `#[cfg(test)] #[path = "scanner_tombstone_tests.rs"] mod tombstone_
//! tests;`, so this file's contents are still the crate-private `crate::
//! library::scanner::tombstone_tests` module.

use super::tests::{completed, fixture_copy, row_by_path, row_count, tag_file};
use super::*;

/// Test-only: `removed_at` of the row at `path` — Task 1.9's tombstone
/// column, `None` once a scan has resurrected it.
fn removed_at_of(conn: &Connection, path: &std::path::Path) -> Option<i64> {
    conn.query_row(
        "SELECT removed_at FROM tracks WHERE path = ?1",
        [path.to_string_lossy().to_string()],
        |r| r.get(0),
    )
    .unwrap()
}

/// Task 1.9 / evidence rule (Beschluss 7/12): a tombstoned row (`removed_at`
/// set — the future "Remove from library" marker; nothing sets it yet, so
/// this test sets it directly via SQL) whose file is found at its exact
/// recorded path with an UNCHANGED mtime must resurrect through the
/// fast-path-restore branch, exactly like a merely-`missing` row already
/// does. The object the removal targeted has provably come back — a scan
/// that finds it right where it always was outranks whatever "Remove" once
/// decided, so the user simply sees the track again. `rating`/`play_count`
/// must survive, same guarantee as every other restore path.
#[test]
fn tombstoned_row_resurrects_on_fast_path_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let file = fixture_copy(tmp.path(), "track.flac");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r1.added, 1);

    let path_str = file.to_string_lossy().to_string();
    let (id_before, ..) = row_by_path(&conn, &file);
    conn.execute(
        "UPDATE tracks SET removed_at = 1, rating = 4, play_count = 7 WHERE path = ?1",
        [&path_str],
    )
    .unwrap();

    // The file itself is untouched on disk: same path, same mtime — the
    // fast path must take the restore branch on `removed_at` alone, since
    // `missing_since` was never set here.
    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert!(
        r2.updated >= 1,
        "resurrecting a tombstoned row must count as an update"
    );

    assert_eq!(
        removed_at_of(&conn, &file),
        None,
        "removed_at must be cleared once the file is proven present again"
    );
    let (id_after, rating, play_count, _) = row_by_path(&conn, &file);
    assert_eq!(id_after, id_before, "resurrect must reuse the same row/id");
    assert_eq!(rating, 4, "rating must survive a resurrect");
    assert_eq!(play_count, 7, "play_count must survive a resurrect");
}

/// Same evidence rule as above, but through the ON CONFLICT upsert arm: the
/// file's mtime is forced to look changed (so the fast path is skipped and
/// the file is genuinely re-read), while the row stays at the SAME path —
/// this is the "content changed while tombstoned" case, distinct from both
/// the unchanged-mtime fast path and a cross-path move.
#[test]
fn tombstoned_row_resurrects_on_upsert_when_mtime_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let file = fixture_copy(tmp.path(), "track.flac");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r1.added, 1);

    let path_str = file.to_string_lossy().to_string();
    let (id_before, ..) = row_by_path(&conn, &file);
    conn.execute(
        "UPDATE tracks SET removed_at = 1, rating = 4, play_count = 7, file_mtime = 0 \
         WHERE path = ?1",
        [&path_str],
    )
    .unwrap();

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r2.updated, 1, "same path re-read must count as an update");

    assert_eq!(
        removed_at_of(&conn, &file),
        None,
        "removed_at must be cleared by the upsert arm too"
    );
    let (id_after, rating, play_count, _) = row_by_path(&conn, &file);
    assert_eq!(id_after, id_before, "resurrect must reuse the same row/id");
    assert_eq!(rating, 4, "rating must survive an upsert resurrect");
    assert_eq!(play_count, 7, "play_count must survive an upsert resurrect");
}

/// Same evidence rule through the move-detection arm's `apply_file_identity`:
/// a tombstoned row whose file gets renamed (same device/inode, unknown new
/// path) must resurrect exactly like an ordinary move, AND — this is also
/// the explicit lock for "relinked with ratings" (Abnahme) the brief calls
/// for — `rating`/`play_count` must survive untouched, since `apply_file_
/// identity` deliberately never sets them.
#[test]
fn tombstoned_row_resurrects_via_move_with_rating_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let old_path = fixture_copy(tmp.path(), "track.flac");
    tag_file(&old_path, "Tombstoned Song", "Some Artist", "Some Album");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r1.added, 1);

    conn.execute(
        "UPDATE tracks SET removed_at = 1, rating = 5, play_count = 9 WHERE path = ?1",
        [old_path.to_string_lossy().to_string()],
    )
    .unwrap();
    let (old_id, ..) = row_by_path(&conn, &old_path);

    let new_dir = tmp.path().join("new_subdir");
    std::fs::create_dir(&new_dir).unwrap();
    let new_path = new_dir.join("track.flac");
    std::fs::rename(&old_path, &new_path).unwrap();

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r2.moved, 1);
    assert_eq!(r2.added, 0);
    assert_eq!(row_count(&conn), 1);

    assert_eq!(
        removed_at_of(&conn, &new_path),
        None,
        "removed_at must be cleared by the move arm's apply_file_identity"
    );
    let (new_id, rating, play_count, _) = row_by_path(&conn, &new_path);
    assert_eq!(new_id, old_id, "resurrect-via-move must reuse the same id");
    assert_eq!(rating, 5, "rating must survive a resurrect-via-move");
    assert_eq!(
        play_count, 9,
        "play_count must survive a resurrect-via-move"
    );
}

/// `ScanReport::healed` counts an `import_errors` row actually deleted by a
/// pass-1 import SUCCESS — not every scan, and not every `clear_error` call
/// vacuously returning `false` when there was nothing to clear. Mirrors
/// `fixing_a_broken_file_clears_its_import_error`'s scenario but asserts the
/// counter itself: `healed == 1` on the fixing scan, then `healed == 0` on a
/// later no-op rescan, since there is no error row left to delete a second
/// time.
#[test]
fn healed_counts_error_row_cleared_by_pass1_success() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("flaky.flac");
    std::fs::write(&path, b"not audio").unwrap();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r1.errors, 1);
    assert_eq!(r1.healed, 0, "a fresh error is not a healing");

    // "Repair" the file: overwrite the SAME path with valid audio content.
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    std::fs::copy(&src, &path).unwrap();

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r2.added, 1);
    assert_eq!(
        r2.healed, 1,
        "the fix must be counted exactly once, from clear_error's own true return"
    );

    // Nothing changed since: the fast path skips it, and even if it didn't,
    // there is no error row left to delete.
    let r3 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r3.healed, 0, "no error row remains to heal a second time");
}
