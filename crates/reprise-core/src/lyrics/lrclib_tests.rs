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
