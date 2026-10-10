//! CUE-19: a scan does not take back the length the analysis learned for the
//! last track of a file, unless the file or the track's cut changed under it.

use super::{bump_mtime, segments_of, Album};

/// What the analysis stored once it decoded 35 s of a file whose metadata says
/// 30 s: the last track, starting at 20 s, lasts 15 s.
const LEARNED_DURATION_MS: i64 = 15_000;

fn learn_the_last_tracks_length(album: &Album) {
    album
        .db
        .conn()
        .execute(
            "UPDATE tracks SET duration_ms = ?1 WHERE segment_index = 3",
            [LEARNED_DURATION_MS],
        )
        .unwrap();
}

fn durations(album: &Album) -> Vec<i64> {
    let mut statement = album
        .db
        .conn()
        .prepare("SELECT duration_ms FROM tracks WHERE path = ?1 ORDER BY segment_index")
        .unwrap();
    statement
        .query_map([album.audio().to_string_lossy().to_string()], |row| {
            row.get(0)
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn sheet_with_the_first_track_renamed() -> String {
    super::THREE_TRACKS.replace("Disorder", "Opener")
}

#[test]
fn cue_19_a_scan_that_rewrites_the_rows_keeps_the_learned_length() {
    let album = Album::new();
    album.scan();
    assert_eq!(durations(&album), [10_000, 10_000, 10_000]);
    learn_the_last_tracks_length(&album);

    album.rewrite_sheet(&sheet_with_the_first_track_renamed());
    let report = album.scan();

    assert_eq!(report.updated, 3, "the edited sheet rewrote every row");
    assert_eq!(durations(&album), [10_000, 10_000, LEARNED_DURATION_MS]);
}

#[test]
fn cue_19_a_replaced_file_gives_back_the_length_its_metadata_claims() {
    let album = Album::new();
    album.scan();
    learn_the_last_tracks_length(&album);

    super::write_wav(&album.audio(), 40);
    bump_mtime(&album.audio());
    album.scan();

    assert_eq!(durations(&album), [10_000, 10_000, 20_000]);
}

#[test]
fn cue_19_a_cut_the_sheet_moved_gives_back_the_length_its_metadata_claims() {
    let album = Album::new();
    album.scan();
    learn_the_last_tracks_length(&album);

    album.rewrite_sheet(&super::THREE_TRACKS.replace("00:20:00", "00:25:00"));
    album.scan();

    let rows = segments_of(album.db.conn(), &album.audio());
    assert_eq!(rows[2].1, Some(25_000));
    assert_eq!(durations(&album), [10_000, 15_000, 5_000]);
}
