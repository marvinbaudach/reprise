use chrono::NaiveDate;

use crate::artist_news::{
    artist_search_url, artists_for_fetch, mark_releases_seen, most_played_album_track_path,
    parse_artist_mbid, parse_release_groups, query_releases, refresh_with, release_groups_url,
    unseen_release_count, ArtistMatch, FetchScope, NewsKind,
};

const ARTIST_ID: &str = "83d91898-7763-47d7-b03b-b92132375c47";
const SEARCH_EXACT: &str = r#"{"artists":[{"id":"83d91898-7763-47d7-b03b-b92132375c47","name":"Pink Floyd","score":100}]}"#;
const RELEASES: &str = r#"{"release-groups":[
  {"id":"1","title":"Future Album","first-release-date":"2027-01-10","primary-type":"Album","secondary-types":[]},
  {"id":"2","title":"Recent EP","first-release-date":"2026-06","primary-type":"EP","secondary-types":[]}
]}"#;

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 7, 13).unwrap()
}

#[test]
fn urls_encode_artist_and_bound_release_group_browse() {
    let search = artist_search_url("AC/DC & Friends");
    assert!(search.contains("artist%3A%22AC%2FDC%20%26%20Friends%22"));
    assert!(!search.contains("AC/DC"));
    let hostile = artist_search_url("A\\\" OR id:*");
    assert!(!hostile.contains(" OR "));
    assert!(hostile.contains("%5C%5C%5C%22"));
    assert_eq!(
        release_groups_url(ARTIST_ID),
        format!("https://musicbrainz.org/ws/2/release-group?artist={ARTIST_ID}&type=album%7Cep%7Csingle&release-group-status=website-default&limit=100&fmt=json")
    );
}

#[test]
fn artist_match_requires_one_exact_high_score_candidate() {
    assert_eq!(
        parse_artist_mbid(SEARCH_EXACT, "  pink   floyd "),
        ArtistMatch::Found(ARTIST_ID.to_string())
    );
    let weak = r#"{"artists":[{"id":"weak","name":"Pink Floyd","score":94}]}"#;
    assert_eq!(parse_artist_mbid(weak, "Pink Floyd"), ArtistMatch::NotFound);
    let inexact = r#"{"artists":[{"id":"other","name":"Pink Floyd Tribute","score":100}]}"#;
    assert_eq!(
        parse_artist_mbid(inexact, "Pink Floyd"),
        ArtistMatch::NotFound
    );
}

#[test]
fn artist_match_rejects_ambiguous_exact_candidates_and_bad_json() {
    let ambiguous = r#"{"artists":[
      {"id":"one","name":"Same Name","score":100},
      {"id":"two","name":"same name","score":99}
    ]}"#;
    assert_eq!(
        parse_artist_mbid(ambiguous, "Same Name"),
        ArtistMatch::Ambiguous
    );
    assert_eq!(
        parse_artist_mbid("not json", "Artist"),
        ArtistMatch::NotFound
    );
}

#[test]
fn release_parser_keeps_regular_albums_and_eps_but_not_local_albums() {
    let items = parse_release_groups(RELEASES, &[" recent   ep ".into()], date());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Future Album");
    assert_eq!(items[0].kind, NewsKind::Upcoming);
}

#[test]
fn release_parser_excludes_secondary_and_non_album_types() {
    let json = r#"{"release-groups":[
      {"id":"1","title":"Live Album","first-release-date":"2026-07-01","primary-type":"Album","secondary-types":["Live"]},
      {"id":"2","title":"Remix EP","first-release-date":"2026-07-01","primary-type":"EP","secondary-types":["Remix"]},
      {"id":"3","title":"Single","first-release-date":"2026-07-01","primary-type":"Single","secondary-types":[]},
      {"id":"4","title":"Regular","first-release-date":"2026-07-01","primary-type":"Album","secondary-types":[]}
    ]}"#;
    let items = parse_release_groups(json, &[], date());
    assert_eq!(
        items
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>(),
        ["Regular"]
    );
}

