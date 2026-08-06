use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use tempfile::TempDir;
use url::Url;

use super::*;
use crate::lyrics::breaker::Breaker;
use crate::lyrics::{
    LyricsBody, LyricsProvider, LyricsQuery, LyricsSource, SourceOutcome, TimedLine,
};

fn query() -> LyricsQuery {
    LyricsQuery {
        title: "Synthetic & Song".into(),
        artist: "Example Artist".into(),
        album: "Test / Album".into(),
        duration_ms: 180_499,
    }
}

#[test]
fn request_url_contains_exact_encoded_metadata_and_rounded_duration() {
    let url = Url::parse(&request_url(&query()).unwrap()).unwrap();
    let pairs: HashMap<_, _> = url.query_pairs().into_owned().collect();

    assert_eq!(url.host_str(), Some(HOST));
    assert_eq!(url.path(), "/api/get");
    assert_eq!(
        pairs,
        HashMap::from([
            ("track_name".into(), "Synthetic & Song".into()),
            ("artist_name".into(), "Example Artist".into()),
            ("album_name".into(), "Test / Album".into()),
            ("duration".into(), "180".into()),
        ])
    );
}

#[test]
fn search_request_contains_only_canonical_title_and_artist() {
    let url = Url::parse(&search_url(&query()).unwrap()).unwrap();
    let pairs: HashMap<_, _> = url.query_pairs().into_owned().collect();

    assert_eq!(url.host_str(), Some(HOST));
    assert_eq!(url.path(), "/api/search");
    assert_eq!(
        pairs,
        HashMap::from([
            ("track_name".into(), "Synthetic & Song".into()),
            ("artist_name".into(), "Example Artist".into()),
        ])
    );
}

#[test]
fn response_prefers_synced_then_plain_and_preserves_instrumental() {
    assert_eq!(
        parse_response(
            r#"{
              "instrumental": false,
              "plainLyrics": "plain fallback",
              "syncedLyrics": "[00:01.00]synced"
            }"#
        ),
        Ok(LyricsBody::Synced(vec![TimedLine::new(1_000, "synced")]))
    );
    assert_eq!(
        parse_response(r#"{"plainLyrics":"plain fallback"}"#),
        Ok(LyricsBody::Plain("plain fallback".into()))
    );
    assert_eq!(
        parse_response(r#"{"instrumental":true}"#),
        Ok(LyricsBody::Instrumental)
    );
}

#[test]
fn invalid_or_empty_response_is_failed_without_panicking() {
    assert!(parse_response("{broken").is_err());
    assert!(parse_response(r#"{"plainLyrics":"  "}"#).is_err());
}

#[test]
fn fixture_uses_source_prefix_and_legacy_name_as_fallback() {
    let temp = TempDir::new().unwrap();
    let request = fixture_request(&request_url(&query()).unwrap()).unwrap();
    let body = r#"{"plainLyrics":"fixture"}"#;
    std::fs::write(temp.path().join(request.filename()), body).unwrap();

    assert_eq!(
        fixture_get_at(&request_url(&query()).unwrap(), temp.path(), None),
        FetchOutcome::Found(body.into())
    );
    assert!(request.filename().starts_with("lrclib-"));
    assert!(request.legacy_filename().starts_with("lyrics-"));
}

#[test]
fn oversized_fixture_response_is_rejected_before_json_parsing() {
    let temp = TempDir::new().unwrap();
    let request = fixture_request(&request_url(&query()).unwrap()).unwrap();
    let oversized = vec![b'x'; crate::http_body::MAX_JSON_RESPONSE_BYTES as usize + 1];
    std::fs::write(temp.path().join(request.filename()), oversized).unwrap();

    assert_eq!(
        fixture_get_at(&request_url(&query()).unwrap(), temp.path(), None),
        FetchOutcome::Failed(false)
    );
}

#[test]
fn provider_maps_clean_not_found_and_hit_with_source() {
    let breaker = Breaker::new(3, 300);
    let not_found = |_url: &str| FetchOutcome::NotFound;
    let provider = LrclibProvider::new(&not_found, &breaker, 100, false);
    assert_eq!(provider.lookup(&query(), None), SourceOutcome::NotFound);

    let found = |_url: &str| FetchOutcome::Found(r#"{"plainLyrics":"fixture"}"#.into());
    let provider = LrclibProvider::new(&found, &breaker, 101, false);
    let SourceOutcome::Hit(hit) = provider.lookup(&query(), None) else {
        panic!("expected LRCLIB hit");
    };
    assert_eq!(hit.source, LyricsSource::Lrclib);
    assert_eq!(hit.body, LyricsBody::Plain("fixture".into()));
}

#[test]
fn lyr_5_clean_exact_miss_uses_search_and_returns_synced_match() {
    let breaker = Breaker::new(3, 300);
    let requested_paths = RefCell::new(Vec::new());
    let fetch = |url: &str| {
        let path = Url::parse(url).unwrap().path().to_string();
        requested_paths.borrow_mut().push(path.clone());
        match path.as_str() {
            "/api/get" => FetchOutcome::NotFound,
            "/api/search" => FetchOutcome::Found(
                r#"[{
                  "trackName": "Synthetic & Song",
                  "artistName": "Example Artist",
                  "albumName": "Another Edition",
                  "duration": 180,
                  "instrumental": false,
                  "plainLyrics": "plain fallback",
                  "syncedLyrics": "[00:01.00]synced rescue"
                }]"#
                .into(),
            ),
            other => panic!("unexpected LRCLIB path: {other}"),
        }
    };

    let outcome = LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None);

    let SourceOutcome::Hit(hit) = outcome else {
        panic!("expected synchronized LRCLIB search rescue, got {outcome:?}");
    };
    assert_eq!(
        hit.body,
        LyricsBody::Synced(vec![TimedLine::new(1_000, "synced rescue")])
    );
    assert_eq!(
        *requested_paths.borrow(),
        ["/api/get".to_string(), "/api/search".to_string()]
    );
}

#[test]
fn lyr_5_search_prefers_synced_lyrics_over_an_album_exact_plain_match() {
    let breaker = Breaker::new(3, 300);
    let fetch = |url: &str| match Url::parse(url).unwrap().path() {
        "/api/get" => FetchOutcome::NotFound,
        "/api/search" => FetchOutcome::Found(
            r#"[
              {
                "trackName": "Synthetic & Song",
                "artistName": "Example Artist",
                "albumName": "Test / Album",
                "duration": 180,
                "plainLyrics": "album-exact plain text"
              },
              {
                "trackName": "Synthetic & Song",
                "artistName": "Example Artist",
                "albumName": "Another Edition",
                "duration": 180,
                "plainLyrics": "other plain text",
                "syncedLyrics": "[00:02.00]preferred synced text"
              }
            ]"#
            .into(),
        ),
        other => panic!("unexpected LRCLIB path: {other}"),
    };

    let outcome = LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None);

    let SourceOutcome::Hit(hit) = outcome else {
        panic!("expected synchronized LRCLIB search result, got {outcome:?}");
    };
    assert_eq!(
        hit.body,
        LyricsBody::Synced(vec![TimedLine::new(2_000, "preferred synced text")])
    );
}

