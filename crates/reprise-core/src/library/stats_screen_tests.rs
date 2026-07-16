use super::*;

// Fixed UTC anchors so month bucketing is deterministic and timezone-free.
// Reference "now" is 2026-07-14 12:00:00 UTC; its 12-month window spans
// 2025-08 .. 2026-07 inclusive.
const NOW_2026_07_14: i64 = 1_784_030_400;
const T_2025_08_01: i64 = 1_754_006_400; // oldest in-window bucket
const T_2025_07_15: i64 = 1_752_537_600; // one month before the window
const T_2026_01_10: i64 = 1_768_003_200;
const T_2026_07_01: i64 = 1_782_864_000;
const T_2026_07_05: i64 = 1_783_209_600;

fn migrated_conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

fn insert_track(conn: &Connection, id: i64, artist: &str, album: &str) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, added_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        params![id, format!("/x/{id}.flac"), format!("t{id}"), artist, album],
    )
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn insert_track_full(
    conn: &Connection,
    id: i64,
    artist: &str,
    album: &str,
    album_artist: &str,
    genre: &str,
    play_count: i64,
    duration_ms: i64,
    last_played_at: Option<i64>,
) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, album_artist, genre, \
             play_count, duration_ms, last_played_at, added_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
        params![
            id,
            format!("/x/{id}.flac"),
            format!("t{id}"),
            artist,
            album,
            album_artist,
            genre,
            play_count,
            duration_ms,
            last_played_at,
        ],
    )
    .unwrap();
}

#[test]
fn record_listen_event_persists_a_row() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "A", "Alb");
    record_listen_event(&conn, 1, 1_700_000_000, 123_456).unwrap();

    let (track_id, played_at, ms_played): (i64, i64, i64) = conn
        .query_row(
            "SELECT track_id, played_at, ms_played FROM listen_events",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(track_id, 1);
    assert_eq!(played_at, 1_700_000_000);
    assert_eq!(ms_played, 123_456);
}

#[test]
fn monthly_timeseries_returns_twelve_ordered_buckets_with_zero_gaps() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "A", "Alb");
    // Two plays in the current month, one in January, one in the oldest
    // bucket, and one just before the window (must be excluded).
    record_listen_event(&conn, 1, T_2026_07_01, 100_000).unwrap();
    record_listen_event(&conn, 1, T_2026_07_05, 200_000).unwrap();
    record_listen_event(&conn, 1, T_2026_01_10, 50_000).unwrap();
    record_listen_event(&conn, 1, T_2025_08_01, 400_000).unwrap();
    record_listen_event(&conn, 1, T_2025_07_15, 999_999).unwrap();

    let series = monthly_listen_timeseries(&conn, NOW_2026_07_14).unwrap();

    assert_eq!(series.len(), 12);
    assert_eq!(series.first().unwrap().year_month, "2025-08");
    assert_eq!(series.last().unwrap().year_month, "2026-07");

    assert_eq!(series[0].total_ms, 400_000);
    assert_eq!(series[0].listens, 1);
    // 2025-09 .. 2025-12 are empty.
    for bucket in &series[1..5] {
        assert_eq!(bucket.total_ms, 0);
        assert_eq!(bucket.listens, 0);
    }
    assert_eq!(series[5].year_month, "2026-01");
    assert_eq!(series[5].total_ms, 50_000);
    assert_eq!(series[5].listens, 1);
    assert_eq!(series[11].total_ms, 300_000);
    assert_eq!(series[11].listens, 2);

    // The out-of-window event is excluded from every bucket.
    let total: i64 = series.iter().map(|b| b.total_ms).sum();
    assert_eq!(total, 750_000);
}

#[test]
fn monthly_timeseries_is_all_zero_for_an_empty_library() {
    let conn = migrated_conn();
    let series = monthly_listen_timeseries(&conn, NOW_2026_07_14).unwrap();
    assert_eq!(series.len(), 12);
    assert!(series.iter().all(|b| b.total_ms == 0 && b.listens == 0));
}

#[test]
fn headline_totals_sum_play_time_and_play_count() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO tracks (id, path, title, play_count, duration_ms, added_at) \
             VALUES (1, '/x/1.flac', 't1', 3, 200000, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, play_count, duration_ms, added_at) \
             VALUES (2, '/x/2.flac', 't2', 2, 100000, 0)",
        [],
    )
    .unwrap();
    // Never-played track contributes nothing.
    conn.execute(
        "INSERT INTO tracks (id, path, title, play_count, duration_ms, added_at) \
             VALUES (3, '/x/3.flac', 't3', 0, 500000, 0)",
        [],
    )
    .unwrap();

    let totals = headline_totals(&conn, None).unwrap();
    assert_eq!(
        totals,
        HeadlineTotals {
            total_ms: 800_000,
            total_plays: 5,
        }
    );
}

