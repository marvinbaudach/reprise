//! `scanner.rs`'s test suite, split into its own file in Stage 3 Task 1
//! purely to keep `scanner.rs` itself under the project's 800-line rule —
//! `scanner.rs` declares this via `#[cfg(test)] #[path = "scanner_tests.rs"]
//! mod tests;`, so this file's contents are still the crate-private
//! `crate::library::scanner::tests` module, with the exact same tests,
//! unchanged, that used to live inline (a pure move, not a rewrite).

use super::*;
use lofty::prelude::*;
use lofty::tag::{Tag, TagType};

fn fixture_copy(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let dst = dir.join(name);
    std::fs::copy(&src, &dst).unwrap();
    dst
}

/// Writes identical title/artist/album tags to a fixture file so two
/// separate copies produce the same fingerprint (title+artist+album+
/// duration+file_size) that move detection's step 2 matches on.
fn tag_file(path: &std::path::Path, title: &str, artist: &str, album: &str) {
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.into());
    tag.set_artist(artist.into());
    tag.set_album(album.into());
    tag.save_to_path(path, lofty::config::WriteOptions::default())
        .unwrap();
}

fn row_by_path(conn: &Connection, path: &std::path::Path) -> (i64, i64, i64, i64) {
    // (id, rating, play_count, added_at)
    conn.query_row(
        "SELECT id, rating, play_count, added_at FROM tracks WHERE path = ?1",
        [path.to_string_lossy().to_string()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .unwrap()
}

fn row_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
        .unwrap()
}

/// `file_stat` must degrade to `None` (not panic, and not fabricate
/// placeholder zeros) when the path doesn't exist — Stage 3 Task 1's
/// `Option` return type exists specifically so `scan_folder` can skip
/// move detection outright in that case rather than fingerprinting
/// against a `(0, 0)` device/inode that could coincidentally match an
/// unrelated row.
#[test]
fn file_stat_returns_none_for_a_path_that_does_not_exist() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist.flac");
    assert_eq!(file_stat(&missing), None);
}

/// `std::fs::rename` on the same filesystem keeps the file's inode (and
/// device) identical while its path becomes unknown to the DB — this
/// exercises move detection's step 1 (device/inode candidate). Note:
/// rename does NOT change mtime, but that's irrelevant here — the new
/// path has no DB row at all, so it enters the unknown-path flow
/// regardless of mtime, exactly like a genuinely new file would.
#[test]
fn move_via_rename_preserves_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let old_path = fixture_copy(tmp.path(), "track.flac");
    tag_file(&old_path, "Moved Song", "Some Artist", "Some Album");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r1.added, 1);

    conn.execute(
        "UPDATE tracks SET rating = 5, play_count = 7 WHERE path = ?1",
        [old_path.to_string_lossy().to_string()],
    )
    .unwrap();
    let (old_id, _, _, added_at_before) = row_by_path(&conn, &old_path);

    let new_dir = tmp.path().join("new_subdir");
    std::fs::create_dir(&new_dir).unwrap();
    let new_path = new_dir.join("track.flac");
    std::fs::rename(&old_path, &new_path).unwrap();

    let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r2.moved, 1);
    assert_eq!(r2.added, 0);
    assert_eq!(row_count(&conn), 1);

    let (new_id, rating, play_count, added_at_after) = row_by_path(&conn, &new_path);
    assert_eq!(new_id, old_id);
    assert_eq!(rating, 5);
    assert_eq!(play_count, 7);
    assert_eq!(added_at_after, added_at_before);
}

/// Copy + delete-original simulates a cross-filesystem move: the copy
/// gets a brand-new inode, so step 1 (device/inode) cannot match and move
/// detection must fall through to step 2 (tag+duration+size fingerprint).
/// Both copies share identical tags and byte-for-byte content (same
/// fixture file), so size and duration line up exactly; only the
/// original's deletion makes its DB row a valid ("path gone") candidate.
#[test]
fn move_via_copy_delete_preserves_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let old_path = fixture_copy(tmp.path(), "track.flac");
    tag_file(&old_path, "Copied Song", "Some Artist", "Some Album");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r1.added, 1);

    conn.execute(
        "UPDATE tracks SET rating = 5, play_count = 7 WHERE path = ?1",
        [old_path.to_string_lossy().to_string()],
    )
    .unwrap();
    let (old_id, _, _, added_at_before) = row_by_path(&conn, &old_path);

    let new_dir = tmp.path().join("new_subdir");
    std::fs::create_dir(&new_dir).unwrap();
    let new_path = new_dir.join("track.flac");
    std::fs::copy(&old_path, &new_path).unwrap();
    std::fs::remove_file(&old_path).unwrap();

    let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r2.moved, 1);
    assert_eq!(r2.added, 0);
    assert_eq!(row_count(&conn), 1);

    let (new_id, rating, play_count, added_at_after) = row_by_path(&conn, &new_path);
    assert_eq!(new_id, old_id);
    assert_eq!(rating, 5);
    assert_eq!(play_count, 7);
    assert_eq!(added_at_after, added_at_before);
}

