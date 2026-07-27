//! Tests for the refresh pipeline and its ledger/freshness interactions in
//! `artist_news_pipeline.rs`. Split out of `artist_news_tests.rs` purely to
//! keep both files under the project's 800-line rule — a pure move, not a
//! rewrite.

use chrono::NaiveDate;

use crate::artist_news::{
    mark_releases_seen, most_played_album_track_path, query_releases, refresh_with,
    unseen_release_count, FetchScope,
};

const ARTIST_ID: &str = "83d91898-7763-47d7-b03b-b92132375c47";
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

fn no_accent(_conn: &rusqlite::Connection, _artist: &str) -> Option<String> {
    None
}

#[test]
fn nr_1a_tag_mbid_skips_search_and_persists_releases() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, album, play_count, added_at) \
         VALUES ('/music/one.flac', 'One', 'Pink Floyd', ?1, 'Local Album', 20, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let mut urls = Vec::new();
    let mut fetch = |url: &str| {
        urls.push(url.to_string());
        Ok(RELEASES.to_string())
    };

    let report = refresh_with(
        &conn,
        date(),
        1_000_000,
        FetchScope::TopArtists,
        true,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();

    assert_eq!(report.artists_fetched, 1);
    assert_eq!(report.releases_upserted, 2);
    assert_eq!(urls.len(), 1);
    assert!(urls[0].contains("/release-group?"));
    let releases = query_releases(&conn, true, date()).unwrap();
    assert_eq!(releases.len(), 2);
    assert!(releases.iter().all(|release| {
        release.artist_name == "Pink Floyd" && release.artist_mbid == ARTIST_ID
    }));

    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, album, added_at) \
         VALUES ('/music/two.flac', 'Two', 'Pink Floyd', ?1, 'Future Album', 0)",
        [ARTIST_ID],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, album, added_at) \
         VALUES ('/music/three.flac', 'Three', 'Pink Floyd', ?1, 'Future Album', 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let after_import = query_releases(&conn, true, date()).unwrap();
    assert_eq!(
        after_import.len(),
        2,
        "an in-library release is annotated, not dropped"
    );
    let future_album = after_import
        .iter()
        .find(|release| release.title == "Future Album")
        .unwrap();
    assert_eq!(
        future_album.presence,
        crate::artist_news::LibraryPresence::Complete,
        "the newly imported album (two local tracks) is marked fully owned"
    );
    let recent_ep = after_import
        .iter()
        .find(|release| release.title == "Recent EP")
        .unwrap();
    assert_eq!(
        recent_ep.presence,
        crate::artist_news::LibraryPresence::Absent,
        "an album with no local match stays absent"
    );
}

