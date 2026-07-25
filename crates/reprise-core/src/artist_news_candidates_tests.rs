//! Tests for fetch-scope configuration and candidate selection/rotation in
//! `artist_news_candidates.rs`. Split out of `artist_news_tests.rs` purely
//! to keep both files under the project's 800-line rule — a pure move, not
//! a rewrite.

use crate::artist_news::{
    artists_for_fetch, configured_fetch_scope, set_fetch_all_artists, FetchScope,
};

fn migrated_conn() -> rusqlite::Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

#[test]
fn nr_1a_fetch_queue_prioritizes_top_artists_and_includes_the_never_checked_rest() {
    let conn = migrated_conn();
    for index in 0..27 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, play_count, added_at) \
             VALUES (?1, 'Track', ?2, ?3, 0)",
            rusqlite::params![
                format!("/music/{index}.flac"),
                format!("Artist {index:02}"),
                100 - index
            ],
        )
        .unwrap();
    }

    let top = artists_for_fetch(&conn, FetchScope::TopArtists).unwrap();
    let all = artists_for_fetch(&conn, FetchScope::AllArtists).unwrap();

    assert_eq!(top.len(), 20);
    assert_eq!(top[0].name, "Artist 00");
    assert_eq!(top[19].name, "Artist 19");
    assert_eq!(
        all.len(),
        27,
        "the 7-artist rest group fits well within REST_ARTISTS_PER_RUN"
    );
    assert_eq!(all[20].name, "Artist 20");
    assert_eq!(all[26].name, "Artist 26");
}

#[test]
fn nr_7_fetch_scope_defaults_to_top_and_round_trips_all_artists() {
    let conn = migrated_conn();
    assert_eq!(
        configured_fetch_scope(&conn).unwrap(),
        FetchScope::TopArtists
    );

    set_fetch_all_artists(&conn, true).unwrap();

    assert_eq!(
        configured_fetch_scope(&conn).unwrap(),
        FetchScope::AllArtists
    );
}

#[test]
fn rotation_prefers_never_checked_artists_over_play_count() {
    let conn = migrated_conn();
    // 22 artists so the rest group is non-empty (TOP_ARTIST_COUNT = 20).
    // Play counts descend, so "artist-21" and "artist-22" are the tail.
    for index in 1..=22 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', ?2, 'Album', ?3, 0)",
            rusqlite::params![
                format!("/music/{index}.flac"),
                format!("artist-{index:02}"),
                100 - index,
            ],
        )
        .unwrap();
    }
    // The very last artist by plays was checked long ago; the second-to-last
    // was checked just now. Only the stale one may come up.
    crate::artist_news_ledger::record_attempt(
        &conn,
        "artist-21",
        None,
        9_000,
        crate::artist_news_ledger::FetchOutcome::Ok,
        0,
    )
    .unwrap();

    let candidates = artists_for_fetch(&conn, FetchScope::AllArtists).unwrap();
    let names = candidates
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(names.len(), 22, "top 20 plus the rest group");
    assert_eq!(
        names[20], "artist-22",
        "never-checked artist must come before a recently checked one"
    );
    assert_eq!(names[21], "artist-21");
}

#[test]
fn rest_group_is_capped_at_rest_artists_per_run_and_keeps_the_stalest() {
    // The rotation test above only ever has 2 (this file) or 7 (the doctest-
    // style comment on `artists_for_fetch`) rest-group candidates — nowhere
    // near REST_ARTISTS_PER_RUN (30), so truncation itself never engages.
    // This test puts 50 candidates in the rest group (comfortably above the
    // cap) and checks not just the count but *which* 30 survive: the
    // never-checked artists first, then the ones with the oldest
    // `last_attempt_at` — never an arbitrary 30.
    let conn = migrated_conn();
    for index in 1..=20 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', ?2, 'Album', ?3, 0)",
            rusqlite::params![
                format!("/music/top-{index}.flac"),
                format!("top-{index:02}"),
                300 - index,
            ],
        )
        .unwrap();
    }
    for index in 1..=50 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', ?2, 'Album', ?3, 0)",
            rusqlite::params![
                format!("/music/rest-{index}.flac"),
                format!("rest-{index:02}"),
                200 - index,
            ],
        )
        .unwrap();
    }
    // rest-01..rest-15 are never checked (no ledger row at all) — `None`
    // sorts before every `Some`, so all 15 must be picked.
    // rest-16..rest-50 (35 artists) each get a distinct, increasing
    // `last_attempt_at`: rest-16 is the oldest, rest-50 the newest. Only the
    // 15 oldest of these (rest-16..rest-30) are needed to fill the
    // remaining slots up to REST_ARTISTS_PER_RUN (30).
    for index in 16..=50 {
        crate::artist_news_ledger::record_attempt(
            &conn,
            &format!("rest-{index:02}"),
            None,
            i64::from(index - 15) * 100,
            crate::artist_news_ledger::FetchOutcome::Ok,
            0,
        )
        .unwrap();
    }

    let candidates = artists_for_fetch(&conn, FetchScope::AllArtists).unwrap();
    let names = candidates
        .iter()
        .map(|candidate| candidate.name.clone())
        .collect::<Vec<_>>();

    assert_eq!(
        names.len(),
        50,
        "TOP_ARTIST_COUNT (20) plus REST_ARTISTS_PER_RUN (30), not all 70 candidates"
    );

    let expected_rest_group: Vec<String> = (1..=15)
        .map(|index| format!("rest-{index:02}"))
        .chain((16..=30).map(|index| format!("rest-{index:02}")))
        .collect();
    assert_eq!(
        names[20..].to_vec(),
        expected_rest_group,
        "the rest group must be exactly the 15 never-checked artists followed \
         by the 15 with the oldest last_attempt_at — the 20 artists with a \
         more recent attempt (rest-31..rest-50) must be excluded"
    );
}

#[test]
fn top_artists_scope_ignores_the_rest_group_entirely() {
    let conn = migrated_conn();
    for index in 1..=22 {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, album, play_count, added_at) \
             VALUES (?1, 'T', ?2, 'Album', ?3, 0)",
            rusqlite::params![
                format!("/music/{index}.flac"),
                format!("artist-{index:02}"),
                100 - index,
            ],
        )
        .unwrap();
    }
    let candidates = artists_for_fetch(&conn, FetchScope::TopArtists).unwrap();
    assert_eq!(candidates.len(), 20);
}

#[test]
fn configured_scope_round_trips_without_a_date() {
    let conn = migrated_conn();
    assert_eq!(
        configured_fetch_scope(&conn).unwrap(),
        FetchScope::TopArtists
    );
    set_fetch_all_artists(&conn, true).unwrap();
    assert_eq!(
        configured_fetch_scope(&conn).unwrap(),
        FetchScope::AllArtists
    );
}

#[test]
fn include_singles_setting_defaults_to_off_and_round_trips() {
    let conn = migrated_conn();
    assert!(!crate::artist_news::include_singles(&conn).unwrap());
    crate::artist_news::set_include_singles(&conn, true).unwrap();
    assert!(crate::artist_news::include_singles(&conn).unwrap());
    crate::artist_news::set_include_singles(&conn, false).unwrap();
    assert!(!crate::artist_news::include_singles(&conn).unwrap());
}
