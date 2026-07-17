//! Task 1.7's integration-shaped test suite: the cases that need a real
//! `scan_folder` walk (episode dedup across repeated scans, the dismiss-skip
//! fast path and its reactivate-on-change behavior, and the directory
//! `chmod` dedup case) rather than a unit-level call into `import_errors`
//! directly. Split from `scanner_tests.rs` for the usual 800-line reason —
//! `scanner.rs` declares this via `#[cfg(test)] #[path =
//! "scanner_import_errors_tests.rs"] mod import_errors_tests;`, so these are
//! still crate-private scanner tests. The three purely unit-level cases
//! (`clear_error`'s return value, `classify_lofty`'s mapping) live in
//! `import_errors_tests.rs` instead, next to the code they test directly.

use rusqlite::OptionalExtension;

use super::tests::{completed, fixture_copy};
use super::*;

fn broken_mp3(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("kaputt.mp3");
    std::fs::write(&path, b"not audio").unwrap();
    path
}

/// `(first_seen, last_seen, seen_count, dismissed_mtime, dismissed_size)`.
type ImportErrorRowTuple = (i64, i64, i64, Option<i64>, Option<i64>);

fn import_error_row(conn: &Connection, path: &str) -> Option<ImportErrorRowTuple> {
    conn.query_row(
        "SELECT first_seen, last_seen, seen_count, dismissed_mtime, dismissed_size \
         FROM import_errors WHERE path = ?1",
        [path],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )
    .optional()
    .unwrap()
}

/// Brief case 1: five scans of the same permanently-broken file must
/// converge on exactly ONE `import_errors` row, not one per scan — the
/// episode upsert (`record_error`) replacing the old DELETE-then-INSERT
/// pair. `seen_count` must land on `5`; `first_seen` must stay pinned to the
/// first failure; `last_seen` must have actually moved past it (a forced
/// clock tick before the final scan, so this doesn't rely on the whole loop
/// spanning a wall-clock second boundary on its own).
#[test]
fn repeated_scans_of_same_broken_file_produce_one_episode_row() {
    let tmp = tempfile::tempdir().unwrap();
    let path = broken_mp3(tmp.path());
    let path_str = path.to_string_lossy().to_string();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    for _ in 0..4 {
        completed(scan_folder(&mut conn, tmp.path()).unwrap());
    }
    let (first_seen_after_four, _, seen_count_after_four, _, _) =
        import_error_row(&conn, &path_str).unwrap();
    assert_eq!(seen_count_after_four, 4);

    std::thread::sleep(std::time::Duration::from_millis(1100));
    completed(scan_folder(&mut conn, tmp.path()).unwrap());

    let total_rows: i64 = conn
        .query_row("SELECT count(*) FROM import_errors", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total_rows, 1, "must converge on ONE row, not one per scan");

    let (first_seen, last_seen, seen_count, _, _) = import_error_row(&conn, &path_str).unwrap();
    assert_eq!(
        seen_count, 5,
        "5 failing scans must produce seen_count == 5"
    );
    assert_eq!(
        first_seen, first_seen_after_four,
        "first_seen must stay stable across repeated failures"
    );
    assert!(
        last_seen > first_seen,
        "last_seen must advance on a later scan, unlike first_seen"
    );
}

/// Brief case 3: a dismissed row whose file is UNCHANGED must be skipped
/// before `read_meta` ever runs — proven with `track_meta::READ_META_CALLS`, not just an
/// assertion on the row (which would also pass if the parse ran and simply
/// didn't change anything). `seen_count` must not bump either: a skip is not
/// a failed attempt.
#[test]
fn dismissed_unchanged_file_is_skipped_without_reading_tags() {
    let tmp = tempfile::tempdir().unwrap();
    let path = broken_mp3(tmp.path());
    let path_str = path.to_string_lossy().to_string();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    completed(scan_folder(&mut conn, tmp.path()).unwrap());

    let stat = std::fs::metadata(&path).unwrap();
    let mtime = stat
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let size = stat.len() as i64;
    conn.execute(
        "UPDATE import_errors SET dismissed_mtime = ?2, dismissed_size = ?3 WHERE path = ?1",
        rusqlite::params![path_str, mtime, size],
    )
    .unwrap();
    let (_, _, seen_count_before, _, _) = import_error_row(&conn, &path_str).unwrap();

    track_meta::READ_META_CALLS.with(|calls| calls.set(0));
    completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(
        track_meta::READ_META_CALLS.with(std::cell::Cell::get),
        0,
        "a dismissed, unchanged file must never reach read_meta"
    );

    let (_, _, seen_count_after, dismissed_mtime, dismissed_size) =
        import_error_row(&conn, &path_str).unwrap();
    assert_eq!(
        seen_count_after, seen_count_before,
        "a skipped scan must not bump seen_count"
    );
    assert_eq!(dismissed_mtime, Some(mtime));
    assert_eq!(dismissed_size, Some(size));
}

