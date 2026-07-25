//! Tests for the MusicBrainz JSON parsing, URL builders, and
//! release-comparison/sorting helpers in `artist_news_parsing.rs`. Split out
//! of `artist_news_tests.rs` purely to keep both files under the project's
//! 800-line rule — a pure move, not a rewrite.

use chrono::NaiveDate;

use crate::artist_news::{
    artist_search_url, parse_artist_mbid, parse_release_groups, release_groups_url, ArtistMatch,
    NewsKind,
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

fn migrated_conn() -> rusqlite::Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
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
fn released_album_is_owned_despite_internal_whitespace_tagging_drift() {
    // Two tracks of the same album, tagged with an internal whitespace run
    // that differs ("The  Wall" vs "The Wall"). SQL's `lower(trim(x))` only
    // trims the ends, so grouping in SQL would split these into two groups
    // of one track each and neither would reach `OWNED_ALBUM_MIN_TRACKS`.
    // `normalize()` additionally collapses internal whitespace runs, so both
    // tracks must land in the same group and the album must count as owned.
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/wall-1.flac', 'T', 'Pink Floyd', 'The  Wall', 1, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/wall-2.flac', 'T', 'Pink Floyd', 'The Wall', 1, 0)",
        [],
    )
    .unwrap();

    let owned = crate::artist_news::local_albums_for_test(&conn, "Pink Floyd").unwrap();
    assert!(
        owned
            .iter()
            .any(|album| crate::artist_news::normalize(album) == crate::artist_news::normalize("The Wall")),
        "two tracks differing only by an internal whitespace run must count as one owned album, got {owned:?}"
    );
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
