//! The issue a sheet raises: when it appears, when it goes, and how a dismissal
//! holds; and the sheet that arrives beside a file whose own issue was dismissed.

use super::super::tests::fixture_copy;
use super::{issue, scan, segments_of};

/// A sheet beside `embedded.flac` that cuts it in two.
const FLAC_SHEET: &str = "FILE \"embedded.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"A\"\n    \
                          INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    TITLE \"B\"\n    INDEX 01 00:00:40\n";

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
