use chrono::NaiveDate;

use crate::artist_news::{
    count_releases_view, filter_release_rows, persisted_releases_filter, query_releases_view,
    query_releases_view_scope, release_status, sort_release_rows, LibraryPresence,
    ReleaseSortDirection, ReleaseStatus, ReleaseTypeSelection, ReleaseWindow, ReleasesFilter,
};
use crate::artist_news_history::HistoryEntry;
use crate::library::settings::{set_bool, set_setting};

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 7, 25).unwrap()
}

fn entry(
    mbid: &str,
    title: &str,
    release_type: &str,
    date: &str,
    presence: LibraryPresence,
    hidden: bool,
) -> HistoryEntry {
    HistoryEntry {
        release_group_mbid: mbid.to_string(),
        artist_name: "Artist".to_string(),
        title: title.to_string(),
        release_type: release_type.to_string(),
        first_release_date: date.to_string(),
        first_seen: Some(1),
        seen_at: None,
        hidden,
        hidden_at: hidden.then_some(2),
        presence,
        announce_url: None,
        track_count: None,
        local_track_count: 0,
    }
}

fn with_counts(mut entry: HistoryEntry, local: i64, official: Option<i64>) -> HistoryEntry {
    entry.local_track_count = local;
    entry.track_count = official;
    entry
}

fn with_artist(mut entry: HistoryEntry, artist: &str) -> HistoryEntry {
    entry.artist_name = artist.to_string();
    entry
}

#[test]
fn nr_24_majority_coverage_hides_a_released_album() {
    let rows = vec![
        with_counts(
            entry(
                "majority",
                "Majority",
                "Album",
                "2026-01-01",
                LibraryPresence::Partial,
                false,
            ),
            7,
            Some(12),
        ),
        with_counts(
            entry(
                "half",
                "Half",
                "Album",
                "2026-01-01",
                LibraryPresence::Partial,
                false,
            ),
            6,
            Some(12),
        ),
    ];

    let visible = filter_release_rows(rows, &ReleasesFilter::default(), today());

    assert_eq!(
        visible
            .into_iter()
            .map(|row| row.release_group_mbid)
            .collect::<Vec<_>>(),
        ["half"]
    );
}

#[test]
fn nr_24_unknown_official_count_never_counts_as_owned() {
    let row = with_counts(
        entry(
            "unknown",
            "Unknown",
            "Album",
            "2026-01-01",
            LibraryPresence::Partial,
            false,
        ),
        99,
        None,
    );

    assert_eq!(
        filter_release_rows(vec![row], &ReleasesFilter::default(), today()).len(),
        1
    );
}

#[test]
fn nr_24_upcoming_release_is_never_owned_by_advance_singles() {
    let row = with_counts(
        entry(
            "future",
            "Future",
            "Album",
            "2026-08-01",
            LibraryPresence::Partial,
            false,
        ),
        7,
        Some(8),
    );

    assert_eq!(
        filter_release_rows(vec![row], &ReleasesFilter::default(), today()).len(),
        1
    );
}

#[test]
fn nr_24_single_is_owned_through_a_matching_track_title() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks (path, title, artist, album_artist, album, added_at)
             VALUES ('/music/song.flac', 'Standalone Song', 'Guest', 'Release Artist',
                     'Later Album', 0)",
            [],
        )
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, first_seen
             ) VALUES ('single', 'Release Artist', 'artist-id', 'Standalone Song',
                       'Single', '2026-01-01', 1, 1)",
            [],
        )
        .unwrap();

    let rows = query_releases_view(
        &db,
        &ReleasesFilter {
            release_types: ReleaseTypeSelection {
                album: false,
                ep: false,
                single: true,
            },
            ..ReleasesFilter::default()
        },
        today(),
    )
    .unwrap();

    assert!(rows.is_empty());
}

#[test]
fn nr_25_default_window_hides_releases_older_than_five_years() {
    let rows = vec![
        entry(
            "recent",
            "Recent",
            "Album",
            "2021-07-25",
            LibraryPresence::Absent,
            false,
        ),
        entry(
            "old",
            "Old",
            "Album",
            "2021-07-24",
            LibraryPresence::Absent,
            false,
        ),
    ];

    let visible = filter_release_rows(rows, &ReleasesFilter::default(), today());

    assert_eq!(visible[0].release_group_mbid, "recent");
    assert_eq!(visible.len(), 1);
}

