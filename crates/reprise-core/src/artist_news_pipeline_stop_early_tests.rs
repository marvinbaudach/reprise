//! Tests for the consecutive-failure circuit breaker in
//! `refresh_with_progress_at` (`artist_news_pipeline.rs`). Split out of
//! `artist_news_pipeline_tests.rs` purely to keep both files under the
//! project's 800-line rule — a pure addition of a new file, not a rewrite.

use chrono::NaiveDate;

use crate::artist_news::{
    refresh_with, refresh_with_progress_at, FetchScope, RefreshHooks, RefreshProgress,
};
use crate::musicbrainz::FetchError;

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 7, 13).unwrap()
}

fn migrated_conn() -> crate::db::Db {
    crate::db::Db::open_in_memory().unwrap()
}

/// Inserts `count` artists, each with a distinct, already-known MBID (so the
/// artist-search request is skipped and every attempt costs exactly one
/// release-groups fetch) and a distinct, descending `play_count` (so
/// `artists_for_fetch` orders them `artist 0, artist 1, ...` deterministically).
fn seed_artists(db: &crate::db::Db, count: usize) {
    for index in 0..count {
        db.conn()
            .execute(
                "INSERT INTO tracks (path, title, artist, artist_mbid, play_count, added_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                rusqlite::params![
                    format!("/music/artist-{index}.flac"),
                    format!("Track {index}"),
                    format!("Artist {index}"),
                    format!("mbid-{index}"),
                    ((count - index) * 10) as i64,
                ],
            )
            .unwrap();
    }
}

fn ledger_key(index: usize) -> String {
    format!("artist {index}")
}

#[test]
fn run_stops_after_three_consecutive_failures_and_leaves_the_rest_unattempted() {
    let conn = migrated_conn();
    seed_artists(&conn, 6);
    let calls = std::cell::Cell::new(0);
    let mut fetch = |_url: &str| {
        calls.set(calls.get() + 1);
        Err(FetchError::Transport)
    };

    let report = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
    )
    .unwrap();

    assert_eq!(
        calls.get(),
        3,
        "the circuit breaker must stop the run at MAX_CONSECUTIVE_FAILURES, not burn a request per candidate"
    );
    assert_eq!(report.failed, 3);
    assert_eq!(report.artists_skipped, 3);
    assert_eq!(report.artists_queued, 6);

    for index in 0..3 {
        assert!(
            crate::artist_news_ledger::last_attempt_at(conn.conn(), &ledger_key(index))
                .unwrap()
                .is_some(),
            "artist {index} was attempted and must have a ledger row"
        );
    }
    for index in 3..6 {
        assert_eq!(
            crate::artist_news_ledger::last_attempt_at(conn.conn(), &ledger_key(index)).unwrap(),
            None,
            "artist {index} was never attempted and must get no ledger row, so it stays due"
        );
    }
}

#[test]
fn run_stops_immediately_on_a_rate_limited_failure() {
    let conn = migrated_conn();
    seed_artists(&conn, 3);
    let calls = std::cell::Cell::new(0);
    let mut fetch = |_url: &str| {
        calls.set(calls.get() + 1);
        Err(FetchError::HttpStatus(429))
    };

    let report = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
    )
    .unwrap();

    assert_eq!(
        calls.get(),
        1,
        "a 429 must stop the run on the first failure, not wait for the third"
    );
    assert_eq!(report.failed, 1);
    assert_eq!(report.artists_skipped, 2);
}

#[test]
fn non_consecutive_failures_do_not_stop_the_run() {
    let conn = migrated_conn();
    seed_artists(&conn, 5);
    // fail, ok, fail, ok, fail — never three in a row.
    let mut fetch = |url: &str| {
        let failing = ["mbid-0", "mbid-2", "mbid-4"]
            .iter()
            .any(|mbid| url.contains(mbid));
        if failing {
            Err(FetchError::Transport)
        } else {
            Ok(r#"{"release-groups":[]}"#.to_string())
        }
    };

    let report = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
    )
    .unwrap();

    assert_eq!(report.failed, 3);
    assert_eq!(
        report.artists_skipped, 0,
        "no run of three consecutive failures ever occurred, so nothing may be skipped"
    );
    for index in 0..5 {
        assert!(
            crate::artist_news_ledger::last_attempt_at(conn.conn(), &ledger_key(index))
                .unwrap()
                .is_some(),
            "artist {index} must have been attempted"
        );
    }
}