/// Brief case 4: a dismissed row whose file's `mtime`/`size` no longer match
/// what was recorded at dismissal time means the file genuinely changed —
/// `check_dismissed` must clear `dismissed_*` and start a fresh episode
/// (`first_seen == now`, `seen_count` reset to `0` at reactivation), which
/// the ensuing failed re-parse then takes to `seen_count == 1`.
#[test]
fn dismissed_file_with_changed_mtime_starts_a_new_episode() {
    let tmp = tempfile::tempdir().unwrap();
    let path = broken_mp3(tmp.path());
    let path_str = path.to_string_lossy().to_string();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    completed(scan_folder(&mut conn, tmp.path()).unwrap());

    // Dismiss against a fingerprint the file will no longer match once
    // rewritten below — an impossible mtime/size pair is fine here, since
    // all that matters is that it differs from the file's real stat.
    conn.execute(
        "UPDATE import_errors SET dismissed_mtime = 1, dismissed_size = 1 WHERE path = ?1",
        [&path_str],
    )
    .unwrap();

    // Change the file's actual size (and, incidentally, its mtime) so it no
    // longer matches the dismissed fingerprint above.
    std::fs::write(&path, b"still not audio, but longer now").unwrap();

    let before = now_unix();
    completed(scan_folder(&mut conn, tmp.path()).unwrap());
    let after = now_unix();

    let (first_seen, _last_seen, seen_count, dismissed_mtime, dismissed_size) =
        import_error_row(&conn, &path_str).unwrap();
    assert!(dismissed_mtime.is_none(), "dismissed_mtime must be cleared");
    assert!(dismissed_size.is_none(), "dismissed_size must be cleared");
    assert!(
        (before..=after).contains(&first_seen),
        "first_seen must be refreshed to the reactivation scan's time"
    );
    assert_eq!(
        seen_count, 1,
        "seen_count resets to 0 at reactivation, then the failed re-parse takes it to 1"
    );
}

/// Brief case 5: a directory walkdir cannot enter must produce exactly ONE
/// `import_errors` row keyed by the DIRECTORY's own path — not the numeric
/// index of the walk error, the pre-Task-1.7 bug this fixes — classified as
/// `PermissionDenied`. Skipped when `chmod 000` doesn't actually block reads
/// (e.g. running as root in a container), same guard as this file's sibling
/// traversal test in `scanner_tests.rs`.
#[cfg(unix)]
#[test]
fn locked_directory_produces_one_row_with_directory_path_and_permission_denied() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    fixture_copy(&locked, "unreachable.flac");

    let mut perms = std::fs::metadata(&locked).unwrap().permissions();
    perms.set_mode(0o000);
    std::fs::set_permissions(&locked, perms).unwrap();

    if std::fs::read_dir(&locked).is_ok() {
        let mut restore = std::fs::metadata(&locked).unwrap().permissions();
        restore.set_mode(0o755);
        std::fs::set_permissions(&locked, restore).unwrap();
        eprintln!(
            "skipping locked_directory_produces_one_row_with_directory_path_and_permission_denied: \
             directory permissions are not enforced in this environment (likely running as root)"
        );
        return;
    }

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    let scan_result = scan_folder(&mut conn, tmp.path());

    // Always restore permissions before asserting, so tempdir cleanup
    // succeeds even if the assertions below fail.
    let mut restore = std::fs::metadata(&locked).unwrap().permissions();
    restore.set_mode(0o755);
    std::fs::set_permissions(&locked, restore).unwrap();

    completed(scan_result.unwrap());

    let locked_str = locked.to_string_lossy().to_string();
    let reason_kind: String = conn
        .query_row(
            "SELECT reason_kind FROM import_errors WHERE path = ?1",
            [&locked_str],
            |r| r.get(0),
        )
        .expect("row must be keyed by the DIRECTORY's own path, not a synthetic index");
    assert_eq!(
        ImportErrorKind::parse(&reason_kind),
        ImportErrorKind::PermissionDenied
    );

    let total_rows: i64 = conn
        .query_row(
            "SELECT count(*) FROM import_errors WHERE path = ?1",
            [&locked_str],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(total_rows, 1);
}
