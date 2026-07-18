//! `scanner.rs`'s test suite, split into its own file in Stage 3 Task 1
//! purely to keep `scanner.rs` itself under the project's 800-line rule —
//! `scanner.rs` declares this via `#[cfg(test)] #[path = "scanner_tests.rs"]
//! mod tests;`, so this file's contents are still the crate-private
//! `crate::library::scanner::tests` module, with the exact same tests,
//! unchanged, that used to live inline (a pure move, not a rewrite).

use super::*;
use lofty::prelude::*;
use lofty::tag::{Tag, TagType};

pub(super) fn fixture_copy(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let dst = dir.join(name);
    std::fs::copy(&src, &dst).unwrap();
    dst
}

/// Test-only: unwraps a `ScanOutcome` down to its `ScanReport`, panicking
/// with the unexpected root on `RootUnavailable`. Every pre-existing test in
/// this file predates Task 1.5's `ScanOutcome` split and only ever scans a
/// root it expects to be reachable, so this keeps those assertions reading
/// exactly as they did against the old bare-`ScanReport` return (`scan_folder
/// (...).unwrap()` becomes `completed(scan_folder(...).unwrap())`) instead of
/// rewriting every one of them to match on `ScanOutcome::Completed` by hand.
pub(super) fn completed(outcome: ScanOutcome) -> ScanReport {
    match outcome {
        ScanOutcome::Completed(report) => report,
        ScanOutcome::RootUnavailable { root } => {
            panic!("expected ScanOutcome::Completed, got RootUnavailable({root:?})")
        }
    }
}

/// Writes identical title/artist/album tags to a fixture file so two
/// separate copies produce the same fingerprint (title+artist+album+
/// duration+file_size) that move detection's step 2 matches on. `pub(super)`
/// (like `fixture_copy`/`completed`/`row_by_path` above) so `scanner_
/// tombstone_tests.rs` can reuse it for its own move-arm test rather than
/// duplicating this helper.
pub(super) fn tag_file(path: &std::path::Path, title: &str, artist: &str, album: &str) {
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.into());
    tag.set_artist(artist.into());
    tag.set_album(album.into());
    tag.save_to_path(path, lofty::config::WriteOptions::default())
        .unwrap();
}

pub(super) fn row_by_path(conn: &Connection, path: &std::path::Path) -> (i64, i64, i64, i64) {
    // (id, rating, play_count, added_at)
    conn.query_row(
        "SELECT id, rating, play_count, added_at FROM tracks WHERE path = ?1",
        [path.to_string_lossy().to_string()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .unwrap()
}

/// `pub(super)`, same reuse reasoning as `tag_file` above.
pub(super) fn row_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
        .unwrap()
}

/// Test-only: `st_dev` of `path`, for asserting `mount_point`'s
/// prefix-and-device invariant (see `mounts::mount_point_of`'s own doc
/// comment) rather than a hardcoded path — the same technique
/// `mounts.rs`'s own tests use, so this passes regardless of the machine's
/// filesystem layout, including a tmpdir that is itself a mount.
fn dev_of(path: &std::path::Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).unwrap().dev()
}

fn mount_point_of(conn: &Connection, path: &std::path::Path) -> Option<String> {
    conn.query_row(
        "SELECT mount_point FROM tracks WHERE path = ?1",
        [path.to_string_lossy().to_string()],
        |r| r.get(0),
    )
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
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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
    let lookup = move_detect::MoveLookup {
        device: 7777,
        inode: 8888,
        title: "Dev Inode Ambiguity",
        artist: "Some Artist",
        album: "Some Album",
        duration_ms: 1000,
        file_size: file_size as i64,
    };
    let result = move_detect::find_move_candidate(&tx, &lookup).unwrap();
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
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r1.added, 2);

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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
    tag.set_date(lofty::tag::items::Timestamp {
        year: 2019,
        ..lofty::tag::items::Timestamp::default()
    });
    tag.set_track(9);
    tag.set_disk(2);
    tag.set_genre("Deathcore".into());
    tag.insert_text(
        lofty::tag::ItemKey::MusicBrainzArtistId,
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into(),
    );
    tag.save_to_path(&file, lofty::config::WriteOptions::default())
        .unwrap();

    let meta = track_meta::read_meta(&file).unwrap();
    assert_eq!(meta.title, "Beast of Darkness");
    assert_eq!(meta.artist, "Brand of Sacrifice");
    assert_eq!(meta.album, "God Hand");
    assert_eq!(meta.year, Some(2019));
    assert_eq!(meta.track_no, Some(9));
    assert_eq!(
        meta.artist_mbid.as_deref(),
        Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
    );
    assert_eq!(meta.disc_no, Some(2));
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

    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!((r1.added, r1.errors), (2, 1));

    // second scan: nothing changed → everything skipped
    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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

    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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

    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
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
///
/// Task 1.6: this fast-path-restore branch is also the ONLY chance a
/// pre-v10 row (or any row whose `mount_point` is NULL for some other
/// reason) has to acquire a `mount_point` without its file actually
/// changing — an unchanged, still-missing-free row never reaches any other
/// arm of the scanner. `mount_point` is explicitly nulled out below before
/// the restore-rescan to prove this branch is the one that (re)populates it,
/// rather than it merely surviving from the initial insert.
#[test]
fn restored_file_at_same_path_clears_missing_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let file = fixture_copy(tmp.path(), "track.flac");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r1.added, 1);

    let path_str = file.to_string_lossy().to_string();
    conn.execute(
        "UPDATE tracks SET missing_since = 1, missing_reason = 'unknown', \
         rating = 4, play_count = 7, mount_point = NULL WHERE path = ?1",
        [&path_str],
    )
    .unwrap();
    tx_insert_import_error(&conn, &path_str);

    // The file itself is untouched on disk: same path, same mtime — this
    // is exactly the "reappeared unchanged" scenario (NAS remount,
    // restore-from-trash), not a content change.
    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert!(
        r2.updated >= 1,
        "restoring a missing track must count as an update"
    );

    let (missing_since, rating, play_count): (Option<i64>, i64, i64) = conn
        .query_row(
            "SELECT missing_since, rating, play_count FROM tracks WHERE path = ?1",
            [&path_str],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert!(
        missing_since.is_none(),
        "missing_since must be cleared on restore"
    );
    assert_eq!(rating, 4, "rating must survive a restore");
    assert_eq!(play_count, 7, "play_count must survive a restore");

    let mount_point = mount_point_of(&conn, &file)
        .expect("the fast-path restore branch must (re)populate mount_point");
    let mount_path = std::path::PathBuf::from(&mount_point);
    assert!(
        file.starts_with(&mount_path),
        "{mount_path:?} must be a prefix of {file:?}"
    );

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
        "INSERT INTO import_errors (path, reason_kind, reason_detail, first_seen, last_seen) \
         VALUES (?1, 'tag', 'stale', 0, 0)",
        [path],
    )
    .unwrap();
}

