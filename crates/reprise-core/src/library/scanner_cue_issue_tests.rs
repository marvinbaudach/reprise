//! The issue a sheet raises: when it appears, when it goes, and how a dismissal
//! holds; and the sheet that arrives beside a file whose own issue was dismissed.

use std::path::Path;

use rusqlite::Connection;

use super::super::tests::fixture_copy;
use super::{issue, scan, segments_of, titles, Album, THREE_TRACKS};

const BROKEN: &str = "this is not a cue sheet at all";

/// A sheet beside `embedded.flac` that cuts it in two.
const FLAC_SHEET: &str = "FILE \"embedded.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"A\"\n    \
                          INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    TITLE \"B\"\n    INDEX 01 00:00:40\n";

/// Dismisses the issue raised against `path`, as the user would, for the file as
/// it is now.
fn dismiss(conn: &Connection, path: &Path) {
    let metadata = std::fs::metadata(path).unwrap();
    let mtime = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let changed = conn
        .execute(
            "UPDATE import_errors SET dismissed_mtime = ?1, dismissed_size = ?2 WHERE path = ?3",
            rusqlite::params![mtime as i64, metadata.len() as i64, path.to_string_lossy()],
        )
        .unwrap();
    assert_eq!(changed, 1, "there is an issue to dismiss");
}

/// `(dismissed_mtime, seen_count)` of the issue raised against `path`.
fn dismissal(conn: &Connection, path: &Path) -> (Option<i64>, i64) {
    conn.query_row(
        "SELECT dismissed_mtime, seen_count FROM import_errors WHERE path = ?1",
        [path.to_string_lossy()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

/// A FLAC whose embedded sheet places a track past its end, scanned once.
fn flac_with_a_broken_embedded_sheet() -> (tempfile::TempDir, crate::db::Db, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let audio = fixture_copy(dir.path(), "embedded.flac");
    super::super::cue_files_tests::embed_sheet(&audio, "TRACK 01 AUDIO\n  INDEX 01 99:00:00\n");
    let db = crate::db::Db::open_in_memory().unwrap();
    scan(&db, dir.path());
    assert!(issue(db.conn(), &audio).is_some());
    (dir, db, audio)
}

#[test]
fn cue_2_a_sheet_that_stops_parsing_leaves_the_file_whole_and_raises_an_issue() {
    let album = Album::new();
    album.scan();

    album.rewrite_sheet(BROKEN);
    album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, 0);
    assert_eq!(
        issue(album.db.conn(), &album.sheet()).map(|(kind, _)| kind),
        Some("invalid_cue_sheet".to_string())
    );
}

#[test]
fn cue_2_mending_a_sheet_that_does_not_parse_clears_its_issue() {
    let album = Album::new();
    album.rewrite_sheet(BROKEN);
    album.scan();
    assert!(issue(album.db.conn(), &album.sheet()).is_some());

    album.rewrite_sheet(THREE_TRACKS);
    album.scan();

    assert_eq!(segments_of(album.db.conn(), &album.audio()).len(), 3);
    assert_eq!(issue(album.db.conn(), &album.sheet()), None);
}

#[test]
fn cue_2_a_dismissed_issue_on_the_audio_does_not_stop_a_sheet_beside_it() {
    let (dir, db, audio) = flac_with_a_broken_embedded_sheet();
    dismiss(db.conn(), &audio);

    std::fs::write(dir.path().join("embedded.cue"), FLAC_SHEET).unwrap();
    scan(&db, dir.path());

    assert_eq!(titles(&segments_of(db.conn(), &audio)), ["A", "B"]);
}

#[test]
fn cue_2_a_dismissed_broken_embedded_sheet_stays_quiet_when_the_file_is_read_again() {
    let (dir, db, audio) = flac_with_a_broken_embedded_sheet();
    dismiss(db.conn(), &audio);
    let before = dismissal(db.conn(), &audio);
    // Rows written under a sheet that is gone by now: the file is read again
    // although it did not change.
    db.conn()
        .execute(
            "UPDATE tracks SET cue_path = ?1, cue_mtime = 1",
            [dir.path().join("gone.cue").to_string_lossy()],
        )
        .unwrap();

    scan(&db, dir.path());

    let cue_path: Option<String> = db
        .conn()
        .query_row("SELECT cue_path FROM tracks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(cue_path, None, "the file was read again");
    assert_eq!(dismissal(db.conn(), &audio), before);
}

#[test]
fn cue_1a_a_sheet_beside_a_flac_wins_over_a_broken_one_inside_it() {
    let (dir, db, audio) = flac_with_a_broken_embedded_sheet();

    std::fs::write(dir.path().join("embedded.cue"), FLAC_SHEET).unwrap();
    scan(&db, dir.path());

    assert_eq!(titles(&segments_of(db.conn(), &audio)), ["A", "B"]);
    assert_eq!(
        issue(db.conn(), &audio),
        None,
        "the embedded sheet is not used"
    );
}

#[test]
fn cue_2_an_embedded_sheet_larger_than_a_sheet_can_be_keeps_the_file_whole() {
    let dir = tempfile::tempdir().unwrap();
    let audio = fixture_copy(dir.path(), "embedded.flac");
    let mut sheet = FLAC_SHEET.to_string();
    sheet.push_str(&" ".repeat(crate::cue::MAX_SHEET_BYTES));
    super::super::cue_files_tests::embed_sheet(&audio, &sheet);
    let db = crate::db::Db::open_in_memory().unwrap();

    scan(&db, dir.path());

    assert_eq!(segments_of(db.conn(), &audio).len(), 1);
    assert_eq!(
        issue(db.conn(), &audio).map(|(kind, _)| kind),
        Some("invalid_cue_sheet".to_string())
    );
}