#[test]
fn lyr_5_search_rejects_synced_candidates_with_wrong_identity_or_duration() {
    let breaker = Breaker::new(3, 300);
    let fetch = |url: &str| match Url::parse(url).unwrap().path() {
        "/api/get" => FetchOutcome::NotFound,
        "/api/search" => FetchOutcome::Found(
            r#"[
              {
                "trackName": "Different Song",
                "artistName": "Example Artist",
                "albumName": "Test / Album",
                "duration": 180,
                "syncedLyrics": "[00:01.00]wrong title"
              },
              {
                "trackName": "Synthetic & Song",
                "artistName": "Example Artist",
                "albumName": "Test / Album",
                "duration": 184,
                "syncedLyrics": "[00:01.00]wrong duration"
              },
              {
                "trackName": "Synthetic & Song",
                "artistName": "Example Artist",
                "albumName": "Another Edition",
                "duration": 181,
                "plainLyrics": "valid plain fallback"
              }
            ]"#
            .into(),
        ),
        other => panic!("unexpected LRCLIB path: {other}"),
    };

    let outcome = LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None);

    let SourceOutcome::Hit(hit) = outcome else {
        panic!("expected the one valid LRCLIB candidate, got {outcome:?}");
    };
    assert_eq!(hit.body, LyricsBody::Plain("valid plain fallback".into()));
}

#[test]
fn lyr_5_search_duration_tolerance_uses_the_unrounded_track_duration() {
    let breaker = Breaker::new(3, 300);
    let fetch = |url: &str| match Url::parse(url).unwrap().path() {
        "/api/get" => FetchOutcome::NotFound,
        "/api/search" => FetchOutcome::Found(
            r#"[{
              "trackName": "Synthetic & Song",
              "artistName": "Example Artist",
              "albumName": "Test / Album",
              "duration": 178,
              "syncedLyrics": "[00:01.00]outside actual tolerance"
            }]"#
            .into(),
        ),
        other => panic!("unexpected LRCLIB path: {other}"),
    };

    assert_eq!(
        LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None),
        SourceOutcome::NotFound
    );
}