#[test]
fn the_next_check_attempts_the_candidates_an_early_stop_left_unattempted() {
    let conn = migrated_conn();
    seed_artists(&conn, 6);
    let mut failing_fetch = |_url: &str| Err(FetchError::Transport);

    let first = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut failing_fetch,
    )
    .unwrap();
    assert_eq!(first.failed, 3);
    assert_eq!(first.artists_skipped, 3);
    for index in 3..6 {
        assert_eq!(
            crate::artist_news_ledger::last_attempt_at(conn.conn(), &ledger_key(index)).unwrap(),
            None,
            "artist {index} must still be unattempted after the first, aborted run"
        );
    }

    // The source has recovered by the next check.
    let mut recovered_fetch = |_url: &str| Ok(r#"{"release-groups":[]}"#.to_string());
    let second = refresh_with(
        &conn,
        date(),
        2_000,
        FetchScope::TopArtists,
        false,
        &mut recovered_fetch,
    )
    .unwrap();

    assert_eq!(
        second.artists_fetched, 6,
        "the never-attempted candidates from the first run must be due at the next check, \
         same as the ones that were recorded as failed"
    );
    for index in 0..6 {
        assert_eq!(
            crate::artist_news_ledger::last_attempt_at(conn.conn(), &ledger_key(index)).unwrap(),
            Some(2_000)
        );
    }
}

#[test]
fn an_unmatched_search_result_resets_the_consecutive_failure_counter() {
    let conn = migrated_conn();
    // No stored MBID: every candidate's first request is an artist search,
    // not a release-groups fetch.
    let names = ["solo0", "solo1", "solo2", "solo3", "solo4"];
    for (index, name) in names.iter().enumerate() {
        conn.conn()
            .execute(
                "INSERT INTO tracks (path, title, artist, play_count, added_at) \
                 VALUES (?1, ?2, ?3, ?4, 0)",
                rusqlite::params![
                    format!("/music/{name}.flac"),
                    format!("Track {index}"),
                    name,
                    ((names.len() - index) * 10) as i64,
                ],
            )
            .unwrap();
    }
    // Every request fails except "solo2"'s own search, which succeeds with
    // no match at all — an `Unmatched` outcome, not a failure — so the
    // sequence is fail, fail, unmatched, fail, fail.
    let mut fetch = |url: &str| {
        if url.contains("solo2") {
            Ok(r#"{"artists":[]}"#.to_string())
        } else {
            Err(FetchError::Transport)
        }
    };

    let report = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
    )
    .unwrap();

    assert_eq!(report.failed, 4, "four failures: indices 0, 1, 3, 4");
    assert_eq!(report.unmatched, 1);
    assert_eq!(
        report.artists_skipped, 0,
        "the Unmatched result at index 2 must reset the counter, so the two failures \
         after it never reach MAX_CONSECUTIVE_FAILURES"
    );
    for name in names {
        assert!(
            crate::artist_news_ledger::last_attempt_at(conn.conn(), name)
                .unwrap()
                .is_some(),
            "{name} must have been attempted"
        );
    }
}

#[test]
fn the_cache_fresh_skip_does_not_reset_the_consecutive_failure_counter() {
    let conn = migrated_conn();
    seed_artists(&conn, 6);
    // "artist 2" already has a fresh, successful attempt from just before
    // this run, so `artist_cache_is_fresh` skips it without making any
    // request — that skip must neither count as nor reset a failure.
    crate::artist_news_ledger::record_attempt(
        conn.conn(),
        &ledger_key(2),
        Some("mbid-2"),
        999,
        crate::artist_news_ledger::FetchOutcome::Ok,
        0,
    )
    .unwrap();
    let mut fetch = |_url: &str| Err(FetchError::Transport);

    let report = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
    )
    .unwrap();

    assert_eq!(
        report.failed, 3,
        "0 and 1 fail, 2 is skipped untouched, 3 fails and trips the breaker"
    );
    assert_eq!(
        report.artists_skipped, 2,
        "candidates 4 and 5 must be left unattempted"
    );
    assert_eq!(
        crate::artist_news_ledger::last_attempt_at(conn.conn(), &ledger_key(2)).unwrap(),
        Some(999),
        "the fresh candidate's ledger row must be untouched by this run"
    );
    for index in [4, 5] {
        assert_eq!(
            crate::artist_news_ledger::last_attempt_at(conn.conn(), &ledger_key(index)).unwrap(),
            None,
            "artist {index} must still be unattempted"
        );
    }
}

#[test]
fn progress_reports_no_event_for_a_candidate_the_early_stop_never_reached() {
    let db = migrated_conn();
    seed_artists(&db, 6);
    let mut fetch = |_url: &str| Err(FetchError::Transport);
    let mut progress = Vec::new();
    let mut completion_time = || 2_000;

    let report = refresh_with_progress_at(
        &db,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut RefreshHooks {
            fetch: &mut fetch,
            on_progress: &mut |update| progress.push(update),
            completion_time: &mut completion_time,
        },
    )
    .unwrap();

    assert_eq!(report.artists_skipped, 3);
    assert_eq!(
        progress,
        vec![
            RefreshProgress {
                checked: 0,
                total: 6
            },
            RefreshProgress {
                checked: 1,
                total: 6
            },
            RefreshProgress {
                checked: 2,
                total: 6
            },
            RefreshProgress {
                checked: 3,
                total: 6
            },
        ],
        "no progress event may be emitted for a candidate the run never attempted"
    );
}
