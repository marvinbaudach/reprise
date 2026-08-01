//! Progress-contract tests for the New Releases refresh pipeline.

use chrono::NaiveDate;

use crate::artist_news::{refresh_with_progress_at, FetchScope, RefreshHooks, RefreshProgress};

fn no_accent(_db: &crate::db::Db, _artist: &str) -> Option<String> {
    None
}

#[test]
fn nr_22_refresh_reports_determinate_progress_for_every_queued_artist() {
    let db = crate::db::Db::open_in_memory().unwrap();
    for (path, artist, mbid, plays) in [
        (
            "/music/one.flac",
            "First Artist",
            "11111111-1111-1111-1111-111111111111",
            20,
        ),
        (
            "/music/two.flac",
            "Second Artist",
            "22222222-2222-2222-2222-222222222222",
            10,
        ),
    ] {
        db.conn()
            .execute(
                "INSERT INTO tracks (path, title, artist, artist_mbid, play_count, added_at) \
                 VALUES (?1, 'Track', ?2, ?3, ?4, 0)",
                rusqlite::params![path, artist, mbid, plays],
            )
            .unwrap();
    }
    let mut fetch = |_url: &str| Ok(r#"{"release-groups":[]}"#.to_string());
    let mut progress = Vec::new();
    let mut completion_time = || 1_000_360;

    let report = refresh_with_progress_at(
        &db,
        NaiveDate::from_ymd_opt(2026, 7, 13).unwrap(),
        1_000_000,
        FetchScope::TopArtists,
        true,
        &mut RefreshHooks {
            fetch: &mut fetch,
            fallback_accent: &mut no_accent,
            on_progress: &mut |update| progress.push(update),
            completion_time: &mut completion_time,
        },
    )
    .unwrap();

    assert_eq!(report.artists_queued, 2);
    assert_eq!(report.artists_fetched, 2);
    assert_eq!(
        progress,
        vec![
            RefreshProgress {
                checked: 0,
                total: 2,
            },
            RefreshProgress {
                checked: 1,
                total: 2,
            },
            RefreshProgress {
                checked: 2,
                total: 2,
            },
        ]
    );
    assert_eq!(
        crate::artist_news::latest_fetched_at(&db).unwrap(),
        Some(1_000_360),
        "the displayed age must start when the run finishes, not when it starts"
    );
}

#[test]
fn nr_22_successful_empty_refresh_still_records_its_completion_time() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let mut fetch = |_url: &str| -> Result<String, crate::musicbrainz::FetchError> {
        panic!("an empty refresh must not issue a request")
    };
    let mut progress = Vec::new();
    let mut completion_time = || 1_000_360;

    let report = refresh_with_progress_at(
        &db,
        NaiveDate::from_ymd_opt(2026, 7, 13).unwrap(),
        1_000_000,
        FetchScope::TopArtists,
        true,
        &mut RefreshHooks {
            fetch: &mut fetch,
            fallback_accent: &mut no_accent,
            on_progress: &mut |update| progress.push(update),
            completion_time: &mut completion_time,
        },
    )
    .unwrap();

    assert_eq!(report.artists_queued, 0);
    assert_eq!(
        progress,
        vec![RefreshProgress {
            checked: 0,
            total: 0
        }]
    );
    assert_eq!(
        crate::artist_news::latest_fetched_at(&db).unwrap(),
        Some(1_000_360)
    );
}

#[test]
fn nr_22_failed_refresh_preserves_the_previous_successful_age() {
    let db = crate::db::Db::open_in_memory().unwrap();
    crate::artist_news_ledger::record_attempt(
        db.conn(),
        "previous artist",
        None,
        640,
        crate::artist_news_ledger::FetchOutcome::Ok,
        0,
    )
    .unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks (path, title, artist, artist_mbid, play_count, added_at) \
             VALUES ('/music/failure.flac', 'Track', 'Failure Artist', \
             '11111111-1111-1111-1111-111111111111', 20, 0)",
            [],
        )
        .unwrap();
    let mut fetch = |_url: &str| Err(crate::musicbrainz::FetchError::Transport);
    let mut progress = Vec::new();
    let mut completion_time = || panic!("a failed run has no completion timestamp");

    let report = refresh_with_progress_at(
        &db,
        NaiveDate::from_ymd_opt(2026, 7, 13).unwrap(),
        1_000,
        FetchScope::TopArtists,
        true,
        &mut RefreshHooks {
            fetch: &mut fetch,
            fallback_accent: &mut no_accent,
            on_progress: &mut |update| progress.push(update),
            completion_time: &mut completion_time,
        },
    )
    .unwrap();

    assert_eq!(report.failed, 1);
    assert_eq!(
        progress.last(),
        Some(&RefreshProgress {
            checked: 1,
            total: 1
        })
    );
    assert_eq!(
        crate::artist_news::latest_fetched_at(&db).unwrap(),
        Some(640),
        "a failed attempt must not make stale cached data look newly updated"
    );
}