/// Two identical copies (same fingerprint) both get deleted, then a
/// single new copy appears at a new location. Both stale rows'
/// paths are gone, so both are valid fingerprint candidates — move
/// detection must refuse to guess between them (logs a warning) and
/// fall back to a normal insert instead of silently attaching the new
/// file's history to an arbitrary one of the two old rows.
#[test]
fn ambiguous_duplicates_are_not_guessed() {
    let tmp = tempfile::tempdir().unwrap();
    let path_a = fixture_copy(tmp.path(), "a.flac");
    tag_file(&path_a, "Duplicate Song", "Some Artist", "Some Album");
    let path_b = fixture_copy(tmp.path(), "b.flac");
    tag_file(&path_b, "Duplicate Song", "Some Artist", "Some Album");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r1.added, 2);
    assert_eq!(row_count(&conn), 2);

    std::fs::remove_file(&path_a).unwrap();
    std::fs::remove_file(&path_b).unwrap();
    let new_dir = tmp.path().join("new_subdir");
    std::fs::create_dir(&new_dir).unwrap();
    let new_path = new_dir.join("c.flac");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac"),
        &new_path,
    )
    .unwrap();
    tag_file(&new_path, "Duplicate Song", "Some Artist", "Some Album");

    let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r2.moved, 0);
    assert_eq!(r2.added, 1);
    // Both stale rows remain untouched (missing-marking is a Stage 3/
    // watcher concern, out of scope here) plus the one newly added row.
    assert_eq!(row_count(&conn), 3);
}

/// Pins the candidate filter's order of operations: the "path gone or
/// missing" validity check must run BEFORE candidates are counted for
/// ambiguity, not after. Two identical copies exist; only ONE is
/// deleted. A new copy shows up at a new location. Naively counting all
/// fingerprint-matching rows first would see 2 candidates and refuse to
/// guess — but the survivor's path still exists on disk, so it is never
/// a *valid* candidate, leaving exactly one true match (the deleted
/// row), and the move must proceed.
#[test]
fn one_deleted_one_alive_duplicate_is_still_an_unambiguous_move() {
    let tmp = tempfile::tempdir().unwrap();
    let path_a = fixture_copy(tmp.path(), "a.flac");
    tag_file(&path_a, "Half Deleted Song", "Some Artist", "Some Album");
    let path_b = fixture_copy(tmp.path(), "b.flac");
    tag_file(&path_b, "Half Deleted Song", "Some Artist", "Some Album");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r1.added, 2);

    conn.execute(
        "UPDATE tracks SET rating = 3, play_count = 2 WHERE path = ?1",
        [path_b.to_string_lossy().to_string()],
    )
    .unwrap();
    let (survivor_id, survivor_rating, survivor_play_count, survivor_added_at) =
        row_by_path(&conn, &path_b);

    // Only path_a is deleted; path_b (the survivor) is left in place.
    std::fs::remove_file(&path_a).unwrap();
    let new_dir = tmp.path().join("new_subdir");
    std::fs::create_dir(&new_dir).unwrap();
    let new_path = new_dir.join("c.flac");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac"),
        &new_path,
    )
    .unwrap();
    tag_file(&new_path, "Half Deleted Song", "Some Artist", "Some Album");

    let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r2.moved, 1);
    assert_eq!(r2.added, 0);
    assert_eq!(row_count(&conn), 2);

    // The survivor (path_b) must be completely untouched.
    let (still_survivor_id, still_rating, still_play_count, still_added_at) =
        row_by_path(&conn, &path_b);
    assert_eq!(still_survivor_id, survivor_id);
    assert_eq!(still_rating, survivor_rating);
    assert_eq!(still_play_count, survivor_play_count);
    assert_eq!(still_added_at, survivor_added_at);
}

