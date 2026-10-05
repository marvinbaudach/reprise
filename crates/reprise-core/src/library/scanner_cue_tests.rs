//! A CUE sheet beside an audio file, or embedded in it, splits the file into
//! its tracks. The audio is synthetic silence the tests write themselves: the
//! developer's own library holds no CUE rips.

use std::path::{Path, PathBuf};

use super::tests::{completed, fixture_copy, row_count};
use super::*;
use lofty::prelude::*;
use rusqlite::OptionalExtension;

const SAMPLE_RATE: u32 = 8_000;

/// A mono 8-bit WAV of `seconds` of silence, which lofty reads the duration of.
fn write_wav(path: &Path, seconds: u32) {
    let data_len = SAMPLE_RATE * seconds;
    let mut body = Vec::new();
    body.extend_from_slice(b"WAVEfmt ");
    body.extend_from_slice(&16_u32.to_le_bytes());
    body.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    body.extend_from_slice(&1_u16.to_le_bytes()); // mono
    body.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    body.extend_from_slice(&SAMPLE_RATE.to_le_bytes()); // byte rate
    body.extend_from_slice(&1_u16.to_le_bytes()); // block align
    body.extend_from_slice(&8_u16.to_le_bytes()); // bits per sample
    body.extend_from_slice(b"data");
    body.extend_from_slice(&data_len.to_le_bytes());
    body.extend(std::iter::repeat_n(0x80_u8, data_len as usize));
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    std::fs::write(path, out).unwrap();
}

const THREE_TRACKS: &str = "REM DATE 1979
REM GENRE \"Post-punk\"
PERFORMER \"Joy Division\"
TITLE \"Unknown Pleasures\"
FILE \"album.wav\" WAVE
  TRACK 01 AUDIO
    TITLE \"Disorder\"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE \"Day of the Lords\"
    INDEX 01 00:10:00
  TRACK 03 AUDIO
    TITLE \"Candidate\"
    INDEX 01 00:20:00
";

fn scan(db: &crate::db::Db, root: &Path) -> ScanReport {
    completed(scan_folder(db, root).unwrap())
}

/// An album of three tracks over thirty seconds, scanned once.
struct Album {
    dir: tempfile::TempDir,
    db: crate::db::Db,
}

impl Album {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        write_wav(&dir.path().join("album.wav"), 30);
        std::fs::write(dir.path().join("album.cue"), THREE_TRACKS).unwrap();
        Self {
            dir,
            db: crate::db::Db::open_in_memory().unwrap(),
        }
    }

    fn audio(&self) -> PathBuf {
        self.dir.path().join("album.wav")
    }

    fn sheet(&self) -> PathBuf {
        self.dir.path().join("album.cue")
    }

    fn scan(&self) -> ScanReport {
        scan(&self.db, self.dir.path())
    }

    fn rewrite_sheet(&self, text: &str) {
        let sheet = self.sheet();
        std::fs::write(&sheet, text).unwrap();
        bump_mtime(&sheet);
    }
}

/// Moves a file's mtime forward, so a rewrite within one second still counts as
/// a change to a scanner that compares whole seconds.
fn bump_mtime(path: &Path) {
    static MINUTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let minutes = MINUTES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let file = std::fs::File::options().write(true).open(path).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60 * minutes);
    file.set_modified(later).unwrap();
}