#[test]
fn nr_25_singles_are_absent_until_their_chip_is_on() {
    let rows = vec![entry(
        "single",
        "Single",
        "Single",
        "2026-01-01",
        LibraryPresence::Absent,
        false,
    )];
    assert!(filter_release_rows(rows.clone(), &ReleasesFilter::default(), today()).is_empty());
    let filter = ReleasesFilter {
        release_types: ReleaseTypeSelection {
            album: true,
            ep: true,
            single: true,
        },
        ..ReleasesFilter::default()
    };

    assert_eq!(filter_release_rows(rows, &filter, today()).len(), 1);
}

#[test]
fn nr_25_undated_release_survives_every_window() {
    let rows = vec![entry(
        "undated",
        "Undated",
        "Album",
        "unknown",
        LibraryPresence::Absent,
        false,
    )];
    for window in [
        ReleaseWindow::OneYear,
        ReleaseWindow::FiveYears,
        ReleaseWindow::TenYears,
        ReleaseWindow::All,
    ] {
        let filter = ReleasesFilter {
            window,
            ..ReleasesFilter::default()
        };
        assert_eq!(filter_release_rows(rows.clone(), &filter, today()).len(), 1);
    }
}

#[test]
fn nr_25_window_all_shows_the_full_catalog() {
    let filter = ReleasesFilter {
        window: ReleaseWindow::All,
        ..ReleasesFilter::default()
    };
    let rows = vec![entry(
        "old",
        "Old",
        "Album",
        "1975-01-01",
        LibraryPresence::Absent,
        false,
    )];

    assert_eq!(filter_release_rows(rows, &filter, today()).len(), 1);
}

#[test]
fn nr_25_empty_type_selection_shows_every_type() {
    let filter = ReleasesFilter {
        release_types: ReleaseTypeSelection::empty(),
        ..ReleasesFilter::default()
    };
    let rows = ["Album", "EP", "Single"]
        .into_iter()
        .map(|release_type| {
            entry(
                release_type,
                release_type,
                release_type,
                "2026-01-01",
                LibraryPresence::Absent,
                false,
            )
        })
        .collect();

    assert_eq!(filter_release_rows(rows, &filter, today()).len(), 3);
}

#[test]
fn nr_25_all_selected_types_with_all_window_is_the_widest_scope() {
    let filter = ReleasesFilter {
        release_types: ReleaseTypeSelection::all(),
        window: ReleaseWindow::All,
        hidden: false,
    };

    assert!(filter.is_widest());
}

#[test]
fn nr_25_count_line_never_exceeds_its_total() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks (path, title, artist, album_artist, album, added_at)
             VALUES ('/music/local.flac', 'Local', 'Artist', 'Artist', 'Local', 0)",
            [],
        )
        .unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, first_seen
             ) VALUES
               ('recent', 'Artist', 'artist-id', 'Recent', 'Album', '2026-01-01', 1, 1),
               ('old', 'Artist', 'artist-id', 'Old', 'EP', '2010-01-01', 1, 1),
               ('single', 'Artist', 'artist-id', 'Other Single', 'Single',
                '2026-02-01', 1, 1);",
        )
        .unwrap();

    let result = query_releases_view_scope(&db, &ReleasesFilter::default(), today()).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.widest_total, 3);
    assert!(result.rows.len() <= result.widest_total);
}

#[test]
fn nr_24_duplicate_album_and_ep_collapse_to_the_album() {
    let rows = vec![
        with_artist(
            entry(
                "album",
                "Visions of Inner Depth",
                "Album",
                "2018-05-11",
                LibraryPresence::Absent,
                false,
            ),
            "By the Thousands",
        ),
        with_artist(
            entry(
                "ep",
                "Visions of Inner Depth",
                "EP",
                "2018-05-11",
                LibraryPresence::Absent,
                false,
            ),
            "By the Thousands",
        ),
    ];
    let filter = ReleasesFilter::widest(false);

    let visible = filter_release_rows(rows, &filter, today());

    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].release_group_mbid, "album");
}

