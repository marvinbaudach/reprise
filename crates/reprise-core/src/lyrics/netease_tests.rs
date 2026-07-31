use std::path::Path;

use tempfile::TempDir;

use super::*;
use crate::lyrics::breaker::Breaker;
use crate::lyrics::{
    LyricsBody, LyricsProvider, LyricsQuery, LyricsSource, SourceOutcome, TimedLine,
};

fn query() -> LyricsQuery {
    LyricsQuery {
        title: "Synthetic Song".into(),
        artist: "Example Artist".into(),
        album: "Test Album".into(),
        duration_ms: 180_000,
    }
}

fn search_body(duration_ms: i64) -> String {
    format!(
        r#"{{
          "result": {{
            "songs": [{{
              "id": 42,
              "name": "  Synthetic   Song ",
              "artists": [{{"name": "EXAMPLE ARTIST"}}],
              "duration": {duration_ms}
            }}]
          }}
        }}"#
    )
}

fn provider_with_fixtures(directory: &Path, search: &str, lyric: Option<&str>) -> SourceOutcome {
    let fetcher = FixtureFetcher::new(directory);
    std::fs::write(directory.join(search_fixture_filename(&query())), search).unwrap();
    if let Some(lyric) = lyric {
        std::fs::write(directory.join(lyric_fixture_filename(42)), lyric).unwrap();
    }
    let breaker = Breaker::new(3, 300);
    NeteaseProvider::new(&fetcher, &breaker, 100, false).lookup(&query(), None)
}

#[test]
fn fixture_candidate_selection_accepts_exact_normalized_match() {
    let temp = TempDir::new().unwrap();
    let outcome = provider_with_fixtures(
        temp.path(),
        &search_body(182_999),
        Some(r#"{"lrc":{"lyric":"[00:01.00]fixture"}}"#),
    );

    let SourceOutcome::Hit(hit) = outcome else {
        panic!("expected NetEase fixture hit");
    };
    assert_eq!(hit.source, LyricsSource::Netease);
    assert_eq!(
        hit.body,
        LyricsBody::Synced(vec![TimedLine::new(1_000, "fixture")])
    );
}

#[test]
fn fixture_candidate_outside_duration_tolerance_is_not_found() {
    let temp = TempDir::new().unwrap();

    assert_eq!(
        provider_with_fixtures(temp.path(), &search_body(183_001), None),
        SourceOutcome::NotFound
    );
}

#[test]
fn fixture_empty_search_is_not_found() {
    let temp = TempDir::new().unwrap();

    assert_eq!(
        provider_with_fixtures(temp.path(), r#"{"result":{"songs":[]}}"#, None),
        SourceOutcome::NotFound
    );
}

#[test]
fn fixture_broken_search_json_is_failed() {
    let temp = TempDir::new().unwrap();

    assert_eq!(
        provider_with_fixtures(temp.path(), "{broken", None),
        SourceOutcome::Failed
    );
}

#[test]
fn lyric_lrc_with_timestamps_is_synced_and_without_is_plain() {
    assert_eq!(
        parse_lyric(r#"{"lrc":{"lyric":"[00:01.25]fixture"}}"#),
        Ok(Some(LyricsBody::Synced(vec![TimedLine::new(
            1_250, "fixture"
        )])))
    );
    assert_eq!(
        parse_lyric(r#"{"lrc":{"lyric":"plain fixture"}}"#),
        Ok(Some(LyricsBody::Plain("plain fixture".into())))
    );
}

#[test]
fn search_and_lyric_urls_are_https_and_bounded_to_netease() {
    let search = url::Url::parse(&search_url(&query()).unwrap()).unwrap();
    let lyric = url::Url::parse(&lyric_url(42).unwrap()).unwrap();

    assert_eq!(search.host_str(), Some(HOST));
    assert_eq!(search.path(), "/api/search/get");
    assert!(search
        .query_pairs()
        .any(|(key, value)| { key == "s" && value == "Example Artist Synthetic Song" }));
    assert_eq!(lyric.host_str(), Some(HOST));
    assert_eq!(lyric.path(), "/api/song/lyric");
    assert!(lyric
        .query_pairs()
        .any(|(key, value)| key == "id" && value == "42"));
}

#[test]
fn provider_skips_an_open_breaker_unless_forced() {
    let temp = TempDir::new().unwrap();
    let fetcher = FixtureFetcher::new(temp.path());
    let breaker = Breaker::new(3, 300);
    for now in 1..=3 {
        breaker.record(HOST, crate::lyrics::breaker::BreakerOutcome::Failure, now);
    }

    assert_eq!(
        NeteaseProvider::new(&fetcher, &breaker, 4, false).lookup(&query(), None),
        SourceOutcome::Skipped
    );
    assert_ne!(
        NeteaseProvider::new(&fetcher, &breaker, 4, true).lookup(&query(), None),
        SourceOutcome::Skipped
    );
}
