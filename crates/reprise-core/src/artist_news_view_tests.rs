use chrono::NaiveDate;

use crate::artist_news::{
    count_releases_view, filter_release_rows, persisted_releases_filter, query_releases_view,
    release_status, sort_release_rows, LibraryPresence, ReleaseSortDirection, ReleaseStatus,
    ReleaseTypeFilter, ReleasesFilter,
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

#[test]
fn nr_17_release_status_distinguishes_upcoming_incomplete_and_missing() {
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
fn nr_16_release_filters_always_exclude_complete_releases_and_singles() {
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
        release_type: Some(ReleaseTypeFilter::Ep),
        hidden: false,
    };
    assert_eq!(
        filter_release_rows(rows.clone(), &filter)
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
                ..filter
            },
        )
        .into_iter()
        .map(|row| row.release_group_mbid)
        .collect::<Vec<_>>(),
        ["hidden"]
    );
    assert_eq!(
        filter_release_rows(rows.clone(), &ReleasesFilter::default())
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
fn persisted_release_filter_tolerates_unknown_type() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    set_bool(&conn, "releases.filter.not_in_library", true).unwrap();
    set_bool(&conn, "releases.filter.hidden", true).unwrap();
    set_setting(&conn, "releases.filter.type", "unexpected").unwrap();

    assert_eq!(
        persisted_releases_filter(&conn).unwrap(),
        ReleasesFilter {
            release_type: None,
            hidden: true,
        }
    );
}

#[test]
fn nr_18_sidebar_badge_count_matches_visible_gap_rows() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, added_at)
         VALUES ('/music/one.flac', 'Track', 'Artist', 'Local', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, fallback_accent, first_seen
         ) VALUES
           ('one', 'Artist', 'artist-id', 'One', 'Album', '2026-08-01', 1, '#123456', 1),
           ('two', 'Artist', 'artist-id', 'Two', 'Single', '2026-08-02', 1, '#123456', 1)",
        [],
    )
    .unwrap();
    let filter = ReleasesFilter {
        release_type: Some(ReleaseTypeFilter::Album),
        ..ReleasesFilter::default()
    };

    let rows = query_releases_view(&conn, &filter, today()).unwrap();
    assert_eq!(
        count_releases_view(&conn, &filter, today()).unwrap(),
        rows.len() as i64
    );
    assert_eq!(rows[0].release_group_mbid, "one");
}

#[test]
fn nr_16_releases_view_is_limited_to_current_library_artists() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album_artist, album, added_at)
         VALUES ('/music/local.flac', 'Track', 'Guest', 'Library Artist', 'Local', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, fallback_accent, first_seen
         ) VALUES
           ('local', 'Library Artist', 'artist-id', 'Missing Album', 'Album',
            '2020-01-01', 1, '#123456', 1),
           ('foreign', 'Former Artist', 'former-id', 'Foreign Album', 'Album',
            '2020-01-01', 1, '#123456', 1)",
        [],
    )
    .unwrap();

    let rows = query_releases_view(&conn, &ReleasesFilter::default(), today()).unwrap();
    assert_eq!(
        rows.into_iter()
            .map(|row| row.release_group_mbid)
            .collect::<Vec<_>>(),
        ["local"]
    );
}