#[test]
fn headline_totals_are_zero_for_an_empty_library() {
    let conn = migrated_conn();
    assert_eq!(
        headline_totals(&conn, None).unwrap(),
        HeadlineTotals {
            total_ms: 0,
            total_plays: 0
        }
    );
}

#[test]
fn headline_totals_filtered_by_year() {
    let conn = migrated_conn();
    // Track 1: last_played in 2026, track 2: last_played in 2025.
    insert_track_full(&conn, 1, "A", "Alb", "", "", 3, 200_000, Some(T_2026_07_01));
    insert_track_full(
        &conn,
        2,
        "B",
        "Alb2",
        "",
        "",
        2,
        100_000,
        Some(T_2025_08_01),
    );
    // listen_events for ms totals
    record_listen_event(&conn, 1, T_2026_07_01, 190_000).unwrap();
    record_listen_event(&conn, 1, T_2026_07_05, 200_000).unwrap();
    record_listen_event(&conn, 2, T_2025_08_01, 95_000).unwrap();

    let totals_2026 = headline_totals(&conn, Some(2026)).unwrap();
    assert_eq!(totals_2026.total_plays, 3); // only track 1
    assert_eq!(totals_2026.total_ms, 390_000); // listen_events in 2026

    let totals_2025 = headline_totals(&conn, Some(2025)).unwrap();
    assert_eq!(totals_2025.total_plays, 2); // only track 2
    assert_eq!(totals_2025.total_ms, 95_000); // listen_events in 2025

    let totals_2024 = headline_totals(&conn, Some(2024)).unwrap();
    assert_eq!(totals_2024.total_plays, 0);
    assert_eq!(totals_2024.total_ms, 0);
}

fn seed_top_fixture(conn: &Connection) {
    // Alpha/A1: 10 + 5 = 15 plays; Beta/B1: 8 plays; Gamma/G1: 0 plays.
    conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, play_count, duration_ms, added_at) \
             VALUES (1, '/x/1.flac', 's1', 'Alpha', 'A1', 'Alpha', 10, 200000, 0)",
            [],
        )
        .unwrap();
    conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, play_count, duration_ms, added_at) \
             VALUES (2, '/x/2.flac', 's2', 'Alpha', 'A1', 'Alpha', 5, 180000, 0)",
            [],
        )
        .unwrap();
    conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, play_count, duration_ms, added_at) \
             VALUES (3, '/x/3.flac', 's3', 'Beta', 'B1', 'Beta', 8, 250000, 0)",
            [],
        )
        .unwrap();
    conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, play_count, duration_ms, added_at) \
             VALUES (4, '/x/4.flac', 's4', 'Gamma', 'G1', '', 0, 300000, 0)",
            [],
        )
        .unwrap();
}

#[test]
fn top_artists_rank_by_summed_plays_excluding_never_played() {
    let conn = migrated_conn();
    seed_top_fixture(&conn);
    let top = top_artists(&conn, 10, None).unwrap();
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].artist, "Alpha");
    assert_eq!(top[0].plays, 15);
    assert_eq!(top[0].total_ms, 10 * 200_000 + 5 * 180_000);
    assert!(!top[0].representative_track_path.is_empty());
    assert_eq!(top[1].artist, "Beta");
    assert_eq!(top[1].plays, 8);
    assert_eq!(top[1].total_ms, 8 * 250_000);
}

#[test]
fn top_artists_filtered_by_year() {
    let conn = migrated_conn();
    // Alpha played in 2026, Beta played in 2025.
    insert_track_full(
        &conn,
        1,
        "Alpha",
        "A1",
        "Alpha",
        "",
        10,
        200_000,
        Some(T_2026_07_01),
    );
    insert_track_full(
        &conn,
        2,
        "Beta",
        "B1",
        "Beta",
        "",
        8,
        250_000,
        Some(T_2025_08_01),
    );

    let top_2026 = top_artists(&conn, 10, Some(2026)).unwrap();
    assert_eq!(top_2026.len(), 1);
    assert_eq!(top_2026[0].artist, "Alpha");

    let top_2025 = top_artists(&conn, 10, Some(2025)).unwrap();
    assert_eq!(top_2025.len(), 1);
    assert_eq!(top_2025[0].artist, "Beta");
}

#[test]
fn top_albums_rank_by_summed_plays_with_effective_artist() {
    let conn = migrated_conn();
    seed_top_fixture(&conn);
    let top = top_albums(&conn, 10, None).unwrap();
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].album, "A1");
    assert_eq!(top[0].album_artist, "Alpha");
    assert_eq!(top[0].plays, 15);
    assert_eq!(top[0].total_ms, 10 * 200_000 + 5 * 180_000);
    assert!(!top[0].track_path.is_empty());
    assert_eq!(top[1].album, "B1");
    assert_eq!(top[1].album_artist, "Beta");
    assert_eq!(top[1].plays, 8);
}