#[test]
fn date_filter_keeps_current_albums_and_ignores_missing_dates() {
    let json = r#"{"release-groups":[
      {"id":"today","title":"Today","first-release-date":"2026-07-13","primary-type":"Album"},
      {"id":"future","title":"Future Edge","first-release-date":"2027-07-13","primary-type":"Album"},
      {"id":"recent","title":"Recent","first-release-date":"2026-04-15","primary-type":"Album"},
      {"id":"too-old","title":"Too Old","first-release-date":"2026-04-13","primary-type":"Album"},
      {"id":"unknown","title":"Unknown","primary-type":"Album"}
    ]}"#;
    let items = parse_release_groups(json, &[], date());
    assert_eq!(items.len(), 3);
    assert!(items
        .iter()
        .any(|item| item.title == "Today" && item.kind == NewsKind::Upcoming));
    assert!(items.iter().any(|item| item.title == "Future Edge"));
    assert!(items.iter().any(|item| item.title == "Recent"));
}

#[test]
fn nr_1_album_and_ep_window_starts_ninety_days_ago() {
    let json = r#"{"release-groups":[
      {"id":"album-edge","title":"Album Edge","first-release-date":"2026-04-14","primary-type":"Album"},
      {"id":"ep-edge","title":"EP Edge","first-release-date":"2026-04-14","primary-type":"EP"},
      {"id":"too-old","title":"Too Old","first-release-date":"2026-04-13","primary-type":"Album"},
      {"id":"future","title":"Distant Future","first-release-date":"2028-01-01","primary-type":"Album"}
    ]}"#;

    let titles = parse_release_groups(json, &[], date())
        .into_iter()
        .map(|item| item.title)
        .collect::<Vec<_>>();

    assert_eq!(titles, ["Distant Future", "Album Edge", "EP Edge"]);
}

#[test]
fn nr_1_singles_require_a_complete_future_date() {
    let json = r#"{"release-groups":[
      {"id":"future","title":"Future Single","first-release-date":"2026-07-14","primary-type":"Single"},
      {"id":"today","title":"Today Single","first-release-date":"2026-07-13","primary-type":"Single"},
      {"id":"past","title":"Past Single","first-release-date":"2026-07-12","primary-type":"Single"},
      {"id":"month","title":"Month Single","first-release-date":"2026-08","primary-type":"Single"},
      {"id":"year","title":"Year Single","first-release-date":"2027","primary-type":"Single"}
    ]}"#;

    let titles = parse_release_groups(json, &[], date())
        .into_iter()
        .map(|item| item.title)
        .collect::<Vec<_>>();

    assert_eq!(titles, ["Future Single"]);
}

#[test]
fn nr_1_secondary_types_are_excluded_before_the_five_item_cap() {
    let json = r#"{"release-groups":[
      {"id":"live","title":"Live","first-release-date":"2026-08-01","primary-type":"Album","secondary-types":["Live"]},
      {"id":"one","title":"One","first-release-date":"2026-08-01","primary-type":"Album"},
      {"id":"two","title":"Two","first-release-date":"2026-08-02","primary-type":"EP"},
      {"id":"three","title":"Three","first-release-date":"2026-08-03","primary-type":"Single"},
      {"id":"four","title":"Four","first-release-date":"2026-08-04","primary-type":"Album"},
      {"id":"five","title":"Five","first-release-date":"2026-08-05","primary-type":"EP"},
      {"id":"six","title":"Six","first-release-date":"2026-08-06","primary-type":"Album"}
    ]}"#;

    let items = parse_release_groups(json, &[], date());

    assert_eq!(items.len(), 5);
    assert!(items.iter().all(|item| item.title != "Live"));
    assert!(items.iter().all(|item| item.title != "Six"));
}

#[test]
fn releases_sort_upcoming_ascending_then_new_descending_and_cap_at_five() {
    let json = r#"{"release-groups":[
      {"id":"u3","title":"Upcoming 3","first-release-date":"2026-10-01","primary-type":"Album"},
      {"id":"n1","title":"New 1","first-release-date":"2026-06-01","primary-type":"Album"},
      {"id":"u1","title":"Upcoming 1","first-release-date":"2026-08-01","primary-type":"Album"},
      {"id":"n3","title":"New 3","first-release-date":"2026-04-01","primary-type":"Album"},
      {"id":"u2","title":"Upcoming 2","first-release-date":"2026-09-01","primary-type":"Album"},
      {"id":"n2","title":"New 2","first-release-date":"2026-05-01","primary-type":"Album"}
    ]}"#;
    let titles = parse_release_groups(json, &[], date())
        .into_iter()
        .map(|item| item.title)
        .collect::<Vec<_>>();
    assert_eq!(
        titles,
        ["Upcoming 1", "Upcoming 2", "Upcoming 3", "New 1", "New 2"]
    );
}

