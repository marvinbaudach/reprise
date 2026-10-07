//! A broken sheet's issue under Issues → Import errors (CUE-14): Retry rescans
//! the sheet's directory, an issue for a sheet that is gone clears when its
//! directory is scanned, and a sheet beside a file that does not fit it gives
//! way to a valid sheet embedded in the file (finding A10).

use std::path::Path;

use crate::models::ImportErrorKind;
use crate::queries::ImportErrorEntry;

use super::super::cue_files_tests::embed_sheet;
use super::super::cue_tests::{issue, scan, segments_of, titles, Album, THREE_TRACKS};
use super::super::tests::fixture_copy;

const BROKEN: &str = "this is not a cue sheet at all";

fn entry(path: &Path, kind: ImportErrorKind) -> ImportErrorEntry {
    ImportErrorEntry {
        path: path.to_string_lossy().into_owned(),
        kind,
        detail: String::new(),
        first_seen: 0,
        last_seen: 0,
        seen_count: 1,
        is_hint: false,
    }
}

#[test]
fn cue_14_retry_on_a_sheet_issue_rescans_the_sheets_directory() {
    let album = Album::new();
    std::fs::write(album.sheet(), BROKEN).unwrap();
    album.scan();
    assert!(issue(album.db.conn(), &album.sheet()).is_some());
    album.rewrite_sheet(THREE_TRACKS);

    let root = entry(&album.sheet(), ImportErrorKind::InvalidCueSheet).retry_root();
    crate::library::scanner::scan_folder(&album.db, &root).unwrap();

    assert_eq!(root, album.dir.path());
    assert_eq!(issue(album.db.conn(), &album.sheet()), None);
    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(titles(&rows), ["Disorder", "Day of the Lords", "Candidate"]);
}

#[test]
fn cue_14_retry_on_an_audio_file_rescans_that_file() {
    let audio = Path::new("/music/album/embedded.flac");

    assert_eq!(
        entry(audio, ImportErrorKind::InvalidCueSheet).retry_root(),
        audio
    );
    assert_eq!(entry(audio, ImportErrorKind::Io).retry_root(), audio);
    let sheet = Path::new("/music/album/Album.CUE");
    assert_eq!(
        entry(sheet, ImportErrorKind::InvalidCueSheet).retry_root(),
        Path::new("/music/album")
    );
}

#[test]
fn cue_14_the_issue_of_a_deleted_sheet_clears_when_its_directory_is_scanned() {
    let album = Album::new();
    std::fs::write(album.sheet(), BROKEN).unwrap();
    album.scan();
    assert!(issue(album.db.conn(), &album.sheet()).is_some());

    std::fs::remove_file(album.sheet()).unwrap();
    album.scan();

    assert_eq!(issue(album.db.conn(), &album.sheet()), None);
}

#[test]
fn cue_14_a_deleted_sheet_leaves_an_embedded_sheets_issue_alone() {
    let dir = tempfile::tempdir().unwrap();
    let audio = fixture_copy(dir.path(), "embedded.flac");
    embed_sheet(&audio, "TRACK 01 AUDIO\n  INDEX 01 99:00:00\n");
    let sheet = dir.path().join("gone.cue");
    std::fs::write(&sheet, BROKEN).unwrap();
    let db = crate::db::Db::open_in_memory().unwrap();
    scan(&db, dir.path());
    assert!(issue(db.conn(), &audio).is_some());
    assert!(issue(db.conn(), &sheet).is_some());

    std::fs::remove_file(&sheet).unwrap();
    scan(&db, dir.path());

    assert_eq!(issue(db.conn(), &sheet), None);
    assert!(
        issue(db.conn(), &audio).is_some(),
        "the embedded sheet's issue is keyed by its audio file"
    );
}

#[test]
fn cue_14_a_sheet_beside_a_file_that_does_not_fit_gives_way_to_its_embedded_sheet() {
    let dir = tempfile::tempdir().unwrap();
    let audio = fixture_copy(dir.path(), "embedded.flac");
    embed_sheet(
        &audio,
        "FILE \"CDImage.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"A\"\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    TITLE \"B\"\n    INDEX 01 00:00:40\n",
    );
    let sheet = dir.path().join("embedded.cue");
    std::fs::write(
        &sheet,
        "FILE \"embedded.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Late\"\n    \
         INDEX 01 99:00:00\n",
    )
    .unwrap();
    let db = crate::db::Db::open_in_memory().unwrap();

    scan(&db, dir.path());

    let rows = segments_of(db.conn(), &audio);
    assert_eq!(titles(&rows), ["A", "B"]);
    assert!(
        issue(db.conn(), &sheet).is_some(),
        "the sheet beside the file still does not fit"
    );
    let again = scan(&db, dir.path());
    assert_eq!(
        (again.added, again.updated, again.skipped_unchanged),
        (0, 0, 1)
    );
    assert_eq!(titles(&segments_of(db.conn(), &audio)), ["A", "B"]);
}

#[test]
fn cue_11_trashing_a_file_whose_embedded_sheet_won_takes_the_sheet_beside_it_along() {
    let dir = tempfile::tempdir().unwrap();
    let audio = fixture_copy(dir.path(), "embedded.flac");
    embed_sheet(
        &audio,
        "FILE \"CDImage.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"A\"\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    TITLE \"B\"\n    INDEX 01 00:00:40\n",
    );
    let sheet = dir.path().join("embedded.cue");
    std::fs::write(
        &sheet,
        "FILE \"embedded.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 99:00:00\n",
    )
    .unwrap();
    let db = crate::db::Db::open_in_memory().unwrap();
    scan(&db, dir.path());
    let selection: Vec<(i64, std::path::PathBuf)> =
        crate::queries::track_ids_for_path(&db, &audio.to_string_lossy())
            .unwrap()
            .into_iter()
            .map(|id| (id, audio.clone()))
            .collect();
    assert_eq!(selection.len(), 2);

    let report = crate::library::trash_tracks::trash_tracks_with(&db, &selection, |path| {
        std::fs::remove_file(path).map_err(|error| error.to_string())
    });

    assert_eq!(report.removed_ids.len(), 2);
    assert!(
        !sheet.exists(),
        "the sheet beside the file names it and would only raise an issue once it is gone"
    );
}
