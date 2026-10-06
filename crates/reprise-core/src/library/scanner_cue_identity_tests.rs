//! Which row a track of an edited sheet keeps: the one that holds the same song,
//! so its rating, its playlists and its listens follow it. And when the scan
//! notices that a sheet was edited at all.

use super::{segments_of, titles, Album};

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