fn migrated_conn() -> rusqlite::Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

fn no_accent(_conn: &rusqlite::Connection, _artist: &str) -> Option<String> {
    None
}

#[test]
fn nr_1_tag_mbid_skips_search_and_persists_releases() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, album, play_count, added_at) \
         VALUES ('/music/one.flac', 'One', 'Pink Floyd', ?1, 'Local Album', 20, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let mut urls = Vec::new();
    let mut fetch = |url: &str| {
        urls.push(url.to_string());
        Ok(RELEASES.to_string())
    };

    let report = refresh_with(
        &conn,
        date(),
        1_000_000,
        FetchScope::TopArtists,
        true,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();

    assert_eq!(report.artists_fetched, 1);
    assert_eq!(report.releases_upserted, 2);
    assert_eq!(urls.len(), 1);
    assert!(urls[0].contains("/release-group?"));
    let releases = query_releases(&conn, true, date()).unwrap();
    assert_eq!(releases.len(), 2);
    assert!(releases.iter().all(|release| {
        release.artist_name == "Pink Floyd" && release.artist_mbid == ARTIST_ID
    }));

    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, album, added_at) \
         VALUES ('/music/two.flac', 'Two', 'Pink Floyd', ?1, 'Future Album', 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let after_import = query_releases(&conn, true, date()).unwrap();
    assert_eq!(after_import.len(), 1);
    assert_eq!(after_import[0].title, "Recent EP");
}

#[test]
fn nr_1_name_resolution_persists_positive_and_negative_results() {
    let conn = migrated_conn();
    for (path, artist, plays) in [
        ("/music/pink.flac", "Pink Floyd", 20),
        ("/music/unknown.flac", "Unknown", 10),
    ] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, play_count, added_at) \
             VALUES (?1, 'Track', ?2, ?3, 0)",
            rusqlite::params![path, artist, plays],
        )
        .unwrap();
    }
    let mut fetch = |url: &str| {
        if url.contains("Pink%20Floyd") {
            Ok(SEARCH_EXACT.to_string())
        } else if url.contains("Unknown") {
            Ok(r#"{"artists":[]}"#.to_string())
        } else {
            Ok(RELEASES.to_string())
        }
    };

    let report = refresh_with(
        &conn,
        date(),
        1_000_000,
        FetchScope::TopArtists,
        true,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();

    assert_eq!((report.artists_fetched, report.unmatched), (1, 1));
    let positive: (Option<String>, i64) = conn
        .query_row(
            "SELECT artist_mbid, artist_mbid_negative FROM tracks WHERE artist = 'Pink Floyd'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(positive, (Some(ARTIST_ID.into()), 0));
    let negative: (Option<String>, i64) = conn
        .query_row(
            "SELECT artist_mbid, artist_mbid_negative FROM tracks WHERE artist = 'Unknown'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(negative, (None, 1));
}

#[test]
fn nr_1_fetch_queue_prioritizes_top_artists_and_rotates_the_rest_by_day() {
    let conn = migrated_conn();
    for index in 0..27 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, play_count, added_at) \
             VALUES (?1, 'Track', ?2, ?3, 0)",
            rusqlite::params![
                format!("/music/{index}.flac"),
                format!("Artist {index:02}"),
                100 - index
            ],
        )
        .unwrap();
    }

    let top = artists_for_fetch(&conn, FetchScope::TopArtists).unwrap();
    let day_zero = artists_for_fetch(&conn, FetchScope::AllArtists { day_index: 0 }).unwrap();
    let day_one = artists_for_fetch(&conn, FetchScope::AllArtists { day_index: 1 }).unwrap();

    assert_eq!(top.len(), 20);
    assert_eq!(top[0].name, "Artist 00");
    assert_eq!(top[19].name, "Artist 19");
    assert_eq!(day_zero.len(), 25);
    assert_eq!(day_zero[20].name, "Artist 20");
    assert_eq!(day_one.len(), 22);
    assert_eq!(day_one[20].name, "Artist 25");
}