/// Two rows with identical (device, inode) both have paths that no longer
/// exist on disk (via direct SQL mutation), making both valid candidates
/// for device/inode-based move detection. When find_move_candidate is
/// called directly with those fake device/inode values, it must refuse
/// to guess and return Ok(None), leaving both rows untouched. Pins the
/// device/inode ambiguity branch (line ~148-155 in find_move_candidate).
#[test]
fn ambiguous_device_inode_candidates_are_not_guessed() {
    let tmp = tempfile::tempdir().unwrap();
    let path_a = fixture_copy(tmp.path(), "a.flac");
    tag_file(&path_a, "Dev Inode Ambiguity", "Some Artist", "Some Album");
    let path_b = fixture_copy(tmp.path(), "b.flac");
    tag_file(&path_b, "Dev Inode Ambiguity", "Some Artist", "Some Album");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r1.added, 2);

    // Manually set both rows to identical fake device/inode and non-existent paths
    // so they both become valid candidates (path no longer exists).
    conn.execute(
        "UPDATE tracks SET device = 7777, inode = 8888, path = '/gone/a.flac' WHERE id = 1",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE tracks SET device = 7777, inode = 8888, path = '/gone/b.flac' WHERE id = 2",
        [],
    )
    .unwrap();

    // Get the fixture file_size for the lookup (inode won't match real file).
    let (file_size, _, _) = file_stat(&path_a).expect("fixture file must stat successfully");

    // Call find_move_candidate directly with the fake device/inode to hit the
    // ambiguity branch. Both rows match device=7777, inode=8888, so both are
    // initially selected by the SQL query, then filtered to validity (both paths
    // gone). With 2 valid candidates, the function must warn and return Ok(None).
    let tx = conn.transaction().unwrap();
    let lookup = MoveLookup {
        device: 7777,
        inode: 8888,
        title: "Dev Inode Ambiguity",
        artist: "Some Artist",
        album: "Some Album",
        duration_ms: 1000,
        file_size: file_size as i64,
    };
    let result = find_move_candidate(&tx, &lookup).unwrap();
    assert_eq!(
        result, None,
        "must return None when device/inode candidates are ambiguous"
    );

    // Verify both rows are still untouched (not guessed).
    let path_1: String = tx
        .query_row("SELECT path FROM tracks WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    let path_2: String = tx
        .query_row("SELECT path FROM tracks WHERE id = 2", [], |r| r.get(0))
        .unwrap();
    assert_eq!(path_1, "/gone/a.flac");
    assert_eq!(path_2, "/gone/b.flac");
}

/// A rescan over an untouched library must not match anything as moved —
/// pins that move detection only ever engages for genuinely unknown
/// paths, never for files whose path/mtime already match a known row.
#[test]
fn unchanged_files_are_not_matched_as_moves() {
    let tmp = tempfile::tempdir().unwrap();
    fixture_copy(tmp.path(), "a.flac");
    fixture_copy(tmp.path(), "b.flac");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r1.added, 2);

    let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r2.moved, 0);
    assert_eq!(r2.skipped_unchanged, 2);
    assert_eq!(r2.added, 0);
}

/// Writes tags to a fixture COPY (never the original) and reads them back with
/// read_meta — the roundtrip from the spec.
#[test]
fn read_meta_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let file = fixture_copy(tmp.path(), "tagged.flac");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title("Beast of Darkness".into());
    tag.set_artist("Brand of Sacrifice".into());
    tag.set_album("God Hand".into());
    tag.set_year(2019);
    tag.set_track(9);
    tag.set_genre("Deathcore".into());
    tag.save_to_path(&file, lofty::config::WriteOptions::default())
        .unwrap();

    let meta = read_meta(&file).unwrap();
    assert_eq!(meta.title, "Beast of Darkness");
    assert_eq!(meta.artist, "Brand of Sacrifice");
    assert_eq!(meta.album, "God Hand");
    assert_eq!(meta.year, Some(2019));
    assert_eq!(meta.track_no, Some(9));
    assert!(meta.duration_ms > 0);
}

#[test]
fn scan_adds_updates_and_reports_errors() {
    let tmp = tempfile::tempdir().unwrap();
    fixture_copy(tmp.path(), "a.flac");
    fixture_copy(tmp.path(), "b.flac");
    // broken "audio" file → import_errors
    std::fs::write(tmp.path().join("kaputt.mp3"), b"not audio").unwrap();
    // non-audio is ignored
    std::fs::write(tmp.path().join("cover.jpg"), b"jpg").unwrap();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!((r1.added, r1.errors), (2, 1));

    // second scan: nothing changed → everything skipped
    let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r2.skipped_unchanged, 2);
    assert_eq!(r2.added, 0);

    let errs: i64 = conn
        .query_row("SELECT count(*) FROM import_errors", [], |r| r.get(0))
        .unwrap();
    assert_eq!(errs, 1);
}

