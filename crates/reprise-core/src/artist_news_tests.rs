use chrono::NaiveDate;

use crate::artist_news::{
    artist_search_url, artists_for_fetch, configured_fetch_scope, hidden_release_count,
    mark_releases_seen, most_played_album_track_path, parse_artist_mbid, parse_release_groups,
    query_releases, refresh_with, release_groups_url, set_fetch_all_artists, set_release_hidden,
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
        format!("https://musicbrainz.org/ws/2/release-group?artist={ARTIST_ID}&type=album%7Cep%7Csingle&release-group-status=website-default&limit=100&inc=url-rels&fmt=json")
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
    let items = parse_release_groups(RELEASES, &[" recent   ep ".into()], date(), false);
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
    let items = parse_release_groups(json, &[], date(), false);
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
    let items = parse_release_groups(json, &[], date(), false);
    assert_eq!(items.len(), 3);
    assert!(items
        .iter()
        .any(|item| item.title == "Today" && item.kind == NewsKind::Upcoming));
    assert!(items.iter().any(|item| item.title == "Future Edge"));
    assert!(items.iter().any(|item| item.title == "Recent"));
}

#[test]
fn nr_1a_album_and_ep_window_starts_ninety_days_ago() {
    let json = r#"{"release-groups":[
      {"id":"album-edge","title":"Album Edge","first-release-date":"2026-04-14","primary-type":"Album"},
      {"id":"ep-edge","title":"EP Edge","first-release-date":"2026-04-14","primary-type":"EP"},
      {"id":"too-old","title":"Too Old","first-release-date":"2026-04-13","primary-type":"Album"},
      {"id":"future","title":"Distant Future","first-release-date":"2028-01-01","primary-type":"Album"}
    ]}"#;

    let titles = parse_release_groups(json, &[], date(), false)
        .into_iter()
        .map(|item| item.title)
        .collect::<Vec<_>>();

    assert_eq!(titles, ["Distant Future", "Album Edge", "EP Edge"]);
}

#[test]
fn nr_1a_singles_require_a_complete_future_date() {
    let json = r#"{"release-groups":[
      {"id":"future","title":"Future Single","first-release-date":"2026-07-14","primary-type":"Single"},
      {"id":"today","title":"Today Single","first-release-date":"2026-07-13","primary-type":"Single"},
      {"id":"past","title":"Past Single","first-release-date":"2026-07-12","primary-type":"Single"},
      {"id":"month","title":"Month Single","first-release-date":"2026-08","primary-type":"Single"},
      {"id":"year","title":"Year Single","first-release-date":"2027","primary-type":"Single"}
    ]}"#;

    let titles = parse_release_groups(json, &[], date(), false)
        .into_iter()
        .map(|item| item.title)
        .collect::<Vec<_>>();

    assert_eq!(titles, ["Future Single"]);
}

