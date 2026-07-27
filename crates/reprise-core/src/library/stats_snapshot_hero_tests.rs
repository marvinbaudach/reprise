use chrono::{TimeZone, Utc};
use rusqlite::{params, Connection};

use super::compute;
use crate::library::stats_period::StatsPeriod;

const NOW_2026_07_19: i64 = 1_784_424_000;

#[test]
fn stats_11_pace_projects_only_year_to_date() {
    let conn = migrated_conn();
    insert_event(&conn, timestamp(2026, 3, 1), 200_000);

    let year_to_date = compute(&conn, StatsPeriod::YearToDate(2026), NOW_2026_07_19, &Utc).unwrap();
    assert_eq!(year_to_date.hero.pace_projection_ms, Some(365_000));

    for period in [
        StatsPeriod::Year(2026),
        StatsPeriod::AllTime,
        StatsPeriod::Last30Days,
    ] {
        let snapshot = compute(&conn, period, NOW_2026_07_19, &Utc).unwrap();
        assert_eq!(snapshot.hero.pace_projection_ms, None, "period: {period:?}");
    }
}

#[test]
fn stats_11_previous_ms_carries_the_seasonal_baseline() {
    let conn = migrated_conn();
    insert_event(&conn, timestamp(2025, 3, 1), 100_000);
    insert_event(&conn, timestamp(2026, 3, 1), 200_000);

    let year_to_date = compute(&conn, StatsPeriod::YearToDate(2026), NOW_2026_07_19, &Utc).unwrap();
    assert_eq!(year_to_date.hero.previous_ms, Some(100_000));

    let all_time = compute(&conn, StatsPeriod::AllTime, NOW_2026_07_19, &Utc).unwrap();
    assert_eq!(all_time.hero.previous_ms, None);
}

fn migrated_conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, artist, album, duration_ms, play_count, added_at) \
         VALUES (1, '/music/track.flac', 'Track', 'Artist', 'Album', 1000000, 0, 0)",
        [],
    )
    .unwrap();
    conn
}

fn insert_event(conn: &Connection, played_at: i64, ms_played: i64) {
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, ?1, ?2)",
        params![played_at, ms_played],
    )
    .unwrap();
}

fn timestamp(year: i32, month: u32, day: u32) -> i64 {
    Utc.with_ymd_and_hms(year, month, day, 12, 0, 0)
        .single()
        .unwrap()
        .timestamp()
}