#[test]
fn nr_24_same_title_on_a_different_date_is_not_a_duplicate() {
    let rows = vec![
        entry(
            "original",
            "Same Title",
            "Album",
            "2018-05-11",
            LibraryPresence::Absent,
            false,
        ),
        entry(
            "rerecording",
            "Same Title",
            "Album",
            "2025-05-11",
            LibraryPresence::Absent,
            false,
        ),
    ];

    assert_eq!(
        filter_release_rows(rows, &ReleasesFilter::widest(false), today()).len(),
        2
    );
}

#[test]
fn duplicate_ties_prefer_a_known_count_then_the_smallest_mbid() {
    let unknown = entry(
        "a-unknown",
        "Same",
        "Album",
        "2025-01-01",
        LibraryPresence::Absent,
        false,
    );
    let known_later = with_counts(
        entry(
            "z-known",
            "Same",
            "Album",
            "2025-01-01",
            LibraryPresence::Absent,
            false,
        ),
        0,
        Some(10),
    );
    let known_first = with_counts(
        entry(
            "b-known",
            "Same",
            "Album",
            "2025-01-01",
            LibraryPresence::Absent,
            false,
        ),
        0,
        Some(10),
    );

    let rows = filter_release_rows(
        vec![unknown, known_later, known_first],
        &ReleasesFilter::widest(false),
        today(),
    );

    assert_eq!(rows[0].release_group_mbid, "b-known");
}

#[test]
fn nr_25_release_status_distinguishes_upcoming_incomplete_and_missing() {
    assert_eq!(
        release_status(
            &entry(
                "owned",
                "Owned",
                "Album",
                "2027",
                LibraryPresence::Complete,
                false,
            ),
            today(),
        ),
        ReleaseStatus::InLibrary
    );
    assert_eq!(
        release_status(
            &entry(
                "future",
                "Future",
                "Album",
                "2026-08",
                LibraryPresence::Absent,
                false,
            ),
            today(),
        ),
        ReleaseStatus::Upcoming
    );
    assert_eq!(
        release_status(
            &entry(
                "broken",
                "Broken",
                "Album",
                "unknown",
                LibraryPresence::Partial,
                false,
            ),
            today(),
        ),
        ReleaseStatus::Incomplete
    );
    assert_eq!(
        release_status(
            &entry(
                "missing",
                "Missing",
                "Album",
                "2026-01-01",
                LibraryPresence::Absent,
                false,
            ),
            today(),
        ),
        ReleaseStatus::Missing
    );
}

#[test]
fn nr_24_release_filters_exclude_owned_releases_and_unselected_singles() {
    let rows = vec![
        entry(
            "owned",
            "Owned",
            "Album",
            "2026-07-01",
            LibraryPresence::Complete,
            false,
        ),
        entry(
            "ep",
            "Visible EP",
            "EP",
            "2026-07-02",
            LibraryPresence::Partial,
            false,
        ),
        entry(
            "hidden",
            "Hidden EP",
            "ep",
            "2026-07-03",
            LibraryPresence::Absent,
            true,
        ),
        entry(
            "single",
            "Visible Single",
            "Single",
            "2026-07-04",
            LibraryPresence::Absent,
            false,
        ),
    ];
    let filter = ReleasesFilter {
        release_types: ReleaseTypeSelection {
            album: false,
            ep: true,
            single: false,
        },
        hidden: false,
        ..ReleasesFilter::default()
    };
    assert_eq!(
        filter_release_rows(rows.clone(), &filter, today())
            .into_iter()
            .map(|row| row.release_group_mbid)
            .collect::<Vec<_>>(),
        ["ep"]
    );
    assert_eq!(
        filter_release_rows(
            rows.clone(),
            &ReleasesFilter {
                hidden: true,
                ..filter.clone()
            },
            today(),
        )
        .into_iter()
        .map(|row| row.release_group_mbid)
        .collect::<Vec<_>>(),
        ["hidden"]
    );
    assert_eq!(
        filter_release_rows(rows.clone(), &ReleasesFilter::default(), today())
            .into_iter()
            .map(|row| row.release_group_mbid)
            .collect::<Vec<_>>(),
        ["ep"],
        "owned releases and singles never belong to the discography-gap view"
    );
}