#[test]
fn nr_1a_secondary_types_are_excluded_before_the_twenty_item_cap() {
    let mut groups = vec![
        r#"{"id":"live","title":"Live","first-release-date":"2026-08-01","primary-type":"Album","secondary-types":["Live"]}"#
            .to_string(),
    ];
    for index in 0..21 {
        groups.push(format!(
            r#"{{"id":"item-{index:02}","title":"Item {index:02}","first-release-date":"2026-08-{day:02}","primary-type":"Album"}}"#,
            index = index,
            day = 2 + index
        ));
    }
    let json = format!(r#"{{"release-groups":[{}]}}"#, groups.join(","));

    let items = parse_release_groups(&json, &[], date(), false);

    assert_eq!(items.len(), 20);
    assert!(items.iter().all(|item| item.title != "Live"));
    assert!(items.iter().all(|item| item.title != "Item 20"));
    assert!(items.iter().any(|item| item.title == "Item 00"));
    assert!(items.iter().any(|item| item.title == "Item 19"));
}

#[test]
fn releases_sort_upcoming_ascending_then_new_descending() {
    let json = r#"{"release-groups":[
      {"id":"u3","title":"Upcoming 3","first-release-date":"2026-10-01","primary-type":"Album"},
      {"id":"n1","title":"New 1","first-release-date":"2026-06-01","primary-type":"Album"},
      {"id":"u1","title":"Upcoming 1","first-release-date":"2026-08-01","primary-type":"Album"},
      {"id":"n3","title":"New 3","first-release-date":"2026-04-01","primary-type":"Album"},
      {"id":"u2","title":"Upcoming 2","first-release-date":"2026-09-01","primary-type":"Album"},
      {"id":"n2","title":"New 2","first-release-date":"2026-05-01","primary-type":"Album"}
    ]}"#;
    let titles = parse_release_groups(json, &[], date(), false)
        .into_iter()
        .map(|item| item.title)
        .collect::<Vec<_>>();
    // "New 3" (2026-04-01) falls outside the 90-day news window measured
    // from `date()` (2026-07-13), not because of the item cap.
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
fn nr_1a_tag_mbid_skips_search_and_persists_releases() {
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
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, album, added_at) \
         VALUES ('/music/three.flac', 'Three', 'Pink Floyd', ?1, 'Future Album', 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let after_import = query_releases(&conn, true, date()).unwrap();
    assert_eq!(
        after_import.len(),
        2,
        "an in-library release is annotated, not dropped"
    );
    let future_album = after_import
        .iter()
        .find(|release| release.title == "Future Album")
        .unwrap();
    assert_eq!(
        future_album.presence,
        crate::artist_news::LibraryPresence::Complete,
        "the newly imported album (two local tracks) is marked fully owned"
    );
    let recent_ep = after_import
        .iter()
        .find(|release| release.title == "Recent EP")
        .unwrap();
    assert_eq!(
        recent_ep.presence,
        crate::artist_news::LibraryPresence::Absent,
        "an album with no local match stays absent"
    );
}

#[test]
fn nr_1a_name_resolution_persists_positive_and_negative_results() {
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
fn nr_1a_fetch_queue_prioritizes_top_artists_and_includes_the_never_checked_rest() {
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
    let all = artists_for_fetch(&conn, FetchScope::AllArtists).unwrap();

    assert_eq!(top.len(), 20);
    assert_eq!(top[0].name, "Artist 00");
    assert_eq!(top[19].name, "Artist 19");
    assert_eq!(
        all.len(),
        27,
        "the 7-artist rest group fits well within REST_ARTISTS_PER_RUN"
    );
    assert_eq!(all[20].name, "Artist 20");
    assert_eq!(all[26].name, "Artist 26");
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

#[test]
fn hide_sets_hidden_and_set_release_hidden_false_restores_it() {
    let conn = migrated_conn();
    insert_release(&conn, "one", None);
    insert_release(&conn, "two", None);

    set_release_hidden(&conn, "one", true).unwrap();

    assert_eq!(hidden_release_count(&conn).unwrap(), 1);
    assert_eq!(
        query_releases(&conn, false, date())
            .unwrap()
            .into_iter()
            .map(|release| release.release_group_mbid)
            .collect::<Vec<_>>(),
        ["two"]
    );
    assert!(query_releases(&conn, true, date())
        .unwrap()
        .into_iter()
        .find(|release| release.release_group_mbid == "one")
        .is_some_and(|release| release.hidden));
    let hidden_at: Option<i64> = conn
        .query_row(
            "SELECT hidden_at FROM new_releases WHERE release_group_mbid = 'one'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(hidden_at.is_some(), "hiding must stamp hidden_at");

    set_release_hidden(&conn, "one", false).unwrap();
    let hidden_at_after_unhide: Option<i64> = conn
        .query_row(
            "SELECT hidden_at FROM new_releases WHERE release_group_mbid = 'one'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        hidden_at_after_unhide.is_none(),
        "un-hiding via set_release_hidden clears hidden_at again"
    );

    set_release_hidden(&conn, "one", true).unwrap();

    // The former blanket "un-hide everything" helper (`show_hidden_releases`)
    // is gone — `restore_release` (A2) replaces it for the real UI path
    // (single release, wired in C1), and `set_release_hidden(.., false)`
    // remains the primitive both build on, which this asserts directly.
    set_release_hidden(&conn, "one", false).unwrap();

    assert_eq!(hidden_release_count(&conn).unwrap(), 0);
    assert_eq!(query_releases(&conn, false, date()).unwrap().len(), 2);
}

#[test]
fn nr_13_query_marks_local_albums_instead_of_dropping_them() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, fallback_accent
         ) VALUES ('owned', 'Pink Floyd', 'artist-id', 'Local Album', 'Album', '2026-08-01', 1, '#123456')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO new_releases (
           release_group_mbid, artist_name, artist_mbid, title, release_type,
           first_release_date, fetched_at, fallback_accent
         ) VALUES ('new', 'Pink Floyd', 'artist-id', 'Brand New Album', 'Album', '2026-08-01', 1, '#123456')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/local.flac', 'Track', 'Pink Floyd', 'Local Album', 5, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/local2.flac', 'Track Two', 'Pink Floyd', 'Local Album', 5, 0)",
        [],
    )
    .unwrap();

    let releases = query_releases(&conn, true, date()).unwrap();

    assert_eq!(releases.len(), 2, "in-library releases stay in the list");
    let owned = releases
        .iter()
        .find(|release| release.release_group_mbid == "owned")
        .unwrap();
    assert_eq!(
        owned.presence,
        crate::artist_news::LibraryPresence::Complete,
        "matching local album (two tracks) is marked fully owned"
    );
    let brand_new = releases
        .iter()
        .find(|release| release.release_group_mbid == "new")
        .unwrap();
    assert_eq!(
        brand_new.presence,
        crate::artist_news::LibraryPresence::Absent,
        "release with no local match stays absent"
    );
}

