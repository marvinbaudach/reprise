use std::cell::Cell;
use std::collections::HashMap;
use std::fs;

use tempfile::TempDir;
use url::Url;

use crate::lyrics::{
    active_line_index, cache_file, load_or_fetch_at, parse_lrc, parse_response, request_url,
    HttpOutcome, LyricsBody, LyricsError, LyricsQuery, TimedLine, NEGATIVE_TTL_SECONDS,
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
    assert_eq!(url.scheme(), "https");
    assert_eq!(url.host_str(), Some("lrclib.net"));
    assert_eq!(url.path(), "/api/get");
    let pairs: HashMap<_, _> = url.query_pairs().into_owned().collect();
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
fn blank_required_metadata_is_rejected_before_fetch() {
    for (title, artist) in [("", "Artist"), ("Title", "  ")] {
        let temp = TempDir::new().unwrap();
        let calls = Cell::new(0);
        let mut missing = query();
        missing.title = title.into();
        missing.artist = artist.into();
        let result = load_or_fetch_at(temp.path(), 1_000, &missing, false, |_| {
            calls.set(calls.get() + 1);
            HttpOutcome::Temporary
        });
        assert_eq!(result, Err(LyricsError::MissingMetadata));
        assert_eq!(calls.get(), 0);
    }
}

#[test]
fn response_prefers_synced_then_plain_and_preserves_instrumental() {
    let synced = parse_response(
        r#"{"instrumental":false,"plainLyrics":"plain fallback","syncedLyrics":"[00:01.00]Timed"}"#,
    )
    .unwrap();
    assert_eq!(
        synced,
        LyricsBody::Synced(vec![TimedLine {
            start_ms: 1_000,
            text: "Timed".into(),
        }])
    );

    let plain = parse_response(
        r#"{"instrumental":false,"plainLyrics":"plain fallback","syncedLyrics":"metadata only"}"#,
    )
    .unwrap();
    assert_eq!(plain, LyricsBody::Plain("plain fallback".into()));

    assert_eq!(
        parse_response(r#"{"instrumental":true,"plainLyrics":null,"syncedLyrics":null}"#),
        Ok(LyricsBody::Instrumental)
    );
    assert_eq!(
        parse_response(r#"{"instrumental":false,"plainLyrics":" ","syncedLyrics":null}"#),
        Err(LyricsError::InvalidResponse)
    );
}

#[test]
fn response_rejects_an_unreasonably_large_lyrics_body() {
    let text = "x".repeat(2 * 1024 * 1024 + 1);
    let body = serde_json::json!({
        "instrumental": false,
        "plainLyrics": text,
        "syncedLyrics": null,
    })
    .to_string();
    assert_eq!(parse_response(&body), Err(LyricsError::InvalidResponse));
}

#[test]
fn lrc_parser_supports_precision_multiple_marks_and_stable_sorting() {
    let parsed = parse_lrc(
        "[ar:Example]\n\
         [00:02]whole\n\
         [00:01.1]tenths\n\
         [00:01.12]hundredths\n\
         [00:01.123]milliseconds\n\
         [00:03.00][00:04.500]repeated\n\
         [00:05.00]first equal\n\
         [00:05.00]second equal\n\
         [broken]ignored\n\
         no timestamp",
    );
    assert_eq!(
        parsed,
        vec![
            TimedLine::new(1_100, "tenths"),
            TimedLine::new(1_120, "hundredths"),
            TimedLine::new(1_123, "milliseconds"),
            TimedLine::new(2_000, "whole"),
            TimedLine::new(3_000, "repeated"),
            TimedLine::new(4_500, "repeated"),
            TimedLine::new(5_000, "first equal"),
            TimedLine::new(5_000, "second equal"),
        ]
    );
}

#[test]
fn active_line_uses_last_timestamp_at_or_before_position() {
    let lines = vec![
        TimedLine::new(1_000, "one"),
        TimedLine::new(2_000, "two"),
        TimedLine::new(2_000, "two again"),
        TimedLine::new(4_000, "four"),
    ];
    assert_eq!(active_line_index(&lines, 999), None);
    assert_eq!(active_line_index(&lines, 1_000), Some(0));
    assert_eq!(active_line_index(&lines, 2_000), Some(2));
    assert_eq!(active_line_index(&lines, 3_999), Some(2));
    assert_eq!(active_line_index(&lines, 9_000), Some(3));
}

#[test]
fn positive_cache_roundtrip_skips_network_and_validates_identity() {
    let temp = TempDir::new().unwrap();
    let calls = Cell::new(0);
    let first = load_or_fetch_at(temp.path(), 1_000, &query(), false, |_| {
        calls.set(calls.get() + 1);
        HttpOutcome::Found(
            r#"{"instrumental":false,"plainLyrics":"cached synthetic","syncedLyrics":null}"#.into(),
        )
    })
    .unwrap();
    assert_eq!(first, LyricsBody::Plain("cached synthetic".into()));

    let second = load_or_fetch_at(temp.path(), 2_000, &query(), false, |_| {
        calls.set(calls.get() + 1);
        HttpOutcome::Temporary
    })
    .unwrap();
    assert_eq!(second, first);
    assert_eq!(calls.get(), 1);

    let cache = cache_file(temp.path(), &query());
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
    value["query"]["title"] = "Different identity".into();
    fs::write(&cache, serde_json::to_vec(&value).unwrap()).unwrap();
    let result = load_or_fetch_at(temp.path(), 3_000, &query(), false, |_| {
        calls.set(calls.get() + 1);
        HttpOutcome::NotFound
    });
    assert_eq!(result, Err(LyricsError::NotFound));
    assert_eq!(calls.get(), 2);
}

#[test]
fn negative_cache_expires_after_seven_days() {
    let temp = TempDir::new().unwrap();
    let calls = Cell::new(0);
    assert_eq!(
        load_or_fetch_at(temp.path(), 100, &query(), false, |_| {
            calls.set(calls.get() + 1);
            HttpOutcome::NotFound
        }),
        Err(LyricsError::NotFound)
    );
    assert_eq!(
        load_or_fetch_at(
            temp.path(),
            100 + NEGATIVE_TTL_SECONDS,
            &query(),
            false,
            |_| {
                calls.set(calls.get() + 1);
                HttpOutcome::Temporary
            }
        ),
        Err(LyricsError::NotFound)
    );
    assert_eq!(calls.get(), 1);

    let retried = load_or_fetch_at(
        temp.path(),
        101 + NEGATIVE_TTL_SECONDS,
        &query(),
        false,
        |_| {
            calls.set(calls.get() + 1);
            HttpOutcome::Found(
                r#"{"instrumental":false,"plainLyrics":"available later","syncedLyrics":null}"#
                    .into(),
            )
        },
    );
    assert_eq!(retried, Ok(LyricsBody::Plain("available later".into())));
    assert_eq!(calls.get(), 2);
}

#[test]
fn corrupt_cache_retries_and_temporary_errors_are_not_negative_cached() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path()).unwrap();
    fs::write(cache_file(temp.path(), &query()), b"broken").unwrap();
    assert_eq!(
        load_or_fetch_at(temp.path(), 500, &query(), false, |_| {
            HttpOutcome::Temporary
        }),
        Err(LyricsError::Temporary)
    );
    assert!(!cache_file(temp.path(), &query()).exists());

    let recovered = load_or_fetch_at(temp.path(), 501, &query(), false, |_| {
        HttpOutcome::Found(
            r#"{"instrumental":false,"plainLyrics":"recovered","syncedLyrics":null}"#.into(),
        )
    });
    assert_eq!(recovered, Ok(LyricsBody::Plain("recovered".into())));
}

#[test]
fn forced_refresh_keeps_positive_cache_on_temporary_failure() {
    let temp = TempDir::new().unwrap();
    let cached = load_or_fetch_at(temp.path(), 10, &query(), false, |_| {
        HttpOutcome::Found(
            r#"{"instrumental":false,"plainLyrics":"safe cached text","syncedLyrics":null}"#.into(),
        )
    })
    .unwrap();
    assert_eq!(
        load_or_fetch_at(temp.path(), 20, &query(), true, |_| HttpOutcome::Temporary),
        Ok(cached)
    );
}

#[test]
fn cache_stays_below_supplied_directory_and_publish_leaves_no_temp_file() {
    let temp = TempDir::new().unwrap();
    let destination = cache_file(temp.path(), &query());
    assert!(destination.starts_with(temp.path()));
    load_or_fetch_at(temp.path(), 10, &query(), false, |_| {
        HttpOutcome::Found(
            r#"{"instrumental":false,"plainLyrics":"stored","syncedLyrics":null}"#.into(),
        )
    })
    .unwrap();
    let files: Vec<_> = fs::read_dir(temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(files, vec![destination.file_name().unwrap()]);
}
