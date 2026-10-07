//! A CUE track removed from the library stays hidden as the song it was, not
//! as the position it held (finding B9), and a CUE file whose every track is
//! hidden is as settled as any other unchanged file (finding A11).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::super::cue_tests::{segments_of, titles, Album, THREE_TRACKS};

/// `THREE_TRACKS` with a track inserted between the first and the second: the
/// second and third keep their start and title, and move one place on.
const FOUR_TRACKS: &str = "REM DATE 1979
PERFORMER \"Joy Division\"
TITLE \"Unknown Pleasures\"
FILE \"album.wav\" WAVE
  TRACK 01 AUDIO
    TITLE \"Disorder\"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE \"Insight\"
    INDEX 01 00:05:00
  TRACK 03 AUDIO
    TITLE \"Day of the Lords\"
    INDEX 01 00:10:00
  TRACK 04 AUDIO
    TITLE \"Candidate\"
    INDEX 01 00:20:00
";

fn remove_titled(album: &Album, title: &str) {
    let (id, path): (i64, String) = album
        .db
        .conn()
        .query_row(
            "SELECT id, path FROM tracks WHERE title = ?1",
            [title],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let removed =
        crate::queries::exclude_tracks_matching_paths(&album.db, &[(id, PathBuf::from(path))], 1)
            .unwrap();
    assert_eq!(removed, [id]);
}

/// `(segment_index, segment_start_ms, segment_title, cue_path, cue_mtime, cue_size)`
/// of every exclusion, by title.
type ExclusionColumns = (
    i64,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
);

fn exclusions(album: &Album) -> Vec<ExclusionColumns> {
    album
        .db
        .conn()
        .prepare(
            "SELECT segment_index, segment_start_ms, segment_title, cue_path, cue_mtime, \
             cue_size FROM library_exclusions ORDER BY segment_title",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn sheet_version(path: &Path) -> (i64, i64) {
    let metadata = std::fs::metadata(path).unwrap();
    let mtime = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    (mtime, metadata.len() as i64)
}

#[test]
fn cue_18_a_hidden_track_records_its_start_title_and_sheet() {
    let album = Album::new();
    album.scan();

    remove_titled(&album, "Day of the Lords");

    let (mtime, size) = sheet_version(&album.sheet());
    assert_eq!(
        exclusions(&album),
        [(
            2,
            Some(10_000),
            Some("Day of the Lords".to_string()),
            Some(album.sheet().to_string_lossy().into_owned()),
            Some(mtime),
            Some(size),
        )]
    );
}

#[test]
fn cue_18_a_sheet_edit_that_inserts_a_track_keeps_the_hidden_song_hidden() {
    let album = Album::new();
    album.scan();
    remove_titled(&album, "Day of the Lords");

    album.rewrite_sheet(FOUR_TRACKS);
    album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(titles(&rows), ["Disorder", "Insight", "Candidate"]);
    let (mtime, size) = sheet_version(&album.sheet());
    let moved = &exclusions(&album)[0];
    assert_eq!(
        moved.0, 3,
        "the exclusion follows its song to its new place"
    );
    assert_eq!((moved.4, moved.5), (Some(mtime), Some(size)));
}

#[test]
fn cue_18_hiding_the_track_that_took_a_hidden_songs_old_place_keeps_both_hidden() {
    let album = Album::new();
    album.scan();
    remove_titled(&album, "Day of the Lords");
    album.rewrite_sheet(FOUR_TRACKS);
    album.scan();

    remove_titled(&album, "Insight");
    album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(titles(&rows), ["Disorder", "Candidate"]);
    let hidden: Vec<Option<String>> = exclusions(&album).into_iter().map(|row| row.2).collect();
    assert_eq!(
        hidden,
        [
            Some("Day of the Lords".to_string()),
            Some("Insight".to_string())
        ]
    );
}

#[test]
fn cue_18_a_hidden_track_whose_sheet_drops_it_comes_back_hidden_when_it_returns() {
    let album = Album::new();
    album.scan();
    remove_titled(&album, "Candidate");

    album.rewrite_sheet(&THREE_TRACKS.replace(
        "  TRACK 03 AUDIO\n    TITLE \"Candidate\"\n    INDEX 01 00:20:00\n",
        "",
    ));
    album.scan();
    album.rewrite_sheet(THREE_TRACKS);
    album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(titles(&rows), ["Disorder", "Day of the Lords"]);
}

#[test]
fn cue_18_a_fully_hidden_cue_file_is_not_read_again_on_an_unchanged_rescan() {
    let album = Album::new();
    album.scan();
    for title in ["Disorder", "Day of the Lords", "Candidate"] {
        remove_titled(&album, title);
    }
    // A sheet the scan reads although nothing changed cannot be read now, and
    // an unreadable sheet leaves a file with no rows to be read as if none
    // were there: the hidden album would come back as one whole-file track.
    let sheet = album.sheet();
    std::fs::set_permissions(&sheet, std::fs::Permissions::from_mode(0o000)).unwrap();

    let report = album.scan();

    std::fs::set_permissions(&sheet, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(segments_of(album.db.conn(), &album.audio()).is_empty());
    assert_eq!(
        (report.added, report.updated, report.skipped_unchanged),
        (0, 0, 1)
    );
}

#[test]
fn cue_18_a_fully_hidden_cue_file_stays_hidden_while_its_changed_sheet_cannot_be_read() {
    let album = Album::new();
    album.scan();
    for title in ["Disorder", "Day of the Lords", "Candidate"] {
        remove_titled(&album, title);
    }
    // The sheet changed since the hidden tracks were placed, and cannot be
    // read now: nothing is known about the file this scan, and the rows it
    // would be read into are not the whole-file track of a file nobody cut.
    let sheet = album.sheet();
    let text = std::fs::read(&sheet).unwrap();
    std::fs::write(&sheet, [text.as_slice(), b"\n"].concat()).unwrap();
    std::fs::set_permissions(&sheet, std::fs::Permissions::from_mode(0o000)).unwrap();

    let report = album.scan();

    std::fs::set_permissions(&sheet, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(segments_of(album.db.conn(), &album.audio()).is_empty());
    assert_eq!(
        (report.added, report.updated, report.skipped_unchanged),
        (0, 0, 1)
    );
}