#[test]
fn nr_1a_name_resolution_persists_positive_and_negative_results() {
    let conn = migrated_conn();
    for (path, artist, plays) in [
        ("/music/pink.flac", "Pink Floyd", 20),
        ("/music/unknown.flac", "Unknown", 10),
    ] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, play_count, added_at) \
             VALUES (?1, 'Track', ?2, ?3, 0)",
            rusqlite::params![path, artist, plays],
        )
        .unwrap();
    }
    let mut fetch = |url: &str| {
        if url.contains("Pink%20Floyd") {
            Ok(r#"{"artists":[{"id":"83d91898-7763-47d7-b03b-b92132375c47","name":"Pink Floyd","score":100}]}"#.to_string())
        } else if url.contains("Unknown") {
            Ok(r#"{"artists":[]}"#.to_string())
        } else {
            Ok(RELEASES.to_string())
        }
    };

    let report = refresh_with(
        &conn,
        date(),
        1_000_000,
        FetchScope::TopArtists,
        true,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();

    assert_eq!((report.artists_fetched, report.unmatched), (1, 1));
    let positive: (Option<String>, i64) = conn
        .query_row(
            "SELECT artist_mbid, artist_mbid_negative FROM tracks WHERE artist = 'Pink Floyd'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(positive, (Some(ARTIST_ID.into()), 0));
    let negative: (Option<String>, i64) = conn
        .query_row(
            "SELECT artist_mbid, artist_mbid_negative FROM tracks WHERE artist = 'Unknown'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(negative, (None, 1));
}

#[test]
fn nr_2_fallback_accent_is_computed_when_release_is_inserted() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, play_count, added_at) \
         VALUES ('/music/accent.flac', 'Track', 'Pink Floyd', ?1, 10, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let mut fetch = |_url: &str| Ok(RELEASES.to_string());
    let mut accent = |_conn: &rusqlite::Connection, artist: &str| {
        assert_eq!(artist, "Pink Floyd");
        Some("#123456".to_string())
    };

    refresh_with(
        &conn,
        date(),
        1_000_000,
        FetchScope::TopArtists,
        true,
        &mut fetch,
        &mut accent,
    )
    .unwrap();

    let stored: String = conn
        .query_row(
            "SELECT fallback_accent FROM new_releases LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, "#123456");
}

#[test]
fn nr_2_accent_source_uses_the_most_played_album() {
    let conn = migrated_conn();
    for (path, album, plays) in [
        ("/music/a-one.flac", "Album A", 5),
        ("/music/a-two.flac", "Album A", 4),
        ("/music/b.flac", "Album B", 8),
    ] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'Track', 'Artist', ?2, ?3, 0)",
            rusqlite::params![path, album, plays],
        )
        .unwrap();
    }

    let path = most_played_album_track_path(&conn, "Artist").unwrap();

    assert_eq!(
        path.as_deref(),
        Some(std::path::Path::new("/music/a-one.flac"))
    );
}

#[test]
fn nr_3a_seen_item_not_rebadged() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, added_at) \
         VALUES ('/music/track.flac', 'Track', 'Pink Floyd', ?1, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let first_payload = r#"{"release-groups":[
      {"id":"known","title":"Known","first-release-date":"2026-08-01","primary-type":"Album"}
    ]}"#;
    let mut first_fetch = |_url: &str| Ok(first_payload.to_string());
    refresh_with(
        &conn,
        date(),
        10,
        FetchScope::TopArtists,
        true,
        &mut first_fetch,
        &mut no_accent,
    )
    .unwrap();
    mark_releases_seen(&conn, &["known".into()], 20).unwrap();

    let second_payload = r#"{"release-groups":[
      {"id":"known","title":"Known","first-release-date":"2026-08-01","primary-type":"Album"},
      {"id":"new","title":"New","first-release-date":"2026-08-02","primary-type":"Album"}
    ]}"#;
    let mut second_fetch = |_url: &str| Ok(second_payload.to_string());
    refresh_with(
        &conn,
        date(),
        30,
        FetchScope::TopArtists,
        true,
        &mut second_fetch,
        &mut no_accent,
    )
    .unwrap();

    assert_eq!(unseen_release_count(&conn).unwrap(), 1);
    let known_seen: Option<i64> = conn
        .query_row(
            "SELECT seen_at FROM new_releases WHERE release_group_mbid = 'known'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(known_seen, Some(20));
}