#[test]
fn nr_2_fallback_accent_is_computed_when_release_is_inserted() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, play_count, added_at) \
         VALUES ('/music/accent.flac', 'Track', 'Pink Floyd', ?1, 10, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let mut fetch = |_url: &str| Ok(RELEASES.to_string());
    let mut accent = |_conn: &rusqlite::Connection, artist: &str| {
        assert_eq!(artist, "Pink Floyd");
        Some("#123456".to_string())
    };

    refresh_with(
        &conn,
        date(),
        1_000_000,
        FetchScope::TopArtists,
        true,
        &mut fetch,
        &mut accent,
    )
    .unwrap();

    let stored: String = conn
        .query_row(
            "SELECT fallback_accent FROM new_releases LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, "#123456");
}

#[test]
fn nr_2_accent_source_uses_the_most_played_album() {
    let conn = migrated_conn();
    for (path, album, plays) in [
        ("/music/a-one.flac", "Album A", 5),
        ("/music/a-two.flac", "Album A", 4),
        ("/music/b.flac", "Album B", 8),
    ] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'Track', 'Artist', ?2, ?3, 0)",
            rusqlite::params![path, album, plays],
        )
        .unwrap();
    }

    let path = most_played_album_track_path(&conn, "Artist").unwrap();

    assert_eq!(
        path.as_deref(),
        Some(std::path::Path::new("/music/a-one.flac"))
    );
}

#[test]
fn nr_3_opening_marks_seen_clears_badge() {
    let conn = migrated_conn();
    insert_release(&conn, "one", None);
    insert_release(&conn, "two", None);
    insert_release(&conn, "already-seen", Some(50));
    assert_eq!(unseen_release_count(&conn).unwrap(), 2);

    mark_releases_seen(&conn, &["one".into(), "two".into()], 100).unwrap();

    assert_eq!(unseen_release_count(&conn).unwrap(), 0);
    let seen_at: Vec<Option<i64>> = ["one", "two", "already-seen"]
        .into_iter()
        .map(|id| {
            conn.query_row(
                "SELECT seen_at FROM new_releases WHERE release_group_mbid = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(seen_at, [Some(100), Some(100), Some(50)]);
}

#[test]
fn nr_3_seen_item_not_rebadged() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, added_at) \
         VALUES ('/music/track.flac', 'Track', 'Pink Floyd', ?1, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let first_payload = r#"{"release-groups":[
      {"id":"known","title":"Known","first-release-date":"2026-08-01","primary-type":"Album"}
    ]}"#;
    let mut first_fetch = |_url: &str| Ok(first_payload.to_string());
    refresh_with(
        &conn,
        date(),
        10,
        FetchScope::TopArtists,
        true,
        &mut first_fetch,
        &mut no_accent,
    )
    .unwrap();
    mark_releases_seen(&conn, &["known".into()], 20).unwrap();

    let second_payload = r#"{"release-groups":[
      {"id":"known","title":"Known","first-release-date":"2026-08-01","primary-type":"Album"},
      {"id":"new","title":"New","first-release-date":"2026-08-02","primary-type":"Album"}
    ]}"#;
    let mut second_fetch = |_url: &str| Ok(second_payload.to_string());
    refresh_with(
        &conn,
        date(),
        30,
        FetchScope::TopArtists,
        true,
        &mut second_fetch,
        &mut no_accent,
    )
    .unwrap();

    assert_eq!(unseen_release_count(&conn).unwrap(), 1);
    let known_seen: Option<i64> = conn
        .query_row(
            "SELECT seen_at FROM new_releases WHERE release_group_mbid = 'known'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(known_seen, Some(20));
}

fn insert_release(conn: &rusqlite::Connection, mbid: &str, seen_at: Option<i64>) {
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, seen_at, fallback_accent
         ) VALUES (?1, 'Artist', 'artist-id', 'Release', 'Album', '2026-08-01', 1, ?2, '#123456')",
        rusqlite::params![mbid, seen_at],
    )
    .unwrap();
}
