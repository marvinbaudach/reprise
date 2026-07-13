use std::cell::Cell;

use chrono::NaiveDate;
use tempfile::TempDir;

use crate::artist_news::{
    artist_search_url, cache_file, load_or_refresh_with, parse_artist_mbid, parse_release_groups,
    release_groups_url, ArtistMatch, NewsError, NewsKind,
};
use crate::musicbrainz::FetchError;

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
        format!("https://musicbrainz.org/ws/2/release-group?artist={ARTIST_ID}&type=album%7Cep&release-group-status=website-default&limit=100&fmt=json")
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
fn date_window_includes_exact_boundaries_and_ignores_missing_dates() {
    let json = r#"{"release-groups":[
      {"id":"today","title":"Today","first-release-date":"2026-07-13","primary-type":"Album"},
      {"id":"future","title":"Future Edge","first-release-date":"2027-07-13","primary-type":"Album"},
      {"id":"old","title":"Past Edge","first-release-date":"2025-07-13","primary-type":"Album"},
      {"id":"too-far","title":"Too Far","first-release-date":"2027-07-14","primary-type":"Album"},
      {"id":"too-old","title":"Too Old","first-release-date":"2025-07-12","primary-type":"Album"},
      {"id":"unknown","title":"Unknown","primary-type":"Album"}
    ]}"#;
    let items = parse_release_groups(json, &[], date());
    assert_eq!(items.len(), 3);
    assert!(items
        .iter()
        .any(|item| item.title == "Today" && item.kind == NewsKind::Upcoming));
    assert!(items.iter().any(|item| item.title == "Future Edge"));
    assert!(items.iter().any(|item| item.title == "Past Edge"));
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

fn fixture_fetch<'a>(
    calls: &'a Cell<usize>,
) -> impl FnMut(&str) -> Result<String, FetchError> + 'a {
    move |url| {
        calls.set(calls.get() + 1);
        if url.contains("/artist/") {
            Ok(SEARCH_EXACT.into())
        } else {
            Ok(RELEASES.into())
        }
    }
}

#[test]
fn fresh_cache_avoids_network_and_forced_refresh_reuses_artist_mbid() {
    let temp = TempDir::new().unwrap();
    let calls = Cell::new(0);
    let mut fetch = fixture_fetch(&calls);
    let first = load_or_refresh_with(
        "Pink Floyd",
        &[],
        date(),
        false,
        1_000_000,
        temp.path(),
        &mut fetch,
    )
    .unwrap();
    assert_eq!(calls.get(), 2);
    let cached = load_or_refresh_with(
        "Pink Floyd",
        &[],
        date(),
        false,
        1_000_100,
        temp.path(),
        &mut fetch,
    )
    .unwrap();
    assert_eq!(cached, first);
    assert_eq!(calls.get(), 2);
    let _ = load_or_refresh_with(
        "Pink Floyd",
        &[],
        date(),
        true,
        1_000_200,
        temp.path(),
        &mut fetch,
    )
    .unwrap();
    assert_eq!(calls.get(), 3);
}

#[test]
fn network_failure_returns_stale_positive_cache() {
    let temp = TempDir::new().unwrap();
    let calls = Cell::new(0);
    let mut fetch = fixture_fetch(&calls);
    load_or_refresh_with(
        "Pink Floyd",
        &[],
        date(),
        false,
        10,
        temp.path(),
        &mut fetch,
    )
    .unwrap();
    let mut offline = |_: &str| Err(FetchError::Transport);
    let stale = load_or_refresh_with(
        "Pink Floyd",
        &[],
        date(),
        true,
        20,
        temp.path(),
        &mut offline,
    )
    .unwrap();
    assert!(stale.stale);
    assert_eq!(stale.artist_mbid, ARTIST_ID);
}

#[test]
fn corrupt_cache_is_ignored_and_replaced() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path()).unwrap();
    std::fs::write(cache_file(temp.path(), "Pink Floyd"), b"broken").unwrap();
    let calls = Cell::new(0);
    let mut fetch = fixture_fetch(&calls);
    let result = load_or_refresh_with(
        "Pink Floyd",
        &[],
        date(),
        false,
        10,
        temp.path(),
        &mut fetch,
    )
    .unwrap();
    assert_eq!(result.items.len(), 2);
    assert_eq!(calls.get(), 2);
}

#[test]
fn negative_match_is_cached_for_one_day() {
    let temp = TempDir::new().unwrap();
    let calls = Cell::new(0);
    let mut no_match = |_: &str| {
        calls.set(calls.get() + 1);
        Ok(r#"{"artists":[]}"#.to_string())
    };
    assert_eq!(
        load_or_refresh_with(
            "Unknown",
            &[],
            date(),
            false,
            100,
            temp.path(),
            &mut no_match
        ),
        Err(NewsError::Unmatched)
    );
    assert_eq!(
        load_or_refresh_with(
            "Unknown",
            &[],
            date(),
            false,
            200,
            temp.path(),
            &mut no_match
        ),
        Err(NewsError::Unmatched)
    );
    assert_eq!(calls.get(), 1);
}