#[test]
fn first_seen_is_set_on_insert_and_preserved_across_upsert() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, play_count, added_at) \
         VALUES ('/music/one.flac', 'One', 'Pink Floyd', ?1, 10, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let mut fetch = |_url: &str| Ok(RELEASES.to_string());

    refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        true,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();

    let (first_seen, fetched_at): (i64, i64) = conn
        .query_row(
            "SELECT first_seen, fetched_at FROM new_releases WHERE release_group_mbid = '1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(first_seen, 1_000);
    assert_eq!(fetched_at, 1_000);

    refresh_with(
        &conn,
        date(),
        2_000,
        FetchScope::TopArtists,
        true,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();

    let (first_seen_after, fetched_at_after): (i64, i64) = conn
        .query_row(
            "SELECT first_seen, fetched_at FROM new_releases WHERE release_group_mbid = '1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        first_seen_after, 1_000,
        "first_seen must never move on re-fetch"
    );
    assert_eq!(
        fetched_at_after, 2_000,
        "fetched_at must still track the latest fetch"
    );
}

#[test]
fn ledger_marks_artist_without_news_fresh_and_second_run_skips_it() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, album, play_count, added_at) \
         VALUES ('/music/one.flac', 'One', 'Pink Floyd', ?1, 'Local Album', 20, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    // No release groups at all — the artist has nothing to report.
    let empty = r#"{"release-groups":[]}"#;
    // A `Cell` rather than a plain counter: the closure only needs a shared
    // reference to bump it, so `calls.get()` can be read between the two
    // `refresh_with` calls without conflicting with the still-live `fetch`.
    let calls = std::cell::Cell::new(0);
    let mut fetch = |_url: &str| {
        calls.set(calls.get() + 1);
        Ok(empty.to_string())
    };

    let first = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(first.artists_fetched, 1);
    let after_first = calls.get();
    assert!(after_first > 0, "first run must hit the network");

    let second = refresh_with(
        &conn,
        date(),
        2_000, // well inside FETCH_TTL_SECONDS
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(
        second.artists_fetched, 0,
        "artist with no news must count as fresh, not be re-fetched"
    );
    assert_eq!(
        calls.get(),
        after_first,
        "second run must issue no requests"
    );
}

#[test]
fn ledger_records_unmatched_outcome_and_negative_match_excludes_future_search() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/two.flac', 'Two', 'Nobody At All', 'Local Album', 5, 0)",
        [],
    )
    .unwrap();
    let calls = std::cell::Cell::new(0);
    let mut fetch = |_url: &str| {
        calls.set(calls.get() + 1);
        Ok(r#"{"artists":[]}"#.to_string())
    };

    let first = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(first.unmatched, 1);
    let after_first = calls.get();

    // Assert directly against the ledger — this is the actual requirement
    // under test. `normalize("Nobody At All")` is "nobody at all".
    let artist_key = "nobody at all";
    assert_eq!(
        crate::artist_news_ledger::last_attempt_at(&conn, artist_key).unwrap(),
        Some(1_000),
        "an unmatched artist must still get a ledger entry for its attempt"
    );
    let outcome: String = conn
        .query_row(
            "SELECT last_outcome FROM artist_news_fetch WHERE artist_key = ?1",
            [artist_key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(outcome, "unmatched");

    // This second call does NOT exercise the ledger's TTL-based freshness
    // check at all: `persist_artist_match` already set
    // `artist_mbid_negative = 1` on the first run, and
    // `artists_for_fetch`'s `HAVING` clause excludes that artist from the
    // candidate list outright. So this only proves a negatively-matched
    // artist stops costing search requests forever, once the ledger
    // assertions above have already pinned the actual requirement.
    refresh_with(
        &conn,
        date(),
        2_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(
        calls.get(),
        after_first,
        "a negatively-matched artist must not be searched again"
    );
}

#[test]
fn ledger_records_failed_outcome_for_a_failed_artist_search() {
    // Same shape as `ledger_records_unmatched_outcome_and_negative_match_excludes_future_search`,
    // but the artist-search request itself fails (transport error) rather
    // than succeeding with an empty match list. `resolve_artist_mbid`
    // increments `report.failed` for this branch, and the ledger entry must
    // say so too — `unmatched` would mean "we looked and found nothing",
    // which is not what happened here.
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
         VALUES ('/music/two.flac', 'Two', 'Nobody At All', 'Local Album', 5, 0)",
        [],
    )
    .unwrap();
    let mut fetch = |_url: &str| Err(crate::musicbrainz::FetchError::Transport);

    let first = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(first.failed, 1);
    assert_eq!(first.unmatched, 0);

    let artist_key = "nobody at all";
    assert_eq!(
        crate::artist_news_ledger::last_attempt_at(&conn, artist_key).unwrap(),
        Some(1_000),
        "a failed artist-search must still get a ledger entry for its attempt"
    );
    let outcome: String = conn
        .query_row(
            "SELECT last_outcome FROM artist_news_fetch WHERE artist_key = ?1",
            [artist_key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        outcome, "failed",
        "a failed artist-search request must be recorded as 'failed', not 'unmatched'"
    );
}

#[test]
fn ledger_records_failed_fetch_and_ttl_prevents_a_retry_within_the_window() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (path, title, artist, artist_mbid, play_count, added_at) \
         VALUES ('/music/one.flac', 'One', 'Pink Floyd', ?1, 20, 0)",
        [ARTIST_ID],
    )
    .unwrap();
    let calls = std::cell::Cell::new(0);
    let mut fetch = |_url: &str| {
        calls.set(calls.get() + 1);
        Err(crate::musicbrainz::FetchError::Transport)
    };

    let first = refresh_with(
        &conn,
        date(),
        1_000,
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(first.failed, 1);
    let after_first = calls.get();
    assert_eq!(
        after_first, 1,
        "a known MBID must skip the search request and hit only release-groups"
    );

    let artist_key = "pink floyd";
    assert_eq!(
        crate::artist_news_ledger::last_attempt_at(&conn, artist_key).unwrap(),
        Some(1_000),
        "a failed fetch must still be recorded in the ledger"
    );
    let outcome: String = conn
        .query_row(
            "SELECT last_outcome FROM artist_news_fetch WHERE artist_key = ?1",
            [artist_key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(outcome, "failed");

    let second = refresh_with(
        &conn,
        date(),
        2_000, // well inside FETCH_TTL_SECONDS: no separate failure backoff exists
        FetchScope::TopArtists,
        false,
        &mut fetch,
        &mut no_accent,
    )
    .unwrap();
    assert_eq!(
        second.failed, 0,
        "a permanently-failing artist must count as fresh inside the TTL window"
    );
    assert_eq!(
        calls.get(),
        after_first,
        "no further requests may be issued for it within the TTL window"
    );
}

#[test]
fn latest_fetched_at_reads_the_ledger_not_found_releases() {
    let conn = migrated_conn();
    assert_eq!(
        crate::artist_news::latest_fetched_at(&conn).unwrap(),
        None,
        "empty ledger means never fetched"
    );
    crate::artist_news_ledger::record_attempt(
        &conn,
        "pink floyd",
        None,
        4_242,
        crate::artist_news_ledger::FetchOutcome::Ok,
        0,
    )
    .unwrap();
    assert_eq!(
        crate::artist_news::latest_fetched_at(&conn).unwrap(),
        Some(4_242),
        "an attempt without any found release must still count"
    );
}
