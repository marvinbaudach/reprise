//! CUE sheets across several files, embedded in a FLAC, moved, removed from the
//! library and vanished: the cases that go beyond one sheet beside one file.

use std::path::Path;

use super::cue_tests::{
    bump_mtime, ids_of, issue, scan, segments_of, titles, write_wav, Album, THREE_TRACKS,
};
use super::tests::{fixture_copy, row_count};
use super::*;
use lofty::prelude::*;

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

pub(super) fn embed_sheet(path: &Path, sheet: &str) {
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
fn cue_3_a_cue_album_copied_to_another_place_keeps_every_track() {
    let album = Album::new();
    album.scan();
    let ids = ids_of(album.db.conn(), &album.audio());
    album
        .db
        .conn()
        .execute("UPDATE tracks SET rating = 5 WHERE segment_index = 2", [])
        .unwrap();
    // A copy and a delete, as a move to another filesystem is: the file keeps
    // its bytes but not its inode.
    let moved = album.dir.path().join("moved");
    std::fs::create_dir(&moved).unwrap();
    std::fs::copy(album.audio(), moved.join("album.wav")).unwrap();
    std::fs::copy(album.sheet(), moved.join("album.cue")).unwrap();
    std::fs::remove_file(album.audio()).unwrap();
    std::fs::remove_file(album.sheet()).unwrap();

    let report = album.scan();

    assert_eq!((report.moved, report.added), (1, 0));
    assert_eq!(ids_of(album.db.conn(), &moved.join("album.wav")), ids);
    let rating: i64 = album
        .db
        .conn()
        .query_row(
            "SELECT rating FROM tracks WHERE segment_index = 2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rating, 5);
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
