//! Trash acts per audio file (CUE-11): a CUE file goes to the trash only when
//! every one of its tracks is selected, with the sheet beside it once no other
//! file still needs that sheet; a partial selection hides the selected tracks.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use super::*;

struct Library {
    dir: tempfile::TempDir,
    db: Db,
}

impl Library {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            db: Db::open_in_memory().unwrap(),
        }
    }

    fn file(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, b"scratch").unwrap();
        path
    }

    fn whole(&self, id: i64, path: &Path) {
        self.db
            .conn()
            .execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (?1, ?2, 'T', '', 0)",
                rusqlite::params![id, path.to_string_lossy()],
            )
            .unwrap();
    }

    /// Track `index` of `path`, cut by `sheet`, or by a sheet embedded in the file.
    fn segment(&self, id: i64, path: &Path, index: i64, sheet: Option<&Path>) {
        self.db
            .conn()
            .execute(
                "INSERT INTO tracks (id, path, title, artist, added_at, segment_index,
                                     segment_start_ms, segment_end_ms, cue_path, cue_mtime, cue_size)
                 VALUES (?1, ?2, ?3, '', 0, ?4, ?5, ?6, ?7, 1, 1)",
                rusqlite::params![
                    id,
                    path.to_string_lossy(),
                    format!("Song {index}"),
                    index,
                    (index - 1) * 1_000,
                    index * 1_000,
                    sheet.map(|sheet| sheet.to_string_lossy().into_owned()),
                ],
            )
            .unwrap();
    }

    fn ids(&self) -> Vec<i64> {
        self.db
            .conn()
            .prepare("SELECT id FROM tracks ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn excluded(&self) -> Vec<(String, i64)> {
        self.db
            .conn()
            .prepare("SELECT path, segment_index FROM library_exclusions ORDER BY segment_index")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }
}

/// Trashes by removing the scratch file and remembers every path it was handed.
fn trash(library: &Library, tracks: &[(i64, PathBuf)]) -> (TrashReport, Vec<PathBuf>) {
    let calls = RefCell::new(Vec::new());
    let report = trash_tracks_with(&library.db, tracks, |path| {
        calls.borrow_mut().push(path.to_path_buf());
        std::fs::remove_file(path).map_err(|error| error.to_string())
    });
    (report, calls.into_inner())
}

#[test]
fn cue_11_selecting_every_track_of_a_cue_file_trashes_the_file_and_its_sheet_once() {
    let library = Library::new();
    let audio = library.file("album.flac");
    let sheet = library.file("album.cue");
    for index in 1..=3 {
        library.segment(index, &audio, index, Some(&sheet));
    }
    let selection: Vec<_> = (1..=3).map(|id| (id, audio.clone())).collect();

    let plan = plan_file_trash(&library.db, &selection);
    let (report, calls) = trash(&library, &selection);

    assert_eq!((plan.files.len(), plan.hidden.len()), (1, 0));
    assert_eq!(calls, [audio.clone(), sheet.clone()]);
    assert_eq!(report.removed_ids, [1, 2, 3]);
    assert!(report.failures.is_empty());
    assert!(library.ids().is_empty());
    assert!(!audio.exists() && !sheet.exists());
}

#[test]
fn cue_11_a_partial_selection_hides_the_selected_tracks_and_trashes_nothing() {
    let library = Library::new();
    let audio = library.file("album.flac");
    let sheet = library.file("album.cue");
    for index in 1..=3 {
        library.segment(index, &audio, index, Some(&sheet));
    }
    let selection = vec![(2, audio.clone())];

    let plan = plan_file_trash(&library.db, &selection);
    let (report, calls) = trash(&library, &selection);

    assert_eq!((plan.files.len(), plan.hidden.len()), (0, 1));
    assert!(calls.is_empty());
    assert_eq!(report.removed_ids, [2]);
    assert_eq!(report.hidden_ids, [2]);
    assert_eq!(library.ids(), [1, 3]);
    assert_eq!(
        library.excluded(),
        [(audio.to_string_lossy().into_owned(), 2)]
    );
    assert!(audio.exists() && sheet.exists());
}

#[test]
fn cue_11_a_sheet_over_two_files_goes_only_with_the_last_of_them() {
    let library = Library::new();
    let first = library.file("01.flac");
    let second = library.file("02.flac");
    let sheet = library.file("disc.cue");
    library.segment(1, &first, 1, Some(&sheet));
    library.segment(2, &second, 1, Some(&sheet));

    let (_, calls) = trash(&library, &[(1, first.clone())]);
    assert_eq!(calls, [first]);
    assert!(sheet.exists(), "the other file still needs the sheet");

    let (report, calls) = trash(&library, &[(2, second.clone())]);
    assert_eq!(calls, [second.clone(), sheet.clone()]);
    assert_eq!(report.removed_ids, [2]);
    assert!(!sheet.exists());
}