/// A file that fails to import once and is later repaired (same path, valid
/// content) must not leave a stale row behind in `import_errors`.
#[test]
fn fixing_a_broken_file_clears_its_import_error() {
    let tmp = tempfile::tempdir().unwrap();
    // lofty::read_from_path determines the file type from the extension, so the
    // broken and repaired content must both use a ".flac" path for the repair
    // to actually parse.
    let path = tmp.path().join("flaky.flac");
    std::fs::write(&path, b"not audio").unwrap();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r1.errors, 1);

    let path_str = path.to_string_lossy().to_string();
    let errs_after_break: i64 = conn
        .query_row(
            "SELECT count(*) FROM import_errors WHERE path = ?1",
            [&path_str],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(errs_after_break, 1);

    // "Repair" the file: overwrite the SAME path with valid audio content.
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    std::fs::copy(&src, &path).unwrap();

    let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r2.added, 1);

    let errs_after_fix: i64 = conn
        .query_row(
            "SELECT count(*) FROM import_errors WHERE path = ?1",
            [&path_str],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(errs_after_fix, 0);
}

/// Rescanning a track whose file was modified must update tag-derived
/// columns while preserving user data (rating, play_count, added_at).
#[test]
fn rescan_preserves_rating_play_count_and_added_at_on_update() {
    let tmp = tempfile::tempdir().unwrap();
    let file = fixture_copy(tmp.path(), "track.flac");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title("Original Title".into());
    tag.save_to_path(&file, lofty::config::WriteOptions::default())
        .unwrap();

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r1.added, 1);

    let path_str = file.to_string_lossy().to_string();
    // Simulate user data plus a "changed file" without depending on filesystem
    // mtime granularity: force file_mtime to 0 so the next scan re-reads it.
    conn.execute(
        "UPDATE tracks SET rating = 4, play_count = 7, file_mtime = 0 WHERE path = ?1",
        [&path_str],
    )
    .unwrap();
    let added_at_before: i64 = conn
        .query_row(
            "SELECT added_at FROM tracks WHERE path = ?1",
            [&path_str],
            |r| r.get(0),
        )
        .unwrap();

    // Re-tag the same file with a different title.
    let mut tag2 = Tag::new(TagType::VorbisComments);
    tag2.set_title("New Title".into());
    tag2.save_to_path(&file, lofty::config::WriteOptions::default())
        .unwrap();

    let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r2.updated, 1);

    let (title, rating, play_count, added_at): (String, i64, i64, i64) = conn
        .query_row(
            "SELECT title, rating, play_count, added_at FROM tracks WHERE path = ?1",
            [&path_str],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(title, "New Title");
    assert_eq!(rating, 4);
    assert_eq!(play_count, 7);
    assert_eq!(added_at, added_at_before);
}

/// A track previously marked `missing` (e.g. by a prior scan that found
/// its file gone) that reappears at its *exact recorded path* with an
/// *unchanged* mtime (a NAS remount, a restore-from-trash) must have its
/// `missing` flag cleared on the next scan. Before this test's fix, the
/// incremental fast path (`known_mtime == Some(mtime)`) only checked the
/// mtime and skipped the row entirely, silently ignoring `missing` — so
/// a restored file stayed invisible/flagged forever even though it was
/// right back where the scanner expected it. rating/play_count must
/// survive untouched, exactly like the ordinary update path does.
#[test]
fn restored_file_at_same_path_clears_missing_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let file = fixture_copy(tmp.path(), "track.flac");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert_eq!(r1.added, 1);

    let path_str = file.to_string_lossy().to_string();
    conn.execute(
        "UPDATE tracks SET missing = 1, rating = 4, play_count = 7 WHERE path = ?1",
        [&path_str],
    )
    .unwrap();
    tx_insert_import_error(&conn, &path_str);

    // The file itself is untouched on disk: same path, same mtime — this
    // is exactly the "reappeared unchanged" scenario (NAS remount,
    // restore-from-trash), not a content change.
    let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
    assert!(
        r2.updated >= 1,
        "restoring a missing track must count as an update"
    );

    let (missing, rating, play_count): (i64, i64, i64) = conn
        .query_row(
            "SELECT missing, rating, play_count FROM tracks WHERE path = ?1",
            [&path_str],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(missing, 0, "missing flag must be cleared on restore");
    assert_eq!(rating, 4, "rating must survive a restore");
    assert_eq!(play_count, 7, "play_count must survive a restore");

    let errs: i64 = conn
        .query_row(
            "SELECT count(*) FROM import_errors WHERE path = ?1",
            [&path_str],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        errs, 0,
        "any stale import_errors row for the restored path must be cleared too"
    );
}

