//! Which row a track of an edited sheet keeps: the one that holds the same song,
//! so its rating, its playlists and its listens follow it. And when the scan
//! notices that a sheet was edited at all.

use super::super::tests::row_count;
use super::{ids_of, segments_of, titles, Album};

/// `(id, rating)` of the row titled `title`.
fn row_titled(album: &Album, title: &str) -> (i64, i64) {
    album
        .db
        .conn()
        .query_row(
            "SELECT id, rating FROM tracks WHERE title = ?1",
            [title],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn rate(album: &Album, title: &str, rating: i64) {
    album
        .db
        .conn()
        .execute(
            "UPDATE tracks SET rating = ?2 WHERE title = ?1",
            rusqlite::params![title, rating],
        )
        .unwrap();
}

#[test]
fn cue_1b_a_track_split_in_two_moves_every_later_track_along_with_its_row() {
    let album = Album::new();
    album.scan();
    rate(&album, "Day of the Lords", 4);
    rate(&album, "Candidate", 5);
    let day = row_titled(&album, "Day of the Lords");
    let candidate = row_titled(&album, "Candidate");
    let before = ids_of(album.db.conn(), &album.audio());

    album.rewrite_sheet(
        "FILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Disorder\"\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    TITLE \"Day of the Lords\"\n    INDEX 01 00:10:00\n  \
         TRACK 03 AUDIO\n    TITLE \"Interlude\"\n    INDEX 01 00:15:00\n  \
         TRACK 04 AUDIO\n    TITLE \"Candidate\"\n    INDEX 01 00:20:00\n",
    );
    album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(
        titles(&rows),
        ["Disorder", "Day of the Lords", "Interlude", "Candidate"]
    );
    assert_eq!(row_titled(&album, "Day of the Lords"), day);
    assert_eq!(
        row_titled(&album, "Candidate"),
        candidate,
        "the song that moved to the fourth place keeps its row and rating"
    );
    let (interlude, rating) = row_titled(&album, "Interlude");
    assert!(!before.contains(&interlude), "the new track is a new row");
    assert_eq!(rating, 0);
    assert_eq!(row_count(album.db.conn()), 4);
}

#[test]
fn cue_1b_tracks_whose_titles_trade_places_keep_their_rows_by_title() {
    let album = Album::new();
    album.scan();
    rate(&album, "Day of the Lords", 4);
    rate(&album, "Candidate", 5);
    let day = row_titled(&album, "Day of the Lords");
    let candidate = row_titled(&album, "Candidate");

    album.rewrite_sheet(
        "FILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Disorder\"\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    TITLE \"Candidate\"\n    INDEX 01 00:10:00\n  \
         TRACK 03 AUDIO\n    TITLE \"Day of the Lords\"\n    INDEX 01 00:20:00\n",
    );
    album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(titles(&rows), ["Disorder", "Candidate", "Day of the Lords"]);
    assert_eq!(
        row_titled(&album, "Candidate"),
        candidate,
        "the title wins over the start when the two disagree"
    );
    assert_eq!(row_titled(&album, "Day of the Lords"), day);
    assert_eq!(rows[1].1, Some(10_000));
    assert_eq!(rows[2].1, Some(20_000));
}

#[test]
fn cue_1b_a_sheet_rewritten_without_a_new_mtime_is_still_read_again() {
    let album = Album::new();
    album.scan();
    let mtime = std::fs::metadata(album.sheet())
        .unwrap()
        .modified()
        .unwrap();

    std::fs::write(
        album.sheet(),
        "FILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Opener\"\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    TITLE \"Closer\"\n    INDEX 01 00:15:00\n",
    )
    .unwrap();
    std::fs::File::options()
        .write(true)
        .open(album.sheet())
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(titles(&rows), ["Opener", "Closer"]);
}
