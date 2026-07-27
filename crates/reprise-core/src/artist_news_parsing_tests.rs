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
fn release_parser_keeps_regular_albums_eps_and_owned_titles() {
    let items = parse_release_groups(RELEASES, date(), false);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Future Album");
    assert_eq!(items[0].kind, NewsKind::Upcoming);
    assert_eq!(items[1].title, "Recent EP");
}

#[test]
fn release_parser_excludes_secondary_and_non_album_types() {
    let json = r#"{"release-groups":[
      {"id":"1","title":"Live Album","first-release-date":"2026-07-01","primary-type":"Album","secondary-types":["Live"]},
      {"id":"2","title":"Remix EP","first-release-date":"2026-07-01","primary-type":"EP","secondary-types":["Remix"]},
      {"id":"3","title":"Single","first-release-date":"2026-07-01","primary-type":"Single","secondary-types":[]},
      {"id":"4","title":"Regular","first-release-date":"2026-07-01","primary-type":"Album","secondary-types":[]}
    ]}"#;
    let items = parse_release_groups(json, date(), false);
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
    let items = parse_release_groups(json, date(), false);
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

    let titles = parse_release_groups(json, date(), false)
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

    let titles = parse_release_groups(json, date(), false)
        .into_iter()
        .map(|item| item.title)
        .collect::<Vec<_>>();

    assert_eq!(titles, ["Future Single"]);
}

#[test]
fn nr_1a_singles_window_starts_ninety_days_ago() {
    // Same boundary as `nr_1a_album_and_ep_window_starts_ninety_days_ago`,
    // pinned for the singles path: with `include_singles` on, a released
    // single follows the same `NEWS_WINDOW_DAYS` window as albums and EPs.
    let json = r#"{"release-groups":[
      {"id":"single-edge","title":"Single Edge","first-release-date":"2026-04-14","primary-type":"Single"},
      {"id":"too-old","title":"Too Old Single","first-release-date":"2026-04-13","primary-type":"Single"}
    ]}"#;

    let titles = parse_release_groups(json, date(), true)
        .into_iter()
        .map(|item| item.title)
        .collect::<Vec<_>>();

    assert_eq!(titles, ["Single Edge"]);
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

    let items = parse_release_groups(&json, date(), false);

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
    let titles = parse_release_groups(json, date(), false)
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
    let items = parse_release_groups(json, date(), false);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Eclipse");
    assert_eq!(items[0].kind, NewsKind::Upcoming);
}

#[test]
fn released_owned_album_is_retained_for_query_time_presence() {
    let json = r#"{"release-groups":[
      {"id":"1","title":"Owned Album","first-release-date":"2026-07-01","primary-type":"Album","secondary-types":[]},
      {"id":"2","title":"Single Only","first-release-date":"2026-07-01","primary-type":"Album","secondary-types":[]}
    ]}"#;
    let items = parse_release_groups(json, date(), false);
    let titles = items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(titles, ["Owned Album", "Single Only"]);
}

#[test]
fn upcoming_album_is_retained_without_fetch_time_library_input() {
    let json = r#"{"release-groups":[
      {"id":"1","title":"Eclipse","first-release-date":"2026-09-01","primary-type":"Album","secondary-types":[]}
    ]}"#;
    let items = parse_release_groups(json, date(), false);

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
    let items = parse_release_groups(SINGLES, date(), false);
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
    let items = parse_release_groups(SINGLES, date(), true);
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
