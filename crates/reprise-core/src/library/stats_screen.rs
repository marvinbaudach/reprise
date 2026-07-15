//! Pure data/logic layer behind the local "My Stats" screen (frontend UI
//! lands separately). Two data sources feed the screen:
//!
//! * `listen_events` (schema v7) — one row per completed play, used to build
//!   a month-by-month timeseries. Written via [`record_listen_event`] from the
//!   playback path once a play crosses the listen threshold (see
//!   `scrobbling::should_scrobble`, reused ungated so local stats count every
//!   qualifying play, not only scrobbled ones).
//! * `tracks.play_count` / `tracks.duration_ms` — the all-time running
//!   counters, used for the headline totals and the top-N lists.
//!
//! Every query is a pure SQL read with no `now()` baked in: the timeseries
//! takes its reference "now" as a parameter so it is deterministic under test.
//! Calendar months are bucketed in **UTC** (`played_at` is stored as unix
//! seconds), which keeps both the storage unit and the tests timezone-free.

use rusqlite::{params, Connection, OptionalExtension};

/// Number of trailing calendar months the stats timeseries covers, including
/// the reference month itself.
const TIMESERIES_MONTHS: i64 = 12;

/// One calendar-month bucket of the [`monthly_listen_timeseries`] result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthlyListens {
    /// `YYYY-MM` label of the bucket (UTC), e.g. `"2026-07"`.
    pub year_month: String,
    /// Sum of `ms_played` across the month's listen events (0 for empty
    /// months).
    pub total_ms: i64,
    /// Number of listen events recorded in the month (0 for empty months).
    pub listens: i64,
}

/// All-time headline aggregates derived from `tracks`, not from
/// `listen_events`: they reflect the running `play_count` counter so they stay
/// consistent with pre-v7 history that predates per-play event recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlineTotals {
    /// `Σ(play_count × duration_ms)` — total listening time in milliseconds.
    pub total_ms: i64,
    /// `Σ play_count` — total number of plays across the library.
    pub total_plays: i64,
}

/// A top-artists row: an artist and their summed play count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopArtist {
    pub artist: String,
    pub plays: i64,
}

/// A top-albums row. `album_artist` is the effective album artist (the
/// `album_artist` tag when present, otherwise the track `artist`), so albums
/// are grouped the way a listener expects rather than split across a blank
/// tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopAlbum {
    pub album: String,
    pub album_artist: String,
    pub plays: i64,
}

/// A top-tracks row: a single track and its all-time play count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopTrack {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub play_count: i64,
}

/// A top-genres row: a genre and its summed play count across all tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopGenre {
    pub genre: String,
    pub plays: i64,
}

/// One hour-of-day bucket (0-23) with its listen count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourlyListens {
    pub hour: u8,
    pub listens: i64,
}

/// Records one completed play into `listen_events`. `played_at` is unix
/// seconds; `ms_played` is how much of the track was actually heard (the
/// furthest position reached). The caller decides whether a play qualifies
/// (threshold predicate) — this function only writes.
pub fn record_listen_event(
    conn: &Connection,
    track_id: i64,
    played_at: i64,
    ms_played: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (?1, ?2, ?3)",
        params![track_id, played_at, ms_played],
    )?;
    Ok(())
}

/// Builds a 12-bucket timeseries of listening activity for the calendar
/// months ending at `now_unix`'s month (inclusive), oldest bucket first.
/// Months with no events are returned as explicit zero buckets so the caller
/// always gets exactly [`TIMESERIES_MONTHS`] contiguous entries. Events
/// outside the window are ignored.
pub fn monthly_listen_timeseries(
    conn: &Connection,
    now_unix: i64,
) -> Result<Vec<MonthlyListens>, rusqlite::Error> {
    // A recursive sequence 0..=11 generates the twelve month buckets relative
    // to `now_unix`; each bucket's `YYYY-MM` label is computed in SQLite so
    // calendar arithmetic (month lengths, year rollover) is handled by the
    // engine. A LEFT JOIN keeps empty months as zero rows.
    let mut statement = conn.prepare(
        "WITH RECURSIVE seq(idx) AS ( \
             SELECT 0 UNION ALL SELECT idx + 1 FROM seq WHERE idx < ?2 - 1 \
         ) \
         SELECT \
             strftime('%Y-%m', ?1, 'unixepoch', 'start of month', \
                      '-' || (?2 - 1 - idx) || ' months') AS ym, \
             COALESCE(SUM(le.ms_played), 0) AS total_ms, \
             COUNT(le.id) AS listens \
         FROM seq \
         LEFT JOIN listen_events le \
             ON strftime('%Y-%m', le.played_at, 'unixepoch') = \
                strftime('%Y-%m', ?1, 'unixepoch', 'start of month', \
                         '-' || (?2 - 1 - idx) || ' months') \
         GROUP BY idx \
         ORDER BY idx",
    )?;
    let rows = statement.query_map(params![now_unix, TIMESERIES_MONTHS], |row| {
        Ok(MonthlyListens {
            year_month: row.get(0)?,
            total_ms: row.get(1)?,
            listens: row.get(2)?,
        })
    })?;
    rows.collect()
}