#[test]
fn lyr_5_search_uses_album_to_disambiguate_equal_synced_candidates() {
    let breaker = Breaker::new(3, 300);
    let fetch = |url: &str| match Url::parse(url).unwrap().path() {
        "/api/get" => FetchOutcome::NotFound,
        "/api/search" => FetchOutcome::Found(
            r#"[
              {
                "trackName": "Synthetic & Song",
                "artistName": "Example Artist",
                "albumName": "Test / Album",
                "duration": 180,
                "syncedLyrics": "[00:01.00]album match"
              },
              {
                "trackName": "Synthetic & Song",
                "artistName": "Example Artist",
                "albumName": "Different Album",
                "duration": 180,
                "syncedLyrics": "[00:01.00]wrong edition"
              }
            ]"#
            .into(),
        ),
        other => panic!("unexpected LRCLIB path: {other}"),
    };

    let outcome = LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None);

    let SourceOutcome::Hit(hit) = outcome else {
        panic!("expected album-disambiguated LRCLIB result, got {outcome:?}");
    };
    assert_eq!(
        hit.body,
        LyricsBody::Synced(vec![TimedLine::new(1_000, "album match")])
    );
}

#[test]
fn lyr_5_search_rejects_equally_ranked_synced_candidates() {
    let breaker = Breaker::new(3, 300);
    let fetch = |url: &str| match Url::parse(url).unwrap().path() {
        "/api/get" => FetchOutcome::NotFound,
        "/api/search" => FetchOutcome::Found(
            r#"[
              {
                "trackName": "Synthetic & Song",
                "artistName": "Example Artist",
                "albumName": "Edition One",
                "duration": 180,
                "syncedLyrics": "[00:01.00]first candidate"
              },
              {
                "trackName": "Synthetic & Song",
                "artistName": "Example Artist",
                "albumName": "Edition Two",
                "duration": 180,
                "syncedLyrics": "[00:01.00]second candidate"
              }
            ]"#
            .into(),
        ),
        other => panic!("unexpected LRCLIB path: {other}"),
    };

    assert_eq!(
        LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None),
        SourceOutcome::NotFound
    );
}

#[test]
fn lyr_5_exact_plain_result_is_upgraded_by_a_synced_search_match() {
    let breaker = Breaker::new(3, 300);
    let requested_paths = RefCell::new(Vec::new());
    let fetch = |url: &str| {
        let path = Url::parse(url).unwrap().path().to_string();
        requested_paths.borrow_mut().push(path.clone());
        match path.as_str() {
            "/api/get" => FetchOutcome::Found(r#"{"plainLyrics":"exact plain"}"#.into()),
            "/api/search" => FetchOutcome::Found(
                r#"[{
                  "trackName": "Synthetic & Song",
                  "artistName": "Example Artist",
                  "albumName": "Another Edition",
                  "duration": 180,
                  "syncedLyrics": "[00:03.00]synced upgrade"
                }]"#
                .into(),
            ),
            other => panic!("unexpected LRCLIB path: {other}"),
        }
    };

    let outcome = LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None);

    let SourceOutcome::Hit(hit) = outcome else {
        panic!("expected synchronized upgrade, got {outcome:?}");
    };
    assert_eq!(
        hit.body,
        LyricsBody::Synced(vec![TimedLine::new(3_000, "synced upgrade")])
    );
    assert_eq!(
        *requested_paths.borrow(),
        ["/api/get".to_string(), "/api/search".to_string()]
    );
}

#[test]
fn lyr_5_exact_synced_result_does_not_issue_a_search_request() {
    let breaker = Breaker::new(3, 300);
    let fetch = |url: &str| match Url::parse(url).unwrap().path() {
        "/api/get" => FetchOutcome::Found(r#"{"syncedLyrics":"[00:01.00]exact synced"}"#.into()),
        other => panic!("unexpected LRCLIB path: {other}"),
    };

    let SourceOutcome::Hit(hit) =
        LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None)
    else {
        panic!("expected exact synchronized lyrics");
    };
    assert_eq!(
        hit.body,
        LyricsBody::Synced(vec![TimedLine::new(1_000, "exact synced")])
    );
}

#[test]
fn lyr_5_exact_plain_result_survives_a_search_failure() {
    let breaker = Breaker::new(3, 300);
    let fetch = |url: &str| match Url::parse(url).unwrap().path() {
        "/api/get" => FetchOutcome::Found(r#"{"plainLyrics":"exact plain"}"#.into()),
        "/api/search" => FetchOutcome::Failed(true),
        other => panic!("unexpected LRCLIB path: {other}"),
    };

    let SourceOutcome::Hit(hit) =
        LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None)
    else {
        panic!("expected the exact plain fallback");
    };
    assert_eq!(hit.body, LyricsBody::Plain("exact plain".into()));
}