#[test]
fn cue_11_a_sheet_whose_other_file_is_hidden_stays() {
    let library = Library::new();
    let first = library.file("01.flac");
    let second = library.file("02.flac");
    let sheet = library.file("disc.cue");
    library.segment(1, &first, 1, Some(&sheet));
    library.segment(2, &second, 1, Some(&sheet));
    crate::queries::exclude_tracks_matching_paths(&library.db, &[(2, second.clone())], 1).unwrap();

    let (_, calls) = trash(&library, &[(1, first.clone())]);

    assert_eq!(calls, [first]);
    assert!(
        sheet.exists(),
        "without its sheet the hidden file would come back whole"
    );
}

#[test]
fn cue_11_a_file_cut_by_an_embedded_sheet_is_trashed_alone() {
    let library = Library::new();
    let audio = library.file("embedded.flac");
    let stranger = library.file("embedded.cue");
    library.segment(1, &audio, 1, None);
    library.segment(2, &audio, 2, None);

    let (report, calls) = trash(&library, &[(1, audio.clone()), (2, audio.clone())]);

    assert_eq!(calls, [audio]);
    assert_eq!(report.removed_ids, [1, 2]);
    assert!(stranger.exists());
}

#[test]
fn cue_11_a_whole_file_and_a_cue_file_mix_in_one_selection() {
    let library = Library::new();
    let plain = library.file("plain.flac");
    let rejected = library.file("plain.cue");
    let audio = library.file("album.flac");
    let sheet = library.file("album.cue");
    library.whole(1, &plain);
    // A sheet that did not fit leaves its file whole and remembers the sheet;
    // it never cut the file, so it is not the file's to take along.
    library
        .db
        .conn()
        .execute(
            "UPDATE tracks SET cue_path = ?1, cue_mtime = 1, cue_size = 1 WHERE id = 1",
            [rejected.to_string_lossy()],
        )
        .unwrap();
    library.segment(2, &audio, 1, Some(&sheet));
    library.segment(3, &audio, 2, Some(&sheet));
    let selection = vec![(1, plain.clone()), (3, audio.clone())];

    let plan = plan_file_trash(&library.db, &selection);
    let (report, calls) = trash(&library, &selection);

    assert_eq!((plan.files.len(), plan.hidden.len()), (1, 1));
    assert_eq!(calls, [plain]);
    assert!(rejected.exists());
    assert_eq!(report.removed_ids, [1, 3]);
    assert_eq!(report.hidden_ids, [3]);
    assert_eq!(library.ids(), [2]);
}

#[test]
fn cue_11_a_failed_trash_keeps_every_track_of_the_file() {
    let library = Library::new();
    let audio = library.file("album.flac");
    let sheet = library.file("album.cue");
    library.segment(1, &audio, 1, Some(&sheet));
    library.segment(2, &audio, 2, Some(&sheet));
    let selection = vec![(1, audio.clone()), (2, audio.clone())];

    let report = trash_tracks_with(&library.db, &selection, |_| Err("refused".into()));

    assert!(report.removed_ids.is_empty());
    assert_eq!(
        report
            .failures
            .iter()
            .map(|failure| failure.id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(library.ids(), [1, 2]);
    assert!(sheet.exists());
}

#[test]
fn cue_11_a_sheet_over_two_files_trashed_together_goes_after_both() {
    let library = Library::new();
    let first = library.file("01.flac");
    let second = library.file("02.flac");
    let sheet = library.file("disc.cue");
    library.segment(1, &first, 1, Some(&sheet));
    library.segment(2, &second, 1, Some(&sheet));

    let (report, calls) = trash(&library, &[(1, first.clone()), (2, second.clone())]);

    assert_eq!(calls, [first, second, sheet.clone()]);
    assert_eq!(report.removed_ids, [1, 2]);
    assert!(!sheet.exists());
}

#[test]
fn cue_11_a_sheet_stays_when_an_earlier_file_it_describes_fails_to_trash() {
    let library = Library::new();
    let first = library.file("01.flac");
    let second = library.file("02.flac");
    let sheet = library.file("disc.cue");
    library.segment(1, &first, 1, Some(&sheet));
    library.segment(2, &second, 1, Some(&sheet));
    let calls = RefCell::new(Vec::new());

    let report = trash_tracks_with(
        &library.db,
        &[(1, first.clone()), (2, second.clone())],
        |path| {
            calls.borrow_mut().push(path.to_path_buf());
            if path == first {
                return Err("refused".into());
            }
            std::fs::remove_file(path).map_err(|error| error.to_string())
        },
    );

    assert_eq!(calls.into_inner(), [first.clone(), second]);
    assert_eq!(report.removed_ids, [2]);
    assert_eq!(library.ids(), [1]);
    assert!(
        sheet.exists(),
        "without its sheet the file left behind would come back whole"
    );
}
