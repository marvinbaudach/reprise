use crate::browse::WindowRange;
use crate::cue_album_test_support::cue_album;
use reprise_core::queries;

const WINDOW: WindowRange = WindowRange {
    offset: 0,
    limit: 50,
};

#[test]
fn mtp_66_each_track_of_a_cue_file_is_its_own_row_and_the_last_runs_to_the_end() {
    let album = cue_album();

    let rows = album
        .library
        .list_album_tracks("Unknown Pleasures".into(), "Joy Division".into(), WINDOW)
        .unwrap()
        .rows;

    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        album.track_ids
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.title.as_str())
            .collect::<Vec<_>>(),
        ["Disorder", "Day of the Lords", "Candidate"]
    );
    assert!(rows.iter().all(|row| row.uri == album.path));
    assert_eq!(
        rows.iter()
            .map(|row| (row.segment_start_ms, row.segment_end_ms))
            .collect::<Vec<_>>(),
        [
            (Some(0), Some(10_000)),
            (Some(10_000), Some(20_000)),
            (Some(20_000), None),
        ]
    );
}

#[test]
fn mtp_68_a_single_row_of_the_last_track_has_no_end_either() {
    let album = cue_album();

    let last = album
        .library
        .track_by_id(album.track_ids[2])
        .unwrap()
        .unwrap();
    let middle = album
        .library
        .track_by_id(album.track_ids[1])
        .unwrap()
        .unwrap();

    assert_eq!(
        (last.segment_start_ms, last.segment_end_ms),
        (Some(20_000), None)
    );
    assert_eq!(
        (middle.segment_start_ms, middle.segment_end_ms),
        (Some(10_000), Some(20_000))
    );
}

#[test]
fn mtp_66_a_track_whose_successor_is_only_excluded_still_plays_to_its_own_end() {
    let album = cue_album();
    // The user removes the last track from the library; it keeps its place in
    // the sheet as an exclusion, but no row.
    let database = reprise_core::db::Db::open_migrated(Some(
        &album._directory.path().join(crate::DATABASE_FILE_NAME),
    ))
    .unwrap();
    queries::exclude_tracks_matching_paths(
        &database,
        &[(album.track_ids[2], std::path::PathBuf::from(&album.path))],
        0,
    )
    .unwrap();
    drop(database);

    let second = album
        .library
        .track_by_id(album.track_ids[1])
        .unwrap()
        .unwrap();

    assert_eq!(
        (second.segment_start_ms, second.segment_end_ms),
        (Some(10_000), Some(20_000)),
        "the removed track's audio is not part of the second"
    );
}