#[test]
fn top_albums_filtered_by_year() {
    let conn = migrated_conn();
    insert_track_full(
        &conn,
        1,
        "Alpha",
        "A1",
        "Alpha",
        "",
        10,
        200_000,
        Some(T_2026_07_01),
    );
    insert_track_full(
        &conn,
        2,
        "Beta",
        "B1",
        "Beta",
        "",
        8,
        250_000,
        Some(T_2025_08_01),
    );

    let top_2026 = top_albums(&conn, 10, Some(2026)).unwrap();
    assert_eq!(top_2026.len(), 1);
    assert_eq!(top_2026[0].album, "A1");

    let top_2025 = top_albums(&conn, 10, Some(2025)).unwrap();
    assert_eq!(top_2025.len(), 1);
    assert_eq!(top_2025[0].album, "B1");
}

#[test]
fn top_tracks_rank_by_play_count_and_respect_limit() {
    let conn = migrated_conn();
    seed_top_fixture(&conn);
    let top = top_tracks(&conn, 2, None).unwrap();
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].title, "s1");
    assert_eq!(top[0].play_count, 10);
    assert_eq!(top[0].total_ms, 10 * 200_000);
    assert_eq!(top[0].track_path, "/x/1.flac");
    assert_eq!(top[1].title, "s3");
    assert_eq!(top[1].play_count, 8);
    assert_eq!(top[1].total_ms, 8 * 250_000);
}

#[test]
fn top_tracks_filtered_by_year() {
    let conn = migrated_conn();
    insert_track_full(
        &conn,
        1,
        "Alpha",
        "A1",
        "",
        "",
        10,
        200_000,
        Some(T_2026_07_01),
    );
    insert_track_full(
        &conn,
        2,
        "Beta",
        "B1",
        "",
        "",
        8,
        250_000,
        Some(T_2025_08_01),
    );

    let top_2026 = top_tracks(&conn, 10, Some(2026)).unwrap();
    assert_eq!(top_2026.len(), 1);
    assert_eq!(top_2026[0].title, "t1");

    let top_2025 = top_tracks(&conn, 10, Some(2025)).unwrap();
    assert_eq!(top_2025.len(), 1);
    assert_eq!(top_2025[0].title, "t2");
}

#[test]
fn top_genres_rank_by_summed_plays() {
    let conn = migrated_conn();
    insert_track_full(&conn, 1, "A", "Alb", "", "Rock", 10, 200_000, None);
    insert_track_full(&conn, 2, "B", "Alb", "", "Rock", 5, 180_000, None);
    insert_track_full(&conn, 3, "C", "Alb", "", "Jazz", 8, 250_000, None);
    insert_track_full(&conn, 4, "D", "Alb", "", "", 3, 100_000, None); // empty genre excluded

    let top = top_genres(&conn, 10, None).unwrap();
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].genre, "Rock");
    assert_eq!(top[0].plays, 15);
    assert_eq!(top[0].total_ms, 10 * 200_000 + 5 * 180_000);
    assert_eq!(top[1].genre, "Jazz");
    assert_eq!(top[1].plays, 8);
}

#[test]
fn top_genres_filtered_by_year() {
    let conn = migrated_conn();
    insert_track_full(
        &conn,
        1,
        "A",
        "Alb",
        "",
        "Rock",
        10,
        200_000,
        Some(T_2026_07_01),
    );
    insert_track_full(
        &conn,
        2,
        "B",
        "Alb",
        "",
        "Jazz",
        5,
        180_000,
        Some(T_2025_08_01),
    );

    let top_2026 = top_genres(&conn, 10, Some(2026)).unwrap();
    assert_eq!(top_2026.len(), 1);
    assert_eq!(top_2026[0].genre, "Rock");

    let top_2025 = top_genres(&conn, 10, Some(2025)).unwrap();
    assert_eq!(top_2025.len(), 1);
    assert_eq!(top_2025[0].genre, "Jazz");
}

#[test]
fn listening_by_hour_returns_active_hours() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "A", "Alb");
    // T_2026_07_01 = 2026-07-01 00:00:00 UTC => hour 0
    record_listen_event(&conn, 1, T_2026_07_01, 100_000).unwrap();
    // T_2026_07_01 + 3600*10 => hour 10
    record_listen_event(&conn, 1, T_2026_07_01 + 3600 * 10, 200_000).unwrap();
    record_listen_event(&conn, 1, T_2026_07_01 + 3600 * 10 + 60, 150_000).unwrap();

    let hours = listening_by_hour(&conn, None).unwrap();
    assert_eq!(hours.len(), 2);
    assert_eq!(hours[0].hour, 0);
    assert_eq!(hours[0].listens, 1);
    assert_eq!(hours[0].total_ms, 100_000);
    assert_eq!(hours[1].hour, 10);
    assert_eq!(hours[1].listens, 2);
    assert_eq!(hours[1].total_ms, 350_000);
}