/// Test-only helper: inserts a stale `import_errors` row for `path`, so
/// `restored_file_at_same_path_clears_missing_flag` can assert the
/// restore path clears it (mirroring the real-world case where a file
/// briefly failed to import before going missing and later being
/// restored).
fn tx_insert_import_error(conn: &Connection, path: &str) {
    conn.execute(
        "INSERT INTO import_errors (path, reason, occurred_at) VALUES (?1, 'stale', 0)",
        [path],
    )
    .unwrap();
}

/// walkdir traversal errors (e.g. a permission-denied subdirectory) must be
/// recorded in `import_errors` and counted, never silently dropped. This test
/// is skipped when running with elevated privileges (e.g. root in a
/// container), where `chmod 000` does not actually block directory reads.
#[cfg(unix)]
#[test]
fn traversal_error_in_unreadable_dir_is_recorded_not_dropped() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    fixture_copy(tmp.path(), "readable.flac");
    let locked = tmp.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    fixture_copy(&locked, "unreachable.flac");

    let mut perms = std::fs::metadata(&locked).unwrap().permissions();
    perms.set_mode(0o000);
    std::fs::set_permissions(&locked, perms).unwrap();

    if std::fs::read_dir(&locked).is_ok() {
        // Permissions did not actually block reads (e.g. running as root).
        let mut restore = std::fs::metadata(&locked).unwrap().permissions();
        restore.set_mode(0o755);
        std::fs::set_permissions(&locked, restore).unwrap();
        eprintln!(
            "skipping traversal_error_in_unreadable_dir_is_recorded_not_dropped: \
             directory permissions are not enforced in this environment"
        );
        return;
    }

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    let scan_result = scan_folder(&mut conn, tmp.path());

    // Always restore permissions before asserting, so tempdir cleanup succeeds
    // even if the assertions below fail.
    let mut restore = std::fs::metadata(&locked).unwrap().permissions();
    restore.set_mode(0o755);
    std::fs::set_permissions(&locked, restore).unwrap();

    let report = scan_result.unwrap();
    assert!(report.errors >= 1, "expected traversal error to be counted");
    assert_eq!(report.added, 1, "the readable file must still be scanned");

    let errs: i64 = conn
        .query_row("SELECT count(*) FROM import_errors", [], |r| r.get(0))
        .unwrap();
    assert!(
        errs >= 1,
        "traversal error must be recorded in import_errors"
    );
}

// -- mark_vanished_under_root (Stage 3 Task 8 — folder watcher) ------------
//
// TDD per the task brief: these tests were written before `mark_vanished_
// under_root` existed. See that function's doc comment in `scanner.rs` for
// the component-wise (not string/LIKE) prefix check and why the watcher
// always runs this *after* an incremental `scan_folder(root)`.

fn missing_flag(conn: &Connection, id: i64) -> i64 {
    conn.query_row("SELECT missing FROM tracks WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .unwrap()
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
    assert_eq!(missing_flag(&conn, id), 0);
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
    assert_eq!(missing_flag(&conn, id), 1);
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
    assert_eq!(missing_flag(&conn, watched_id), 1);
    assert_eq!(
        missing_flag(&conn, other_id),
        0,
        "a track outside the watched root must never be touched"
    );
}

/// An already-missing track must not be recounted (and its `missing` flag,
/// already `1`, is left as-is) — the watcher only wants to know how many
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

    // Second pass: the same track is already missing=1, so it must not be
    // counted again even though its file is still gone.
    let second = mark_vanished_under_root(&conn, tmp.path()).unwrap();
    assert_eq!(second, 0);
    assert_eq!(missing_flag(&conn, id), 1);
}

/// Test-only: inserts a bare, non-missing track row at `path` with no audio
/// file backing it. Enough for `mark_vanished_under_root`, whose candidate
/// query and prefix/`exists()` checks only read `id`/`path` — the file never
/// having existed means `Path::exists()` is `false`, so the ONLY thing that
/// keeps such a row from being marked is the under-root membership test.
fn insert_raw_track(conn: &Connection, path: &std::path::Path) {
    conn.execute(
        "INSERT INTO tracks (path, added_at, missing) VALUES (?1, 0, 0)",
        [path.to_string_lossy().to_string()],
    )
    .unwrap();
}

fn missing_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM tracks WHERE missing = 1", [], |r| {
        r.get(0)
    })
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
