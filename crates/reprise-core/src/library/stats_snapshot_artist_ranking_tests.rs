use chrono::{TimeZone, Utc};
use rusqlite::params;

use super::{compute, SortBy};
use crate::library::stats_period::StatsPeriod;

const NOW_2026_07_19: i64 = 1_784_424_000;

/// The toggle changes the ranking, not just its labels: a short-track band with
/// many plays leads by plays and trails by time.
#[test]
fn stats_23_top_artists_sorted_follows_the_chosen_metric() {
    let conn = migrated_conn();
    // "Sprinter": six short plays. "Marathon": two long ones.
    insert_track(&conn, 1, "Short", "Sprinter", 60_000);
    insert_track(&conn, 2, "Long", "Marathon", 600_000);
    for play in 0..6 {
        insert_event(&conn, 1, timestamp(2026, 2, 1, 12, play), 60_000);
    }
    for play in 0..2 {
        insert_event(&conn, 2, timestamp(2026, 2, 2, 12, play), 600_000);
    }

    let snapshot = compute(&conn, StatsPeriod::AllTime, NOW_2026_07_19, &Utc).unwrap();

    let by_plays = snapshot.top_artists_sorted(SortBy::Plays);
    let by_time = snapshot.top_artists_sorted(SortBy::Time);
    assert_eq!(by_plays[0].group.label, "Sprinter");
    assert_eq!(by_time[0].group.label, "Marathon");
    assert_eq!(by_plays.len(), by_time.len(), "sorting drops no artist");
}

/// The share is the leader's share of all artist listening — so it has to be
/// recomputed when the toggle hands the lead to somebody else.
#[test]
fn stats_23_artist_share_follows_whoever_leads() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "Short", "Sprinter", 60_000);
    insert_track(&conn, 2, "Long", "Marathon", 600_000);
    for play in 0..6 {
        insert_event(&conn, 1, timestamp(2026, 2, 1, 12, play), 60_000);
    }
    for play in 0..2 {
        insert_event(&conn, 2, timestamp(2026, 2, 2, 12, play), 600_000);
    }

    let snapshot = compute(&conn, StatsPeriod::AllTime, NOW_2026_07_19, &Utc).unwrap();

    let sprinter = &snapshot.top_artists_sorted(SortBy::Plays)[0];
    let marathon = &snapshot.top_artists_sorted(SortBy::Time)[0];
    // 360 s of 1560 s, and 1200 s of 1560 s.
    assert_eq!(snapshot.artist_share_percent(sprinter), 23);
    assert_eq!(snapshot.artist_share_percent(marathon), 77);
}

fn migrated_conn() -> crate::db::Db {
    crate::db::Db::open_in_memory().unwrap()
}

fn insert_track(conn: &crate::db::Db, id: i64, title: &str, artist: &str, duration_ms: i64) {
    conn.conn()
        .execute(
            "INSERT INTO tracks \
             (id, path, title, artist, album, album_artist, genre, duration_ms, \
              play_count, added_at) \
             VALUES (?1, ?2, ?3, ?4, 'Album', '', 'Rock', ?5, 0, 0)",
            params![id, format!("/music/{id}.flac"), title, artist, duration_ms],
        )
        .unwrap();
}

fn insert_event(conn: &crate::db::Db, track_id: i64, played_at: i64, ms_played: i64) {
    conn.conn()
        .execute(
            "INSERT INTO listen_events (track_id, played_at, ms_played) \
             VALUES (?1, ?2, ?3)",
            params![track_id, played_at, ms_played],
        )
        .unwrap();
}

fn timestamp(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .unwrap()
        .timestamp()
}
