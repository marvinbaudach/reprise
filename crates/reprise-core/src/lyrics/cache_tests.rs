use std::fs;

use tempfile::TempDir;

use super::*;
use crate::lyrics::{LyricsBody, LyricsHit, LyricsQuery, LyricsSource, TimedLine};

fn query() -> LyricsQuery {
    LyricsQuery {
        title: "Synthetic Song".into(),
        artist: "Example Artist".into(),
        album: "Test Album".into(),
        duration_ms: 180_000,
    }
}

fn synced_hit() -> LyricsHit {
    LyricsHit {
        body: LyricsBody::Synced(vec![TimedLine::new(1_000, "fixture")]),
        source: LyricsSource::Lrclib,
    }
}

#[test]
fn version_one_cache_record_is_ignored() {
    let temp = TempDir::new().unwrap();
    let path = cache_file(temp.path(), &query());
    fs::create_dir_all(temp.path()).unwrap();
    fs::write(
        &path,
        r#"{
          "version": 1,
          "query": {
            "title": "Synthetic Song",
            "artist": "Example Artist",
            "album": "Test Album",
            "duration_ms": 180000
          },
          "fetched_at": 100,
          "result": {"Found": {"Plain": "old"}}
        }"#,
    )
    .unwrap();

    assert!(read_cache(temp.path(), &query()).is_none());
}

#[test]
fn version_two_negative_cache_is_invalidated_for_the_lrclib_search_chain() {
    let temp = TempDir::new().unwrap();
    let path = cache_file(temp.path(), &query());
    fs::create_dir_all(temp.path()).unwrap();
    fs::write(
        &path,
        r#"{
          "version": 2,
          "query": {
            "title": "Synthetic Song",
            "artist": "Example Artist",
            "album": "Test Album",
            "duration_ms": 180000
          },
          "fetched_at": 100,
          "result": "NotFound"
        }"#,
    )
    .unwrap();

    assert!(read_cache(temp.path(), &query()).is_none());
    assert!(!path.exists());
}

#[test]
fn found_source_survives_a_cache_round_trip() {
    let temp = TempDir::new().unwrap();

    write_found(temp.path(), 100, &query(), &synced_hit(), false);

    assert_eq!(cached_hit(temp.path(), &query()), Some(synced_hit()));
}

#[test]
fn negative_cache_ttl_remains_seven_days() {
    let temp = TempDir::new().unwrap();
    write_not_found(temp.path(), 100, &query());

    assert_eq!(
        needs_fetch_at(temp.path(), 100 + NEGATIVE_TTL_SECONDS, &query()),
        NeedsFetch::Skip
    );
    assert_eq!(
        needs_fetch_at(temp.path(), 101 + NEGATIVE_TTL_SECONDS, &query()),
        NeedsFetch::Fetch
    );
}

#[test]
fn needs_fetch_skips_positive_synced_and_fresh_negative_entries() {
    let positive = TempDir::new().unwrap();
    write_found(positive.path(), 100, &query(), &synced_hit(), false);
    assert_eq!(
        needs_fetch_at(positive.path(), 200, &query()),
        NeedsFetch::Skip
    );

    let negative = TempDir::new().unwrap();
    write_not_found(negative.path(), 100, &query());
    assert_eq!(
        needs_fetch_at(negative.path(), 200, &query()),
        NeedsFetch::Skip
    );
}

#[test]
fn needs_fetch_retries_plain_for_synced_and_fetches_a_missing_entry() {
    let temp = TempDir::new().unwrap();
    let plain = LyricsHit {
        body: LyricsBody::Plain("fixture".into()),
        source: LyricsSource::Lrclib,
    };
    write_found(temp.path(), 100, &query(), &plain, false);

    assert_eq!(
        needs_fetch_at(temp.path(), 200, &query()),
        NeedsFetch::RetryForSynced
    );

    let missing = TempDir::new().unwrap();
    assert_eq!(
        needs_fetch_at(missing.path(), 200, &query()),
        NeedsFetch::Fetch
    );
}

#[test]
fn completed_plain_retry_is_throttled_for_one_negative_ttl_window() {
    let temp = TempDir::new().unwrap();
    let plain = LyricsHit {
        body: LyricsBody::Plain("fixture".into()),
        source: LyricsSource::Lrclib,
    };
    write_found(temp.path(), 100, &query(), &plain, true);

    assert_eq!(
        needs_fetch_at(temp.path(), 100 + NEGATIVE_TTL_SECONDS, &query()),
        NeedsFetch::Skip
    );
    assert_eq!(
        needs_fetch_at(temp.path(), 101 + NEGATIVE_TTL_SECONDS, &query()),
        NeedsFetch::RetryForSynced
    );
}