/// `(segment_index, start_ms, end_ms, title)` of every row at `path`, in order.
fn segments_of(conn: &Connection, path: &Path) -> Vec<(i64, Option<i64>, Option<i64>, String)> {
    let mut statement = conn
        .prepare(
            "SELECT segment_index, segment_start_ms, segment_end_ms, title FROM tracks \
             WHERE path = ?1 ORDER BY segment_index",
        )
        .unwrap();
    statement
        .query_map([path.to_string_lossy().to_string()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn ids_of(conn: &Connection, path: &Path) -> Vec<i64> {
    let mut statement = conn
        .prepare("SELECT id FROM tracks WHERE path = ?1 ORDER BY segment_index")
        .unwrap();
    statement
        .query_map([path.to_string_lossy().to_string()], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn issue(conn: &Connection, path: &Path) -> Option<(String, String)> {
    conn.query_row(
        "SELECT reason_kind, reason_detail FROM import_errors WHERE path = ?1",
        [path.to_string_lossy().to_string()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .unwrap()
}

fn titles(rows: &[(i64, Option<i64>, Option<i64>, String)]) -> Vec<&str> {
    rows.iter().map(|row| row.3.as_str()).collect()
}

#[test]
fn cue_1a_a_sheet_beside_the_file_splits_it_into_its_tracks() {
    let album = Album::new();

    let report = album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(
        rows,
        [
            (1, Some(0), Some(10_000), "Disorder".to_string()),
            (
                2,
                Some(10_000),
                Some(20_000),
                "Day of the Lords".to_string()
            ),
            (3, Some(20_000), Some(30_000), "Candidate".to_string()),
        ]
    );
    assert_eq!(report.added, 3, "the report counts tracks, not files");
    assert_eq!(
        row_count(album.db.conn()),
        3,
        "no whole-file row beside them"
    );
}

#[test]
fn every_track_carries_the_sheets_metadata_and_its_own_duration() {
    let album = Album::new();
    album.scan();

    let row: (
        String,
        String,
        String,
        Option<i32>,
        Option<i32>,
        String,
        i64,
        String,
    ) = album
        .db
        .conn()
        .query_row(
            "SELECT artist, album, album_artist, year, track_no, genre, duration_ms, cue_path \
             FROM tracks WHERE segment_index = 2",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(
        row,
        (
            "Joy Division".into(),
            "Unknown Pleasures".into(),
            "Joy Division".into(),
            Some(1979),
            Some(2),
            "Post-punk".into(),
            10_000,
            album.sheet().to_string_lossy().into_owned(),
        )
    );
}

#[test]
fn what_the_sheet_lacks_comes_from_the_files_own_tags() {
    let dir = tempfile::tempdir().unwrap();
    let audio = fixture_copy(dir.path(), "live.flac");
    let mut tag = lofty::tag::Tag::new(lofty::tag::TagType::VorbisComments);
    tag.set_album("Tagged Album".into());
    tag.set_artist("Tagged Artist".into());
    tag.insert_text(lofty::tag::ItemKey::AlbumArtist, "Tagged Band".into());
    tag.set_date(lofty::tag::items::Timestamp {
        year: 1994,
        ..Default::default()
    });
    tag.save_to_path(&audio, lofty::config::WriteOptions::default())
        .unwrap();
    std::fs::write(
        dir.path().join("live.cue"),
        "FILE \"live.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"One\"\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    INDEX 01 00:00:40\n",
    )
    .unwrap();
    let db = crate::db::Db::open_in_memory().unwrap();

    scan(&db, dir.path());

    let rows: Vec<(String, String, String, String, Option<i32>)> = db
        .conn()
        .prepare(
            "SELECT title, artist, album, album_artist, year FROM tracks ORDER BY segment_index",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        rows,
        [
            (
                "One".into(),
                "Tagged Artist".into(),
                "Tagged Album".into(),
                "Tagged Band".into(),
                Some(1994)
            ),
            (
                "Track 02".into(),
                "Tagged Artist".into(),
                "Tagged Album".into(),
                "Tagged Band".into(),
                Some(1994)
            ),
        ]
    );
}

#[test]
fn a_second_scan_changes_nothing_and_does_not_read_the_file_again() {
    let album = Album::new();
    album.scan();
    let ids = ids_of(album.db.conn(), &album.audio());
    track_meta::READ_META_CALLS.with(|calls| calls.set(0));

    let report = album.scan();

    assert_eq!((report.added, report.updated), (0, 0));
    assert_eq!(report.skipped_unchanged, 1);
    assert_eq!(track_meta::READ_META_CALLS.with(std::cell::Cell::get), 0);
    assert_eq!(ids_of(album.db.conn(), &album.audio()), ids);
}

#[test]
fn cue_1a_a_sheet_that_arrives_after_its_audio_in_the_walk_still_splits_it() {
    let album = Album::new();
    let source = SheetsLastSource;

    let report = completed(scan_folder_with_source(&source, &album.db, album.dir.path()).unwrap());

    assert_eq!(report.added, 3);
    assert_eq!(row_count(album.db.conn()), 3);
}

/// Delivers every `.cue` entry after all the others, the order that would break
/// a scanner that needed the sheet before the audio it describes.
struct SheetsLastSource;

impl source::LibrarySource for SheetsLastSource {
    fn residence_token(&self, at: &Path) -> Option<i64> {
        UnixLibrarySource.residence_token(at)
    }
    fn mount_point(&self, at: &Path) -> Option<PathBuf> {
        UnixLibrarySource.mount_point(at)
    }
    fn display_name(&self, at: &Path) -> Option<String> {
        UnixLibrarySource.display_name(at)
    }
    fn container_name(&self, at: &Path) -> Option<String> {
        UnixLibrarySource.container_name(at)
    }
    fn relative_path(&self, root: &Path, at: &Path) -> Option<PathBuf> {
        UnixLibrarySource.relative_path(root, at)
    }
    fn open_read(&self, at: &Path) -> std::io::Result<source::LibraryReadHandle> {
        UnixLibrarySource.open_read(at)
    }
    fn probe(&self, at: &Path, links: LibraryLinkMode) -> LibraryPathPresence {
        UnixLibrarySource.probe(at, links)
    }
    fn read_directory(&self, directory: &Path) -> Option<Vec<source::LibraryDirectoryEntry>> {
        UnixLibrarySource.read_directory(directory)
    }
    fn walk(
        &self,
        root: &Path,
        order: source::LibraryWalkOrder,
        visitor: &mut dyn source::LibraryWalkVisitor,
    ) {
        struct Collect(Vec<source::LibraryWalkItem>);
        impl source::LibraryWalkVisitor for Collect {
            fn visit(&mut self, item: source::LibraryWalkItem) -> source::LibraryWalkControl {
                self.0.push(item);
                source::LibraryWalkControl::Continue
            }
        }
        let mut collected = Collect(Vec::new());
        UnixLibrarySource.walk(root, order, &mut collected);
        let is_sheet = |item: &source::LibraryWalkItem| {
            matches!(item, source::LibraryWalkItem::Entry(entry)
                if entry.path.extension().is_some_and(|extension| extension == "cue"))
        };
        let (sheets, rest): (Vec<_>, Vec<_>) = collected.0.into_iter().partition(is_sheet);
        for item in rest.into_iter().chain(sheets) {
            if visitor.visit(item) == source::LibraryWalkControl::Stop {
                return;
            }
        }
    }
}

#[test]
fn cue_1b_a_changed_sheet_resegments_and_keeps_the_rows_it_still_has() {
    let album = Album::new();
    album.scan();
    let before = ids_of(album.db.conn(), &album.audio());
    album
        .db
        .conn()
        .execute("UPDATE tracks SET rating = 4 WHERE segment_index = 2", [])
        .unwrap();

    album.rewrite_sheet(
        "FILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Opener\"\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    TITLE \"Closer\"\n    INDEX 01 00:15:00\n",
    );
    let report = album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(titles(&rows), ["Opener", "Closer"]);
    assert_eq!(rows[1].1, Some(15_000));
    assert_eq!(rows[1].2, Some(30_000));
    assert_eq!(
        ids_of(album.db.conn(), &album.audio()),
        before[..2],
        "kept rows keep their ids"
    );
    let rating: i64 = album
        .db
        .conn()
        .query_row(
            "SELECT rating FROM tracks WHERE segment_index = 2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rating, 4, "a track keeps its rating when the sheet changes");
    assert_eq!(report.updated, 2);
}

#[test]
fn cue_1b_removing_the_sheet_brings_back_the_single_track() {
    let album = Album::new();
    album.scan();

    std::fs::remove_file(album.sheet()).unwrap();
    let report = album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].0, rows[0].1, rows[0].2), (0, None, None));
    assert_eq!(report.updated, 1);
}

#[test]
fn cue_1b_a_sheet_added_beside_a_known_file_replaces_its_whole_file_track() {
    let album = Album::new();
    std::fs::rename(album.sheet(), album.dir.path().join("later.txt")).unwrap();
    album.scan();
    let whole = ids_of(album.db.conn(), &album.audio());
    assert_eq!(whole.len(), 1);

    std::fs::rename(album.dir.path().join("later.txt"), album.sheet()).unwrap();
    album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(rows.len(), 3);
    assert!(!ids_of(album.db.conn(), &album.audio()).contains(&whole[0]));
}

#[test]
fn cue_2_a_sheet_with_a_track_past_the_end_keeps_the_file_whole_and_raises_an_issue() {
    let album = Album::new();
    album.rewrite_sheet(
        "FILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    INDEX 01 10:00:00\n",
    );

    let report = album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, 0);
    assert_eq!(report.added, 1);
    let (kind, detail) = issue(album.db.conn(), &album.sheet()).expect("the sheet is reported");
    assert_eq!(kind, "invalid_cue_sheet");
    assert!(detail.contains("past"), "{detail}");
    assert_eq!(
        issue(album.db.conn(), &album.audio()),
        None,
        "keyed by the sheet"
    );
}

#[test]
fn cue_2_a_rejected_sheet_is_not_tried_again_until_it_changes() {
    let album = Album::new();
    album.rewrite_sheet(
        "FILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    INDEX 01 10:00:00\n",
    );
    album.scan();
    track_meta::READ_META_CALLS.with(|calls| calls.set(0));

    let report = album.scan();

    assert_eq!((report.added, report.updated), (0, 0));
    assert_eq!(report.skipped_unchanged, 1);
    assert_eq!(track_meta::READ_META_CALLS.with(std::cell::Cell::get), 0);
    assert!(issue(album.db.conn(), &album.sheet()).is_some());

    album.rewrite_sheet(THREE_TRACKS);
    album.scan();

    assert_eq!(segments_of(album.db.conn(), &album.audio()).len(), 3);
    assert_eq!(
        issue(album.db.conn(), &album.sheet()),
        None,
        "mending the sheet clears the issue"
    );
}

#[test]
fn cue_2_sheets_that_do_not_describe_the_audio_all_leave_it_whole() {
    let broken = [
        ("a file that is not there", "FILE \"missing.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n"),
        ("no audio track", "FILE \"album.wav\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n"),
        (
            "one file twice",
            "FILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\nFILE \"ALBUM.WAV\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n",
        ),
        ("not a sheet", "this is not a cue sheet at all"),
    ];
    for (why, text) in broken {
        let album = Album::new();
        album.rewrite_sheet(text);

        let report = album.scan();

        let rows = segments_of(album.db.conn(), &album.audio());
        assert_eq!(rows.len(), 1, "{why}: one ordinary track");
        assert_eq!(rows[0].0, 0, "{why}");
        assert_eq!(report.added, 1, "{why}");
        let (kind, _) = issue(album.db.conn(), &album.sheet())
            .unwrap_or_else(|| panic!("{why}: the sheet is reported"));
        assert_eq!(kind, "invalid_cue_sheet", "{why}");
    }
}

#[test]
fn a_sheet_over_several_files_gives_each_file_its_own_track() {
    let dir = tempfile::tempdir().unwrap();
    write_wav(&dir.path().join("01.wav"), 12);
    write_wav(&dir.path().join("02.wav"), 20);
    std::fs::write(
        dir.path().join("disc.cue"),
        "PERFORMER \"Band\"\nTITLE \"Disc\"\n\
         FILE \"01.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"First\"\n    INDEX 01 00:00:00\n\
         FILE \"02.wav\" WAVE\n  TRACK 02 AUDIO\n    TITLE \"Second\"\n    INDEX 01 00:00:00\n",
    )
    .unwrap();
    let db = crate::db::Db::open_in_memory().unwrap();

    let report = scan(&db, dir.path());

    assert_eq!(report.added, 2);
    let first = segments_of(db.conn(), &dir.path().join("01.wav"));
    let second = segments_of(db.conn(), &dir.path().join("02.wav"));
    assert_eq!(first, [(1, Some(0), Some(12_000), "First".to_string())]);
    assert_eq!(second, [(1, Some(0), Some(20_000), "Second".to_string())]);
    let numbers: Vec<i64> = db
        .conn()
        .prepare("SELECT track_no FROM tracks ORDER BY track_no")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(numbers, [1, 2]);
}

#[test]
fn cue_1a_a_sheet_embedded_in_a_flac_splits_it() {
    let dir = tempfile::tempdir().unwrap();
    let audio = fixture_copy(dir.path(), "embedded.flac");
    embed_sheet(
        &audio,
        "FILE \"CDImage.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"A\"\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    TITLE \"B\"\n    INDEX 01 00:00:40\n",
    );
    let db = crate::db::Db::open_in_memory().unwrap();

    let report = scan(&db, dir.path());

    let rows = segments_of(db.conn(), &audio);
    assert_eq!(titles(&rows), ["A", "B"]);
    assert_eq!(report.added, 2);
    let cue_path: Option<String> = db
        .conn()
        .query_row(
            "SELECT cue_path FROM tracks WHERE segment_index = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cue_path, None, "an embedded sheet has no path");

    let again = scan(&db, dir.path());
    assert_eq!(
        (again.added, again.updated, again.skipped_unchanged),
        (0, 0, 1)
    );
}

#[test]
fn a_broken_embedded_sheet_keeps_the_file_whole_and_is_reported_against_it() {
    let dir = tempfile::tempdir().unwrap();
    let audio = fixture_copy(dir.path(), "embedded.flac");
    embed_sheet(&audio, "TRACK 01 AUDIO\n  INDEX 01 99:00:00\n");
    let db = crate::db::Db::open_in_memory().unwrap();

    scan(&db, dir.path());

    assert_eq!(segments_of(db.conn(), &audio).len(), 1);
    assert_eq!(
        issue(db.conn(), &audio).map(|(kind, _)| kind).as_deref(),
        Some("invalid_cue_sheet")
    );
}

fn embed_sheet(path: &Path, sheet: &str) {
    let mut flac = lofty::flac::FlacFile::read_from(
        &mut std::fs::File::open(path).unwrap(),
        lofty::config::ParseOptions::new(),
    )
    .unwrap();
    if flac.vorbis_comments().is_none() {
        flac.set_vorbis_comments(lofty::ogg::tag::VorbisComments::new());
    }
    flac.vorbis_comments_mut()
        .unwrap()
        .insert("CUESHEET".to_string(), sheet.to_string());
    flac.save_to_path(path, lofty::config::WriteOptions::default())
        .unwrap();
}

#[test]
fn cue_3_a_moved_cue_album_keeps_every_track() {
    let album = Album::new();
    album.scan();
    let ids = ids_of(album.db.conn(), &album.audio());
    album
        .db
        .conn()
        .execute(
            "UPDATE tracks SET play_count = 9 WHERE segment_index = 3",
            [],
        )
        .unwrap();
    let moved = album.dir.path().join("moved");
    std::fs::create_dir(&moved).unwrap();
    std::fs::rename(album.audio(), moved.join("album.wav")).unwrap();
    std::fs::rename(album.sheet(), moved.join("album.cue")).unwrap();

    let report = album.scan();

    assert_eq!(report.moved, 1);
    assert_eq!(report.added, 0);
    let new_path = moved.join("album.wav");
    assert_eq!(ids_of(album.db.conn(), &new_path), ids);
    assert_eq!(row_count(album.db.conn()), 3);
    let plays: i64 = album
        .db
        .conn()
        .query_row(
            "SELECT play_count FROM tracks WHERE segment_index = 3",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(plays, 9);
}

#[test]
fn cue_4_a_track_removed_on_its_own_stays_out_while_its_siblings_stay() {
    let album = Album::new();
    album.scan();
    let removed: (i64, String) = album
        .db
        .conn()
        .query_row(
            "SELECT id, path FROM tracks WHERE segment_index = 2",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let tx = album.db.conn().unchecked_transaction().unwrap();
    assert!(
        crate::library::exclusions::record_track(&tx, removed.0, Path::new(&removed.1), 1).unwrap()
    );
    tx.execute("DELETE FROM tracks WHERE id = ?1", [removed.0])
        .unwrap();
    tx.commit().unwrap();

    album.rewrite_sheet(THREE_TRACKS);
    album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(titles(&rows), ["Disorder", "Candidate"]);
}

#[test]
fn cue_4_removing_the_whole_file_hides_all_its_tracks() {
    let album = Album::new();
    album.scan();
    let (id, path): (i64, String) = album
        .db
        .conn()
        .query_row(
            "SELECT id, path FROM tracks WHERE segment_index = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    // The whole file is excluded by identity, as removing a plain track does.
    album
        .db
        .conn()
        .execute(
            "INSERT INTO library_exclusions (path, device, inode, file_size, file_mtime, excluded_at, segment_index) \
             SELECT path, device, inode, file_size, file_mtime, 1, 0 FROM tracks WHERE id = ?1",
            [id],
        )
        .unwrap();
    album
        .db
        .conn()
        .execute("DELETE FROM tracks WHERE path = ?1", [&path])
        .unwrap();

    let report = album.scan();

    assert_eq!(row_count(album.db.conn()), 0);
    assert_eq!(report.excluded, 1);
}

#[test]
fn a_changed_audio_file_under_an_unchanged_sheet_is_cut_again() {
    let album = Album::new();
    album.scan();
    let before = ids_of(album.db.conn(), &album.audio());

    write_wav(&album.audio(), 40);
    bump_mtime(&album.audio());
    let report = album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(rows.last().map(|row| row.2), Some(Some(40_000)));
    assert_eq!(ids_of(album.db.conn(), &album.audio()), before);
    assert_eq!(report.updated, 3);
}

#[test]
fn cue_2_a_dismissed_sheet_issue_stays_quiet_until_the_sheet_changes() {
    let album = Album::new();
    album.rewrite_sheet("this is not a cue sheet at all");
    album.scan();
    let (mtime, size): (i64, i64) = {
        let metadata = std::fs::metadata(album.sheet()).unwrap();
        let mtime = metadata
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        (mtime as i64, metadata.len() as i64)
    };
    album
        .db
        .conn()
        .execute(
            "UPDATE import_errors SET dismissed_mtime = ?1, dismissed_size = ?2 WHERE path = ?3",
            rusqlite::params![mtime, size, album.sheet().to_string_lossy()],
        )
        .unwrap();
    let seen = |album: &Album| -> i64 {
        album
            .db
            .conn()
            .query_row("SELECT seen_count FROM import_errors", [], |row| row.get(0))
            .unwrap()
    };
    let before = seen(&album);

    album.scan();

    assert_eq!(
        seen(&album),
        before,
        "a dismissed issue is not raised again"
    );
}

#[test]
fn the_progress_estimate_counts_files_not_tracks() {
    let album = Album::new();
    album.scan();

    let estimate = scan_progress::estimated_audio_files(album.db.conn(), album.dir.path()).unwrap();

    assert_eq!(estimate, Some(1));
}

#[test]
fn cue_8_a_cue_file_that_disappears_marks_every_track_missing_and_its_return_restores_them() {
    let album = Album::new();
    album.scan();
    let hidden = album.dir.path().join("album.bak");
    std::fs::rename(album.audio(), &hidden).unwrap();
    std::fs::rename(album.sheet(), album.dir.path().join("album.cue.bak")).unwrap();

    let report = album.scan();

    assert_eq!(report.vanished, 3);
    let missing = |album: &Album| -> i64 {
        album
            .db
            .conn()
            .query_row(
                "SELECT count(*) FROM tracks WHERE missing_since IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap()
    };
    assert_eq!(missing(&album), 3);

    std::fs::rename(&hidden, album.audio()).unwrap();
    std::fs::rename(album.dir.path().join("album.cue.bak"), album.sheet()).unwrap();
    album.scan();

    assert_eq!(missing(&album), 0);
    assert_eq!(segments_of(album.db.conn(), &album.audio()).len(), 3);
}