/// Task 1.6: every row a scan newly inserts must come out with a non-NULL
/// `mount_point` that satisfies `mounts::mount_point_of`'s own invariant —
/// asserted structurally (prefix + device-boundary), not against a
/// hardcoded path, so this passes on any machine's filesystem layout. The
/// mount point can only ever be recorded while the file is reachable (see
/// `scanner_mount.rs`'s module doc comment for why it can't be derived
/// later, once a drive is gone); this test pins that the insert/upsert arm
/// actually does that recording rather than leaving the column NULL.
#[test]
fn newly_scanned_track_gets_mount_point_satisfying_invariant() {
    let tmp = tempfile::tempdir().unwrap();
    let file = fixture_copy(tmp.path(), "track.flac");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r1.added, 1);

    let mount_point =
        mount_point_of(&conn, &file).expect("newly scanned row must have mount_point recorded");
    let mount_path = std::path::PathBuf::from(&mount_point);

    assert!(
        file.starts_with(&mount_path),
        "{mount_path:?} must be a prefix of {file:?}"
    );
    assert_eq!(dev_of(&mount_path), dev_of(&file));
}

/// Task 1.6 / Beschluss 3: a track moved from one location to another must
/// have its `mount_point` refreshed to the NEW location on the rescan that
/// detects the move, not left holding the value recorded at the old path.
/// The row's `mount_point` is deliberately poisoned with a value
/// `mount_point_of` could never produce for either location before the
/// move-rescan runs: if the move arm forgot to recompute it, this stale
/// sentinel would still be sitting there and the test would catch it, even
/// though old and new path happen to share the same real mount point on a
/// single-filesystem test machine.
#[test]
fn move_detection_refreshes_mount_point_to_new_location() {
    let tmp = tempfile::tempdir().unwrap();
    let old_path = fixture_copy(tmp.path(), "track.flac");
    tag_file(&old_path, "Moved Song", "Some Artist", "Some Album");

    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let r1 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r1.added, 1);

    conn.execute(
        "UPDATE tracks SET mount_point = '/definitely-not-a-real-mount' WHERE path = ?1",
        [old_path.to_string_lossy().to_string()],
    )
    .unwrap();

    let new_dir = tmp.path().join("new_subdir");
    std::fs::create_dir(&new_dir).unwrap();
    let new_path = new_dir.join("track.flac");
    std::fs::rename(&old_path, &new_path).unwrap();

    let r2 = completed(scan_folder(&mut conn, tmp.path()).unwrap());
    assert_eq!(r2.moved, 1);

    let mount_point = mount_point_of(&conn, &new_path)
        .expect("moved row must have mount_point recorded after the move rescan");
    assert_ne!(
        mount_point, "/definitely-not-a-real-mount",
        "move arm must refresh mount_point, not leave the stale value from the old location"
    );

    let expected = crate::library::mounts::mount_point_of(&new_path)
        .map(|p| p.to_string_lossy().into_owned())
        .expect("mount_point_of must resolve for a path that exists");
    assert_eq!(mount_point, expected);
}

// Task 1.9: the tombstone-resurrect + `healed`-counter test suite lives in
// its own sibling file — see `scanner_tombstone_tests.rs`'s own module doc
// comment — purely to keep this file under the project's 800-line rule,
// same reasoning as every other `_tests.rs` split in this module.

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

    let report = completed(scan_result.unwrap());
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