/// Computes the all-time headline totals from `tracks`.
pub fn headline_totals(conn: &Connection) -> Result<HeadlineTotals, rusqlite::Error> {
    conn.query_row(
        "SELECT \
             COALESCE(SUM(play_count * duration_ms), 0), \
             COALESCE(SUM(play_count), 0) \
         FROM tracks",
        [],
        |row| {
            Ok(HeadlineTotals {
                total_ms: row.get(0)?,
                total_plays: row.get(1)?,
            })
        },
    )
}

/// Top artists by summed all-time play count, most-played first. Artists with
/// zero plays or a blank name are excluded. Ties break alphabetically for a
/// stable order.
pub fn top_artists(conn: &Connection, limit: usize) -> Result<Vec<TopArtist>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT artist, SUM(play_count) AS plays \
         FROM tracks \
         WHERE play_count > 0 AND artist <> '' \
         GROUP BY artist \
         ORDER BY plays DESC, artist ASC \
         LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok(TopArtist {
            artist: row.get(0)?,
            plays: row.get(1)?,
        })
    })?;
    rows.collect()
}

/// Top albums by summed all-time play count, most-played first. Rows are
/// grouped by album title and effective album artist (see [`TopAlbum`]).
/// Albums with zero plays or a blank title are excluded; ties break
/// alphabetically.
pub fn top_albums(conn: &Connection, limit: usize) -> Result<Vec<TopAlbum>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT album, \
                CASE WHEN album_artist <> '' THEN album_artist ELSE artist END AS eff_artist, \
                SUM(play_count) AS plays \
         FROM tracks \
         WHERE play_count > 0 AND album <> '' \
         GROUP BY album, eff_artist \
         ORDER BY plays DESC, album ASC \
         LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok(TopAlbum {
            album: row.get(0)?,
            album_artist: row.get(1)?,
            plays: row.get(2)?,
        })
    })?;
    rows.collect()
}

/// Top individual tracks by all-time play count, most-played first. Never-
/// played tracks are excluded; ties break by title for a stable order.
pub fn top_tracks(conn: &Connection, limit: usize) -> Result<Vec<TopTrack>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, title, artist, album, play_count \
         FROM tracks \
         WHERE play_count > 0 \
         ORDER BY play_count DESC, title ASC \
         LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok(TopTrack {
            track_id: row.get(0)?,
            title: row.get(1)?,
            artist: row.get(2)?,
            album: row.get(3)?,
            play_count: row.get(4)?,
        })
    })?;
    rows.collect()
}

/// Top genres by summed all-time play count across all tracks, most-played
/// first. Tracks with a blank genre or zero plays are excluded. Ties break
/// alphabetically for a stable order.
pub fn top_genres(conn: &Connection, limit: usize) -> Result<Vec<TopGenre>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT genre, SUM(play_count) AS plays \
         FROM tracks \
         WHERE play_count > 0 AND genre <> '' \
         GROUP BY genre \
         ORDER BY plays DESC, genre ASC \
         LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok(TopGenre {
            genre: row.get(0)?,
            plays: row.get(1)?,
        })
    })?;
    rows.collect()
}