#[test]
fn first_seen_is_set_on_insert_and_preserved_across_upsert() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, play_count, added_at) \
         VALUES ('/music/one.flac', 'One', 'Pink Floyd', ?1, 10, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let mut fetch = |_url: &str| Ok(RELEASES.to_string());

    refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        true,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();

    let (first_seen, fetched_at): (i64, i64) = conn
        .query_row(
            "SELECT first_seen, fetched_at FROM new_releases WHERE release_group_mbid = '1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(first_seen, 1_000);
    assert_eq!(fetched_at, 1_000);

    refresh_with(
        &conn,
        date(),
        2_000,
        FetchScope::TopArtists,
        true,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();

    let (first_seen_after, fetched_at_after): (i64, i64) = conn
        .query_row(
            "SELECT first_seen, fetched_at FROM new_releases WHERE release_group_mbid = '1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        first_seen_after, 1_000,
        "first_seen must never move on re-fetch"
    );
    assert_eq!(
        fetched_at_after, 2_000,
        "fetched_at must still track the latest fetch"
    );
}

#[test]
fn nr_7_fetch_scope_defaults_to_top_and_round_trips_all_artists() {
    let conn = migrated_conn();
    assert_eq!(
        configured_fetch_scope(&conn).unwrap(),
        FetchScope::TopArtists
    );

    set_fetch_all_artists(&conn, true).unwrap();

    assert_eq!(
        configured_fetch_scope(&conn).unwrap(),
        FetchScope::AllArtists
    );
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

#[test]
fn ledger_marks_artist_without_news_fresh_and_second_run_skips_it() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, album, play_count, added_at) \
         VALUES ('/music/one.flac', 'One', 'Pink Floyd', ?1, 'Local Album', 20, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    // No release groups at all — the artist has nothing to report.
    let empty = r#"{"release-groups":[]}"#;
    // A `Cell` rather than a plain counter: the closure only needs a shared
    // reference to bump it, so `calls.get()` can be read between the two
    // `refresh_with` calls without conflicting with the still-live `fetch`.
    let calls = std::cell::Cell::new(0);
    let mut fetch = |_url: &str| {
        calls.set(calls.get() + 1);
        Ok(empty.to_string())
    };

    let first = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(first.artists_fetched, 1);
    let after_first = calls.get();
    assert!(after_first > 0, "first run must hit the network");

    let second = refresh_with(
        &conn,
        date(),
        2_000, // well inside FETCH_TTL_SECONDS
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(
        second.artists_fetched, 0,
        "artist with no news must count as fresh, not be re-fetched"
    );
    assert_eq!(
        calls.get(),
        after_first,
        "second run must issue no requests"
    );
}

#[test]
fn ledger_records_unmatched_outcome_and_negative_match_excludes_future_search() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/two.flac', 'Two', 'Nobody At All', 'Local Album', 5, 0)",
        [],
    )
    .unwrap();
    let calls = std::cell::Cell::new(0);
    let mut fetch = |_url: &str| {
        calls.set(calls.get() + 1);
        Ok(r#"{"artists":[]}"#.to_string())
    };

    let first = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(first.unmatched, 1);
    let after_first = calls.get();

    // Assert directly against the ledger — this is the actual requirement
    // under test. `normalize("Nobody At All")` is "nobody at all".
    let artist_key = "nobody at all";
    assert_eq!(
        crate::artist_news_ledger::last_attempt_at(&conn, artist_key).unwrap(),
        Some(1_000),
        "an unmatched artist must still get a ledger entry for its attempt"
    );
    let outcome: String = conn
        .query_row(
            "SELECT last_outcome FROM artist_news_fetch WHERE artist_key = ?1",
            [artist_key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(outcome, "unmatched");

    // This second call does NOT exercise the ledger's TTL-based freshness
    // check at all: `persist_artist_match` already set
    // `artist_mbid_negative = 1` on the first run, and
    // `artists_for_fetch`'s `HAVING` clause excludes that artist from the
    // candidate list outright. So this only proves a negatively-matched
    // artist stops costing search requests forever, once the ledger
    // assertions above have already pinned the actual requirement.
    refresh_with(
        &conn,
        date(),
        2_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(
        calls.get(),
        after_first,
        "a negatively-matched artist must not be searched again"
    );
}

#[test]
fn ledger_records_failed_fetch_and_ttl_prevents_a_retry_within_the_window() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, play_count, added_at) \
         VALUES ('/music/one.flac', 'One', 'Pink Floyd', ?1, 20, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let calls = std::cell::Cell::new(0);
    let mut fetch = |_url: &str| {
        calls.set(calls.get() + 1);
        Err(crate::musicbrainz::FetchError::Transport)
    };

    let first = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(first.failed, 1);
    let after_first = calls.get();
    assert_eq!(
        after_first, 1,
        "a known MBID must skip the search request and hit only release-groups"
    );

    let artist_key = "pink floyd";
    assert_eq!(
        crate::artist_news_ledger::last_attempt_at(&conn, artist_key).unwrap(),
        Some(1_000),
        "a failed fetch must still be recorded in the ledger"
    );
    let outcome: String = conn
        .query_row(
            "SELECT last_outcome FROM artist_news_fetch WHERE artist_key = ?1",
            [artist_key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(outcome, "failed");

    let second = refresh_with(
        &conn,
        date(),
        2_000, // well inside FETCH_TTL_SECONDS: no separate failure backoff exists
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(
        second.failed, 0,
        "a permanently-failing artist must count as fresh inside the TTL window"
    );
    assert_eq!(
        calls.get(),
        after_first,
        "no further requests may be issued for it within the TTL window"
    );
}

#[test]
fn latest_fetched_at_reads_the_ledger_not_found_releases() {
    let conn = migrated_conn();
    assert_eq!(
        crate::artist_news::latest_fetched_at(&conn).unwrap(),
        None,
        "empty ledger means never fetched"
    );
    crate::artist_news_ledger::record_attempt(
        &conn,
        "pink floyd",
        None,
        4_242,
        crate::artist_news_ledger::FetchOutcome::Ok,
        0,
    )
    .unwrap();
    assert_eq!(
        crate::artist_news::latest_fetched_at(&conn).unwrap(),
        Some(4_242),
        "an attempt without any found release must still count"
    );
}

#[test]
fn rotation_prefers_never_checked_artists_over_play_count() {
    let conn = migrated_conn();
    // 22 artists so the rest group is non-empty (TOP_ARTIST_COUNT = 20).
    // Play counts descend, so "artist-21" and "artist-22" are the tail.
    for index in 1..=22 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', ?2, 'Album', ?3, 0)",
            rusqlite::params![
                format!("/music/{index}.flac"),
                format!("artist-{index:02}"),
                100 - index,
            ],
        )
        .unwrap();
    }
    // The very last artist by plays was checked long ago; the second-to-last
    // was checked just now. Only the stale one may come up.
    crate::artist_news_ledger::record_attempt(
        &conn,
        "artist-21",
        None,
        9_000,
        crate::artist_news_ledger::FetchOutcome::Ok,
        0,
    )
    .unwrap();

    let candidates = artists_for_fetch(&conn, FetchScope::AllArtists).unwrap();
    let names = candidates
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(names.len(), 22, "top 20 plus the rest group");
    assert_eq!(
        names[20], "artist-22",
        "never-checked artist must come before a recently checked one"
    );
    assert_eq!(names[21], "artist-21");
}

#[test]
fn top_artists_scope_ignores_the_rest_group_entirely() {
    let conn = migrated_conn();
    for index in 1..=22 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', ?2, 'Album', ?3, 0)",
            rusqlite::params![
                format!("/music/{index}.flac"),
                format!("artist-{index:02}"),
                100 - index,
            ],
        )
        .unwrap();
    }
    let candidates = artists_for_fetch(&conn, FetchScope::TopArtists).unwrap();
    assert_eq!(candidates.len(), 20);
}

#[test]
fn configured_scope_round_trips_without_a_date() {
    let conn = migrated_conn();
    assert_eq!(
        configured_fetch_scope(&conn).unwrap(),
        FetchScope::TopArtists
    );
    set_fetch_all_artists(&conn, true).unwrap();
    assert_eq!(
        configured_fetch_scope(&conn).unwrap(),
        FetchScope::AllArtists
    );
}

#[test]
fn upcoming_album_survives_a_local_title_match() {
    // The lead single is tagged with the forthcoming album's name. An album
    // that has not been released yet cannot be owned, so the match must be
    // ignored entirely — this is the case the whole change exists for.
    let json = r#"{"release-groups":[
      {"id":"1","title":"Eclipse","first-release-date":"2026-09-01","primary-type":"Album","secondary-types":[]}
    ]}"#;
    let items = parse_release_groups(json, &["Eclipse".into()], date(), false);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Eclipse");
    assert_eq!(items[0].kind, NewsKind::Upcoming);
}

#[test]
fn released_album_is_filtered_only_when_the_local_album_is_really_owned() {
    let conn = migrated_conn();
    // Two tracks under "Owned Album" — that counts as owned.
    for index in 1..=2 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', 'Pink Floyd', 'Owned Album', 1, 0)",
            [format!("/music/owned-{index}.flac")],
        )
        .unwrap();
    }
    // One track under "Single Only" — a single, not the album.
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/single.flac', 'S', 'Pink Floyd', 'Single Only', 1, 0)",
        [],
    )
    .unwrap();

    let owned = crate::artist_news::local_albums_for_test(&conn, "Pink Floyd").unwrap();
    assert!(owned.iter().any(|album| album == "Owned Album"));
    assert!(
        !owned.iter().any(|album| album == "Single Only"),
        "one track must not make the whole album count as owned"
    );

    let json = r#"{"release-groups":[
      {"id":"1","title":"Owned Album","first-release-date":"2026-07-01","primary-type":"Album","secondary-types":[]},
      {"id":"2","title":"Single Only","first-release-date":"2026-07-01","primary-type":"Album","secondary-types":[]}
    ]}"#;
    let items = parse_release_groups(json, &owned, date(), false);
    let titles = items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(titles, ["Single Only"]);
}