#[test]
fn listening_by_hour_filtered_by_year() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "A", "Alb");
    // 2026 event at hour 0
    record_listen_event(&conn, 1, T_2026_07_01, 100_000).unwrap();
    // 2025 event at hour 12
    record_listen_event(&conn, 1, T_2025_08_01 + 3600 * 12, 200_000).unwrap();

    let hours_2026 = listening_by_hour(&conn, Some(2026)).unwrap();
    assert_eq!(hours_2026.len(), 1);
    assert_eq!(hours_2026[0].hour, 0);

    let hours_2025 = listening_by_hour(&conn, Some(2025)).unwrap();
    assert_eq!(hours_2025.len(), 1);
    assert_eq!(hours_2025[0].hour, 12);
}

#[test]
fn distinct_artists_played_counts_unique_artists() {
    let conn = migrated_conn();
    seed_top_fixture(&conn);
    let count = distinct_artists_played(&conn, None).unwrap();
    assert_eq!(count, 2); // Alpha and Beta (Gamma has 0 plays)
}

#[test]
fn distinct_artists_played_filtered_by_year() {
    let conn = migrated_conn();
    insert_track_full(
        &conn,
        1,
        "Alpha",
        "A1",
        "",
        "",
        10,
        200_000,
        Some(T_2026_07_01),
    );
    insert_track_full(
        &conn,
        2,
        "Beta",
        "B1",
        "",
        "",
        8,
        250_000,
        Some(T_2025_08_01),
    );

    assert_eq!(distinct_artists_played(&conn, Some(2026)).unwrap(), 1);
    assert_eq!(distinct_artists_played(&conn, Some(2025)).unwrap(), 1);
    assert_eq!(distinct_artists_played(&conn, Some(2024)).unwrap(), 0);
}

#[test]
fn most_active_weekday_returns_busiest_day() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "A", "Alb");
    // T_2026_07_01 is a Wednesday (2026-07-01). Add 3 events on Wed, 1 on Thu.
    record_listen_event(&conn, 1, T_2026_07_01, 100_000).unwrap();
    record_listen_event(&conn, 1, T_2026_07_01 + 60, 100_000).unwrap();
    record_listen_event(&conn, 1, T_2026_07_01 + 120, 100_000).unwrap();
    // Thursday = T_2026_07_01 + 86400
    record_listen_event(&conn, 1, T_2026_07_01 + 86400, 100_000).unwrap();

    let result = most_active_weekday(&conn, None).unwrap();
    assert!(result.is_some());
    let (day, count) = result.unwrap();
    assert_eq!(day, "Wednesday");
    assert_eq!(count, 3);
}

#[test]
fn most_active_weekday_returns_none_for_empty() {
    let conn = migrated_conn();
    let result = most_active_weekday(&conn, None).unwrap();
    assert!(result.is_none());
}

#[test]
fn most_active_weekday_filtered_by_year() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "A", "Alb");
    // 2026 events on Wednesday (T_2026_07_01)
    record_listen_event(&conn, 1, T_2026_07_01, 100_000).unwrap();
    record_listen_event(&conn, 1, T_2026_07_01 + 60, 100_000).unwrap();
    // 2025 event on Friday (T_2025_08_01 = 2025-08-01 = Friday)
    record_listen_event(&conn, 1, T_2025_08_01, 100_000).unwrap();

    let result_2026 = most_active_weekday(&conn, Some(2026)).unwrap();
    assert_eq!(result_2026.unwrap().0, "Wednesday");

    let result_2025 = most_active_weekday(&conn, Some(2025)).unwrap();
    assert_eq!(result_2025.unwrap().0, "Friday");

    let result_2024 = most_active_weekday(&conn, Some(2024)).unwrap();
    assert!(result_2024.is_none());
}

#[test]
fn available_years_returns_distinct_years_descending() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "A", "Alb");
    record_listen_event(&conn, 1, T_2026_07_01, 100_000).unwrap();
    record_listen_event(&conn, 1, T_2026_07_05, 200_000).unwrap();
    record_listen_event(&conn, 1, T_2025_08_01, 300_000).unwrap();
    record_listen_event(&conn, 1, T_2025_07_15, 400_000).unwrap();

    let years = available_years(&conn).unwrap();
    assert_eq!(years, vec![2026, 2025]);
}

#[test]
fn available_years_empty_for_no_events() {
    let conn = migrated_conn();
    let years = available_years(&conn).unwrap();
    assert!(years.is_empty());
}