/// Listening activity grouped by hour of day (0-23, UTC). Returns up to 24
/// buckets; hours with no events are omitted (the caller should fill in
/// zeros for a full 0-23 axis). Uses `listen_events.played_at`.
pub fn listening_by_hour(conn: &Connection) -> Result<Vec<HourlyListens>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT CAST(strftime('%H', played_at, 'unixepoch') AS INTEGER) AS hour, \
                COUNT(*) AS listens \
         FROM listen_events \
         GROUP BY hour \
         ORDER BY hour",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(HourlyListens {
            hour: row.get::<_, i64>(0)? as u8,
            listens: row.get(1)?,
        })
    })?;
    rows.collect()
}

/// Count of distinct artists that have at least one play.
pub fn distinct_artists_played(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(DISTINCT artist) FROM tracks \
         WHERE play_count > 0 AND artist <> ''",
        [],
        |row| row.get(0),
    )
}

/// The weekday with the most listen events (from `listen_events`). Returns
/// `None` when the table is empty. The weekday is derived via
/// `strftime('%w')` (0=Sunday .. 6=Saturday) and mapped to an English name.
pub fn most_active_weekday(conn: &Connection) -> Result<Option<String>, rusqlite::Error> {
    let result: Option<i64> = conn
        .query_row(
            "SELECT CAST(strftime('%w', played_at, 'unixepoch') AS INTEGER) AS dow \
             FROM listen_events \
             GROUP BY dow \
             ORDER BY COUNT(*) DESC \
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(result.map(weekday_name))
}

/// Maps a SQLite `strftime('%w')` result (0-6) to an English day name.
fn weekday_name(dow: i64) -> String {
    match dow {
        0 => "Sunday",
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        6 => "Saturday",
        _ => "Unknown",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
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

        let totals = headline_totals(&conn).unwrap();
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
            headline_totals(&conn).unwrap(),
            HeadlineTotals {
                total_ms: 0,
                total_plays: 0
            }
        );
    }

    fn seed_top_fixture(conn: &Connection) {
        // Alpha/A1: 10 + 5 = 15 plays; Beta/B1: 8 plays; Gamma/G1: 0 plays.
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, play_count, added_at) \
             VALUES (1, '/x/1.flac', 's1', 'Alpha', 'A1', 'Alpha', 10, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, play_count, added_at) \
             VALUES (2, '/x/2.flac', 's2', 'Alpha', 'A1', 'Alpha', 5, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, play_count, added_at) \
             VALUES (3, '/x/3.flac', 's3', 'Beta', 'B1', 'Beta', 8, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, album_artist, play_count, added_at) \
             VALUES (4, '/x/4.flac', 's4', 'Gamma', 'G1', '', 0, 0)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn top_artists_rank_by_summed_plays_excluding_never_played() {
        let conn = migrated_conn();
        seed_top_fixture(&conn);
        let top = top_artists(&conn, 10).unwrap();
        assert_eq!(
            top,
            vec![
                TopArtist {
                    artist: "Alpha".to_string(),
                    plays: 15
                },
                TopArtist {
                    artist: "Beta".to_string(),
                    plays: 8
                },
            ]
        );
    }

    #[test]
    fn top_albums_rank_by_summed_plays_with_effective_artist() {
        let conn = migrated_conn();
        seed_top_fixture(&conn);
        let top = top_albums(&conn, 10).unwrap();
        assert_eq!(
            top,
            vec![
                TopAlbum {
                    album: "A1".to_string(),
                    album_artist: "Alpha".to_string(),
                    plays: 15
                },
                TopAlbum {
                    album: "B1".to_string(),
                    album_artist: "Beta".to_string(),
                    plays: 8
                },
            ]
        );
    }

    #[test]
    fn top_tracks_rank_by_play_count_and_respect_limit() {
        let conn = migrated_conn();
        seed_top_fixture(&conn);
        let top = top_tracks(&conn, 2).unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].title, "s1");
        assert_eq!(top[0].play_count, 10);
        assert_eq!(top[1].title, "s3");
        assert_eq!(top[1].play_count, 8);
    }

    // --- top_genres ---

    fn seed_genre_fixture(conn: &Connection) {
        conn.execute(
            "INSERT INTO tracks (id, path, title, genre, play_count, added_at) \
             VALUES (1, '/x/1.flac', 't1', 'Rock', 10, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, genre, play_count, added_at) \
             VALUES (2, '/x/2.flac', 't2', 'Rock', 5, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, genre, play_count, added_at) \
             VALUES (3, '/x/3.flac', 't3', 'Jazz', 8, 0)",
            [],
        )
        .unwrap();
        // Blank genre — excluded.
        conn.execute(
            "INSERT INTO tracks (id, path, title, genre, play_count, added_at) \
             VALUES (4, '/x/4.flac', 't4', '', 20, 0)",
            [],
        )
        .unwrap();
        // Zero plays — excluded.
        conn.execute(
            "INSERT INTO tracks (id, path, title, genre, play_count, added_at) \
             VALUES (5, '/x/5.flac', 't5', 'Pop', 0, 0)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn top_genres_rank_by_summed_plays_excluding_blank_and_zero() {
        let conn = migrated_conn();
        seed_genre_fixture(&conn);
        let top = top_genres(&conn, 10).unwrap();
        assert_eq!(
            top,
            vec![
                TopGenre {
                    genre: "Rock".to_string(),
                    plays: 15
                },
                TopGenre {
                    genre: "Jazz".to_string(),
                    plays: 8
                },
            ]
        );
    }

    #[test]
    fn top_genres_respects_limit() {
        let conn = migrated_conn();
        seed_genre_fixture(&conn);
        let top = top_genres(&conn, 1).unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].genre, "Rock");
    }

    #[test]
    fn top_genres_empty_library() {
        let conn = migrated_conn();
        let top = top_genres(&conn, 10).unwrap();
        assert!(top.is_empty());
    }

    // --- listening_by_hour ---

    #[test]
    fn listening_by_hour_groups_by_utc_hour() {
        let conn = migrated_conn();
        insert_track(&conn, 1, "A", "Alb");
        // 2026-07-01 00:00:00 UTC -> hour 0
        record_listen_event(&conn, 1, 1_782_864_000, 100_000).unwrap();
        // 2026-07-01 00:30:00 UTC -> hour 0
        record_listen_event(&conn, 1, 1_782_865_800, 100_000).unwrap();
        // 2026-07-01 13:00:00 UTC -> hour 13
        record_listen_event(&conn, 1, 1_782_910_800, 100_000).unwrap();

        let hourly = listening_by_hour(&conn).unwrap();
        assert_eq!(hourly.len(), 2);
        assert_eq!(
            hourly[0],
            HourlyListens {
                hour: 0,
                listens: 2
            }
        );
        assert_eq!(
            hourly[1],
            HourlyListens {
                hour: 13,
                listens: 1
            }
        );
    }

    #[test]
    fn listening_by_hour_empty_library() {
        let conn = migrated_conn();
        let hourly = listening_by_hour(&conn).unwrap();
        assert!(hourly.is_empty());
    }

    // --- distinct_artists_played ---

    #[test]
    fn distinct_artists_played_counts_unique_artists_with_plays() {
        let conn = migrated_conn();
        seed_top_fixture(&conn);
        // Alpha and Beta have plays, Gamma has 0.
        let count = distinct_artists_played(&conn).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn distinct_artists_played_empty_library() {
        let conn = migrated_conn();
        let count = distinct_artists_played(&conn).unwrap();
        assert_eq!(count, 0);
    }

    // --- most_active_weekday ---

    #[test]
    fn most_active_weekday_returns_day_with_most_events() {
        let conn = migrated_conn();
        insert_track(&conn, 1, "A", "Alb");
        // 2026-07-01 is a Wednesday (dow=3). Two events on Wednesday.
        record_listen_event(&conn, 1, 1_782_864_000, 100_000).unwrap();
        record_listen_event(&conn, 1, 1_782_865_800, 100_000).unwrap();
        // 2026-07-02 is a Thursday (dow=4). One event.
        record_listen_event(&conn, 1, 1_782_950_400, 100_000).unwrap();

        let day = most_active_weekday(&conn).unwrap();
        assert_eq!(day, Some("Wednesday".to_string()));
    }

    #[test]
    fn most_active_weekday_returns_none_for_empty_table() {
        let conn = migrated_conn();
        let day = most_active_weekday(&conn).unwrap();
        assert_eq!(day, None);
    }
}