#[test]
fn upcoming_album_bypasses_the_owned_threshold_even_when_two_local_tracks_match() {
    // The user owns the lead single *and* a B-side, both mis-tagged with the
    // forthcoming album's name — two local tracks, which is exactly
    // `OWNED_ALBUM_MIN_TRACKS`. By the threshold's own measure this album
    // looks owned. But the album has not been released yet, so an unreleased
    // album cannot be owned: the bypass must let it through regardless.
    let conn = migrated_conn();
    for index in 1..=2 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', 'Pink Floyd', 'Eclipse', 1, 0)",
            [format!("/music/eclipse-{index}.flac")],
        )
        .unwrap();
    }

    let owned = crate::artist_news::local_albums_for_test(&conn, "Pink Floyd").unwrap();
    assert!(
        owned.iter().any(|album| album == "Eclipse"),
        "two local tracks must meet the owned-album threshold"
    );

    let json = r#"{"release-groups":[
      {"id":"1","title":"Eclipse","first-release-date":"2026-09-01","primary-type":"Album","secondary-types":[]}
    ]}"#;
    let items = parse_release_groups(json, &owned, date(), false);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Eclipse");
    assert_eq!(items[0].kind, NewsKind::Upcoming);
}

#[test]
fn presence_distinguishes_absent_partial_and_complete() {
    use crate::artist_news::{presence_for, LibraryPresence};

    let mut counts = std::collections::HashMap::new();
    counts.insert(("pink floyd".to_string(), "owned album".to_string()), 2);
    counts.insert(("pink floyd".to_string(), "just a single".to_string()), 1);

    assert_eq!(
        presence_for(&counts, "Pink Floyd", "Owned Album"),
        LibraryPresence::Complete
    );
    assert_eq!(
        presence_for(&counts, " PINK   FLOYD ", " just a single "),
        LibraryPresence::Partial,
        "normalization must match query_releases' own"
    );
    assert_eq!(
        presence_for(&counts, "Pink Floyd", "Never Heard Of It"),
        LibraryPresence::Absent
    );
}