#[test]
fn release_sort_keeps_invalid_dates_last_and_uses_title_tiebreak() {
    let rows = vec![
        entry(
            "b",
            "Beta",
            "Album",
            "2026-05",
            LibraryPresence::Absent,
            false,
        ),
        entry(
            "invalid",
            "Invalid",
            "Album",
            "unknown",
            LibraryPresence::Absent,
            false,
        ),
        entry(
            "a",
            "Alpha",
            "Album",
            "2026-05",
            LibraryPresence::Absent,
            false,
        ),
        entry(
            "new",
            "Newest",
            "Album",
            "2026-06",
            LibraryPresence::Absent,
            false,
        ),
    ];
    let sorted = sort_release_rows(rows, ReleaseSortDirection::Descending);
    assert_eq!(
        sorted
            .into_iter()
            .map(|row| row.release_group_mbid)
            .collect::<Vec<_>>(),
        ["new", "a", "b", "invalid"]
    );
}

#[test]
fn persisted_release_filter_defaults_unknown_values_and_reads_type_combinations() {
    let db = crate::db::Db::open_in_memory().unwrap();
    set_bool(&db, "releases.filter.not_in_library", true).unwrap();
    set_bool(&db, "releases.filter.hidden", true).unwrap();
    set_setting(&db, "releases.filter.type", "unexpected").unwrap();
    set_setting(&db, "releases.filter.window", "unexpected").unwrap();

    assert_eq!(
        persisted_releases_filter(&db).unwrap(),
        ReleasesFilter {
            release_types: ReleaseTypeSelection::default(),
            window: ReleaseWindow::FiveYears,
            hidden: true,
        }
    );

    set_setting(&db, "releases.filter.type", "album,single").unwrap();
    set_setting(&db, "releases.filter.window", "10y").unwrap();
    assert_eq!(
        persisted_releases_filter(&db).unwrap(),
        ReleasesFilter {
            release_types: ReleaseTypeSelection {
                album: true,
                ep: false,
                single: true,
            },
            window: ReleaseWindow::TenYears,
            hidden: true,
        }
    );
}

#[test]
fn nr_26_badge_follows_the_window_filter() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks (path, title, artist, album, added_at)
         VALUES ('/music/one.flac', 'Track', 'Artist', 'Local', 0)",
            [],
        )
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, first_seen
         ) VALUES
           ('recent', 'Artist', 'artist-id', 'Recent', 'Album', '2026-07-01', 1, 1),
           ('old', 'Artist', 'artist-id', 'Old', 'Album', '2010-01-01', 1, 1)",
            [],
        )
        .unwrap();
    let filter = ReleasesFilter {
        release_types: ReleaseTypeSelection {
            album: true,
            ep: false,
            single: false,
        },
        ..ReleasesFilter::default()
    };

    let rows = query_releases_view(&db, &filter, today()).unwrap();
    assert_eq!(
        count_releases_view(&db, &filter, today()).unwrap(),
        rows.len() as i64
    );
    assert_eq!(rows[0].release_group_mbid, "recent");

    let widest = ReleasesFilter::widest(false);
    assert_eq!(count_releases_view(&db, &widest, today()).unwrap(), 2);
}

#[test]
fn nr_24_releases_view_is_limited_to_current_library_artists() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks (path, title, artist, album_artist, album, added_at)
         VALUES ('/music/local.flac', 'Track', 'Guest', 'Library Artist', 'Local', 0)",
            [],
        )
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, first_seen
         ) VALUES
           ('local', 'Library Artist', 'artist-id', 'Missing Album', 'Album',
            '2020-01-01', 1, 1),
           ('foreign', 'Former Artist', 'former-id', 'Foreign Album', 'Album',
            '2020-01-01', 1, 1)",
            [],
        )
        .unwrap();

    let rows = query_releases_view(&db, &ReleasesFilter::widest(false), today()).unwrap();
    assert_eq!(
        rows.into_iter()
            .map(|row| row.release_group_mbid)
            .collect::<Vec<_>>(),
        ["local"]
    );
}