#[test]
fn lyr_5_search_retry_after_preserves_exact_plain_and_blocks_forced_retry() {
    let breaker = Breaker::new(3, 300);
    let fetch = |url: &str| match Url::parse(url).unwrap().path() {
        "/api/get" => FetchOutcome::Found(r#"{"plainLyrics":"exact plain"}"#.into()),
        "/api/search" => FetchOutcome::RateLimited(Some(130)),
        other => panic!("unexpected LRCLIB path: {other}"),
    };

    let SourceOutcome::Hit(hit) =
        LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None)
    else {
        panic!("expected the exact plain fallback");
    };
    assert_eq!(hit.body, LyricsBody::Plain("exact plain".into()));

    let must_not_fetch =
        |_url: &str| -> FetchOutcome { panic!("Retry-After must suppress the request") };
    assert_eq!(
        LrclibProvider::new(&must_not_fetch, &breaker, 129, true).lookup(&query(), None),
        SourceOutcome::Skipped
    );
}

#[test]
fn exact_transport_failure_does_not_issue_a_search_request() {
    let breaker = Breaker::new(3, 300);
    let requests = RefCell::new(0);
    let fetch = |_url: &str| {
        *requests.borrow_mut() += 1;
        FetchOutcome::Failed(true)
    };

    assert_eq!(
        LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None),
        SourceOutcome::Failed
    );
    assert_eq!(*requests.borrow(), 1);
}

#[test]
fn malformed_search_response_is_a_provider_failure_not_a_clean_miss() {
    let breaker = Breaker::new(3, 300);
    let fetch = |url: &str| match Url::parse(url).unwrap().path() {
        "/api/get" => FetchOutcome::NotFound,
        "/api/search" => FetchOutcome::Found("{broken".into()),
        other => panic!("unexpected LRCLIB path: {other}"),
    };

    assert_eq!(
        LrclibProvider::new(&fetch, &breaker, 100, false).lookup(&query(), None),
        SourceOutcome::Failed
    );
}

#[test]
fn lyr_5_server_retry_after_blocks_even_a_forced_request_until_the_deadline() {
    let breaker = Breaker::new(3, 300);
    let rate_limited = |_url: &str| FetchOutcome::RateLimited(Some(130));
    assert_eq!(
        LrclibProvider::new(&rate_limited, &breaker, 100, false).lookup(&query(), None),
        SourceOutcome::Failed
    );

    let must_not_fetch =
        |_url: &str| -> FetchOutcome { panic!("Retry-After must suppress the request") };
    assert_eq!(
        LrclibProvider::new(&must_not_fetch, &breaker, 129, true).lookup(&query(), None),
        SourceOutcome::Skipped
    );

    let recovered =
        |_url: &str| FetchOutcome::Found(r#"{"syncedLyrics":"[00:01.00]available again"}"#.into());
    assert!(matches!(
        LrclibProvider::new(&recovered, &breaker, 130, false).lookup(&query(), None),
        SourceOutcome::Hit(_)
    ));
}

#[test]
fn lrclib_http_status_maps_retry_after_without_sleeping() {
    let observed_at = std::time::UNIX_EPOCH
        + std::time::Duration::from_secs(100)
        + std::time::Duration::from_millis(500);
    assert_eq!(http_status_outcome(200, None, observed_at), None);
    assert_eq!(
        http_status_outcome(404, None, observed_at),
        Some(FetchOutcome::NotFound)
    );
    assert_eq!(
        http_status_outcome(429, Some("30"), observed_at),
        Some(FetchOutcome::RateLimited(Some(131)))
    );
    assert_eq!(
        http_status_outcome(429, Some("not-a-delay"), observed_at),
        Some(FetchOutcome::RateLimited(None))
    );
    assert_eq!(
        http_status_outcome(503, None, observed_at),
        Some(FetchOutcome::Failed(true))
    );
    assert_eq!(
        http_status_outcome(400, None, observed_at),
        Some(FetchOutcome::Failed(false))
    );
}

#[test]
fn provider_skips_an_open_breaker_unless_forced() {
    let breaker = Breaker::new(3, 300);
    for now in 1..=3 {
        breaker.record(HOST, crate::lyrics::breaker::BreakerOutcome::Failure, now);
    }
    let fetch = |_url: &str| FetchOutcome::Found(r#"{"plainLyrics":"fixture"}"#.into());

    assert_eq!(
        LrclibProvider::new(&fetch, &breaker, 4, false).lookup(&query(), None),
        SourceOutcome::Skipped
    );
    assert!(matches!(
        LrclibProvider::new(&fetch, &breaker, 4, true).lookup(&query(), None),
        SourceOutcome::Hit(_)
    ));
}

#[test]
fn track_path_is_not_part_of_the_remote_request_contract() {
    let breaker = Breaker::new(3, 300);
    let fetch = |_url: &str| FetchOutcome::NotFound;
    let provider = LrclibProvider::new(&fetch, &breaker, 100, false);

    assert_eq!(
        provider.lookup(&query(), Some(Path::new("/not/read/by/provider"))),
        SourceOutcome::NotFound
    );
}