#[test]
fn query_releases_reports_partial_ownership_for_a_single_track() {
    use crate::artist_news::LibraryPresence;

    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/lead.flac', 'Lead Single', 'Pink Floyd', 'Eclipse', 1, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO new_releases (release_group_mbid, artist_name, artist_mbid, title, \
         release_type, first_release_date, fetched_at, fallback_accent, first_seen) \
         VALUES ('rg-1', 'Pink Floyd', 'mbid-1', 'Eclipse', 'Album', '2026-09-01', 100, \
         '#3584E4', 100)",
        [],
    )
    .unwrap();

    let releases = query_releases(&conn, false, date()).unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].presence, LibraryPresence::Partial);
}

#[test]
fn track_counts_survive_internal_whitespace_tagging_drift() {
    use crate::artist_news::{local_album_track_counts, presence_for, LibraryPresence};

    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/one.flac', 'T1', 'Pink Floyd', 'Eclipse', 1, 0)",
        [],
    )
    .unwrap();
    // Same artist tag, but with an extra internal space. SQL's
    // `lower(trim(x))` grouping treats this as a distinct artist, while
    // Rust's `normalize()` collapses both to "pink floyd". If counting
    // happens on the SQL side, this second track lands in its own group of
    // one and the two real tracks are never summed together.
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/two.flac', 'T2', 'Pink  Floyd', 'Eclipse', 1, 0)",
        [],
    )
    .unwrap();

    let counts = local_album_track_counts(&conn).unwrap();
    assert_eq!(
        presence_for(&counts, "Pink Floyd", "Eclipse"),
        LibraryPresence::Complete,
        "two tracks tagged with an internal-whitespace-only artist variant must still count as one owned album"
    );
}

const SINGLES: &str = r#"{"release-groups":[
  {"id":"1","title":"Released Single","first-release-date":"2026-07-01","primary-type":"Single","secondary-types":[]},
  {"id":"2","title":"Announced Single","first-release-date":"2026-08-20","primary-type":"Single","secondary-types":[]},
  {"id":"3","title":"Old Single","first-release-date":"2025-01-01","primary-type":"Single","secondary-types":[]}
]}"#;

#[test]
fn released_singles_are_dropped_while_the_switch_is_off() {
    let items = parse_release_groups(SINGLES, &[], date(), false);
    let titles = items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        titles,
        ["Announced Single"],
        "an announced single with an exact date passes regardless of the switch"
    );
}

#[test]
fn released_singles_pass_within_the_window_while_the_switch_is_on() {
    let items = parse_release_groups(SINGLES, &[], date(), true);
    let mut titles = items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<Vec<_>>();
    titles.sort_unstable();
    assert_eq!(
        titles,
        ["Announced Single", "Released Single"],
        "'Old Single' is outside NEWS_WINDOW_DAYS and stays out"
    );
}

#[test]
fn include_singles_setting_defaults_to_off_and_round_trips() {
    let conn = migrated_conn();
    assert!(!crate::artist_news::include_singles(&conn).unwrap());
    crate::artist_news::set_include_singles(&conn, true).unwrap();
    assert!(crate::artist_news::include_singles(&conn).unwrap());
    crate::artist_news::set_include_singles(&conn, false).unwrap();
    assert!(!crate::artist_news::include_singles(&conn).unwrap());
}
