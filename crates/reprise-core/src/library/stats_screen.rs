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

use rusqlite::{params, Connection};

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
    /// `SUM(play_count * duration_ms)` for all tracks by this artist.
    pub total_ms: i64,
    /// Path to any one track by this artist (for cover art loading).
    pub representative_track_path: String,
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
    /// `SUM(play_count * duration_ms)` for all tracks on this album.
    pub total_ms: i64,
    /// Path to any one track on this album (for cover art loading).
    pub track_path: String,
}

/// A top-tracks row: a single track and its all-time play count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopTrack {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub play_count: i64,
    /// `play_count * duration_ms` for this track.
    pub total_ms: i64,
    /// Path to this track's file (for cover art loading).
    pub track_path: String,
}

/// A top-genres row: a genre and its summed play count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopGenre {
    pub genre: String,
    pub plays: i64,
    pub total_ms: i64,
}

/// A single hour bucket (0..23) with listening activity counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourlyListens {
    pub hour: i32,
    pub listens: i64,
    pub total_ms: i64,
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

/// Computes the headline totals. When `year` is `None`, sums all-time from
/// `tracks`. When `year` is `Some`, restricts play_count sums to tracks whose
/// `last_played_at` falls in that year, and listening-time totals to
/// listen_events in that year.
pub fn headline_totals(
    conn: &Connection,
    year: Option<i32>,
) -> Result<HeadlineTotals, rusqlite::Error> {
    match year {
        None => conn.query_row(
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
        ),
        Some(y) => {
            let year_str = y.to_string();
            // For year-filtered totals: total_ms from listen_events (accurate
            // per-play data), total_plays from tracks with last_played_at in
            // that year (best approximation without per-event play counts).
            let total_ms: i64 = conn.query_row(
                "SELECT COALESCE(SUM(le.ms_played), 0) \
                 FROM listen_events le \
                 WHERE strftime('%Y', le.played_at, 'unixepoch') = ?1",
                params![year_str],
                |row| row.get(0),
            )?;
            let total_plays: i64 = conn.query_row(
                "SELECT COALESCE(SUM(play_count), 0) \
                 FROM tracks \
                 WHERE strftime('%Y', last_played_at, 'unixepoch') = ?1",
                params![year_str],
                |row| row.get(0),
            )?;
            Ok(HeadlineTotals {
                total_ms,
                total_plays,
            })
        }
    }
}

/// Top artists by summed play count, most-played first. Artists with zero
/// plays or a blank name are excluded. Ties break alphabetically for a stable
/// order. When `year` is `Some`, only tracks whose `last_played_at` falls in
/// that year are counted.
pub fn top_artists(
    conn: &Connection,
    limit: usize,
    year: Option<i32>,
) -> Result<Vec<TopArtist>, rusqlite::Error> {
    let (sql, year_str) = match year {
        None => (
            "SELECT artist, SUM(play_count) AS plays, \
                    SUM(play_count * duration_ms) AS total_ms, \
                    MIN(path) AS track_path \
             FROM tracks \
             WHERE play_count > 0 AND artist <> '' \
             GROUP BY artist \
             ORDER BY plays DESC, artist ASC \
             LIMIT ?1"
                .to_string(),
            String::new(),
        ),
        Some(y) => (
            "SELECT artist, SUM(play_count) AS plays, \
                    SUM(play_count * duration_ms) AS total_ms, \
                    MIN(path) AS track_path \
             FROM tracks \
             WHERE play_count > 0 AND artist <> '' \
               AND strftime('%Y', last_played_at, 'unixepoch') = ?2 \
             GROUP BY artist \
             ORDER BY plays DESC, artist ASC \
             LIMIT ?1"
                .to_string(),
            y.to_string(),
        ),
    };
    let mut statement = conn.prepare(&sql)?;
    let map_row = |row: &rusqlite::Row| {
        Ok(TopArtist {
            artist: row.get(0)?,
            plays: row.get(1)?,
            total_ms: row.get(2)?,
            representative_track_path: row.get(3)?,
        })
    };
    if year.is_some() {
        statement
            .query_map(params![limit as i64, year_str], map_row)?
            .collect()
    } else {
        statement
            .query_map(params![limit as i64], map_row)?
            .collect()
    }
}

/// Top albums by summed play count, most-played first. Rows are grouped by
/// album title and effective album artist (see [`TopAlbum`]). Albums with zero
/// plays or a blank title are excluded; ties break alphabetically. When `year`
/// is `Some`, only tracks whose `last_played_at` falls in that year are
/// counted.
pub fn top_albums(
    conn: &Connection,
    limit: usize,
    year: Option<i32>,
) -> Result<Vec<TopAlbum>, rusqlite::Error> {
    let (sql, year_str) = match year {
        None => (
            "SELECT album, \
                    CASE WHEN album_artist <> '' THEN album_artist ELSE artist END AS eff_artist, \
                    SUM(play_count) AS plays, \
                    SUM(play_count * duration_ms) AS total_ms, \
                    MIN(path) AS track_path \
             FROM tracks \
             WHERE play_count > 0 AND album <> '' \
             GROUP BY album, eff_artist \
             ORDER BY plays DESC, album ASC \
             LIMIT ?1"
                .to_string(),
            String::new(),
        ),
        Some(y) => (
            "SELECT album, \
                    CASE WHEN album_artist <> '' THEN album_artist ELSE artist END AS eff_artist, \
                    SUM(play_count) AS plays, \
                    SUM(play_count * duration_ms) AS total_ms, \
                    MIN(path) AS track_path \
             FROM tracks \
             WHERE play_count > 0 AND album <> '' \
               AND strftime('%Y', last_played_at, 'unixepoch') = ?2 \
             GROUP BY album, eff_artist \
             ORDER BY plays DESC, album ASC \
             LIMIT ?1"
                .to_string(),
            y.to_string(),
        ),
    };
    let mut statement = conn.prepare(&sql)?;
    let map_row = |row: &rusqlite::Row| {
        Ok(TopAlbum {
            album: row.get(0)?,
            album_artist: row.get(1)?,
            plays: row.get(2)?,
            total_ms: row.get(3)?,
            track_path: row.get(4)?,
        })
    };
    if year.is_some() {
        statement
            .query_map(params![limit as i64, year_str], map_row)?
            .collect()
    } else {
        statement
            .query_map(params![limit as i64], map_row)?
            .collect()
    }
}

/// Top individual tracks by play count, most-played first. Never-played
/// tracks are excluded; ties break by title for a stable order. When `year`
/// is `Some`, only tracks whose `last_played_at` falls in that year are
/// counted.
pub fn top_tracks(
    conn: &Connection,
    limit: usize,
    year: Option<i32>,
) -> Result<Vec<TopTrack>, rusqlite::Error> {
    let (sql, year_str) = match year {
        None => (
            "SELECT id, title, artist, album, play_count, \
                    play_count * duration_ms AS total_ms, \
                    path \
             FROM tracks \
             WHERE play_count > 0 \
             ORDER BY play_count DESC, title ASC \
             LIMIT ?1"
                .to_string(),
            String::new(),
        ),
        Some(y) => (
            "SELECT id, title, artist, album, play_count, \
                    play_count * duration_ms AS total_ms, \
                    path \
             FROM tracks \
             WHERE play_count > 0 \
               AND strftime('%Y', last_played_at, 'unixepoch') = ?2 \
             ORDER BY play_count DESC, title ASC \
             LIMIT ?1"
                .to_string(),
            y.to_string(),
        ),
    };
    let mut statement = conn.prepare(&sql)?;
    let map_row = |row: &rusqlite::Row| {
        Ok(TopTrack {
            track_id: row.get(0)?,
            title: row.get(1)?,
            artist: row.get(2)?,
            album: row.get(3)?,
            play_count: row.get(4)?,
            total_ms: row.get(5)?,
            track_path: row.get(6)?,
        })
    };
    if year.is_some() {
        statement
            .query_map(params![limit as i64, year_str], map_row)?
            .collect()
    } else {
        statement
            .query_map(params![limit as i64], map_row)?
            .collect()
    }
}

/// Top genres by summed play count, most-played first. Tracks with an empty
/// genre are excluded. When `year` is `Some`, only tracks whose
/// `last_played_at` falls in that year are counted.
pub fn top_genres(
    conn: &Connection,
    limit: usize,
    year: Option<i32>,
) -> Result<Vec<TopGenre>, rusqlite::Error> {
    let (sql, year_str) = match year {
        None => (
            "SELECT genre, SUM(play_count) AS plays, \
                    SUM(play_count * duration_ms) AS total_ms \
             FROM tracks \
             WHERE play_count > 0 AND genre <> '' \
             GROUP BY genre \
             ORDER BY plays DESC, genre ASC \
             LIMIT ?1"
                .to_string(),
            String::new(),
        ),
        Some(y) => (
            "SELECT genre, SUM(play_count) AS plays, \
                    SUM(play_count * duration_ms) AS total_ms \
             FROM tracks \
             WHERE play_count > 0 AND genre <> '' \
               AND strftime('%Y', last_played_at, 'unixepoch') = ?2 \
             GROUP BY genre \
             ORDER BY plays DESC, genre ASC \
             LIMIT ?1"
                .to_string(),
            y.to_string(),
        ),
    };
    let mut statement = conn.prepare(&sql)?;
    let map_row = |row: &rusqlite::Row| {
        Ok(TopGenre {
            genre: row.get(0)?,
            plays: row.get(1)?,
            total_ms: row.get(2)?,
        })
    };
    if year.is_some() {
        statement
            .query_map(params![limit as i64, year_str], map_row)?
            .collect()
    } else {
        statement
            .query_map(params![limit as i64], map_row)?
            .collect()
    }
}

/// Listening activity by hour of day (0..23), based on `listen_events`.
/// Returns only hours that have at least one event, ordered by hour. When
/// `year` is `Some`, only events in that year are counted.
pub fn listening_by_hour(
    conn: &Connection,
    year: Option<i32>,
) -> Result<Vec<HourlyListens>, rusqlite::Error> {
    let (sql, year_str) = match year {
        None => (
            "SELECT CAST(strftime('%H', played_at, 'unixepoch') AS INTEGER) AS hour, \
                    COUNT(*) AS listens, \
                    COALESCE(SUM(ms_played), 0) AS total_ms \
             FROM listen_events \
             GROUP BY hour \
             ORDER BY hour"
                .to_string(),
            String::new(),
        ),
        Some(y) => (
            "SELECT CAST(strftime('%H', played_at, 'unixepoch') AS INTEGER) AS hour, \
                    COUNT(*) AS listens, \
                    COALESCE(SUM(ms_played), 0) AS total_ms \
             FROM listen_events \
             WHERE strftime('%Y', played_at, 'unixepoch') = ?1 \
             GROUP BY hour \
             ORDER BY hour"
                .to_string(),
            y.to_string(),
        ),
    };
    let mut statement = conn.prepare(&sql)?;
    let map_row = |row: &rusqlite::Row| {
        Ok(HourlyListens {
            hour: row.get(0)?,
            listens: row.get(1)?,
            total_ms: row.get(2)?,
        })
    };
    if year.is_some() {
        statement
            .query_map(params![year_str], map_row)?
            .collect()
    } else {
        statement.query_map([], map_row)?.collect()
    }
}

/// Count of distinct artists with at least one play. When `year` is `Some`,
/// only tracks whose `last_played_at` falls in that year are counted.
pub fn distinct_artists_played(
    conn: &Connection,
    year: Option<i32>,
) -> Result<i64, rusqlite::Error> {
    match year {
        None => conn.query_row(
            "SELECT COUNT(DISTINCT artist) FROM tracks \
             WHERE play_count > 0 AND artist <> ''",
            [],
            |row| row.get(0),
        ),
        Some(y) => {
            let year_str = y.to_string();
            conn.query_row(
                "SELECT COUNT(DISTINCT artist) FROM tracks \
                 WHERE play_count > 0 AND artist <> '' \
                   AND strftime('%Y', last_played_at, 'unixepoch') = ?1",
                params![year_str],
                |row| row.get(0),
            )
        }
    }
}

/// The most active weekday by listen event count. Returns the weekday name
/// (e.g. `"Monday"`) and its event count, or `None` if there are no events.
/// When `year` is `Some`, only events in that year are counted.
pub fn most_active_weekday(
    conn: &Connection,
    year: Option<i32>,
) -> Result<Option<(String, i64)>, rusqlite::Error> {
    // SQLite strftime('%w') returns 0=Sunday .. 6=Saturday.
    let weekday_names = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];

    let (sql, year_str) = match year {
        None => (
            "SELECT CAST(strftime('%w', played_at, 'unixepoch') AS INTEGER) AS dow, \
                    COUNT(*) AS listens \
             FROM listen_events \
             GROUP BY dow \
             ORDER BY listens DESC \
             LIMIT 1"
                .to_string(),
            String::new(),
        ),
        Some(y) => (
            "SELECT CAST(strftime('%w', played_at, 'unixepoch') AS INTEGER) AS dow, \
                    COUNT(*) AS listens \
             FROM listen_events \
             WHERE strftime('%Y', played_at, 'unixepoch') = ?1 \
             GROUP BY dow \
             ORDER BY listens DESC \
             LIMIT 1"
                .to_string(),
            y.to_string(),
        ),
    };
    let mut statement = conn.prepare(&sql)?;
    let map_row = |row: &rusqlite::Row| {
        let dow: usize = row.get(0)?;
        let listens: i64 = row.get(1)?;
        Ok((dow, listens))
    };
    let mut result_rows = if year.is_some() {
        statement.query_map(params![year_str], map_row)?
    } else {
        statement.query_map([], map_row)?
    };
    match result_rows.next() {
        Some(Ok((dow, listens))) => {
            let name = weekday_names.get(dow).unwrap_or(&"Unknown");
            Ok(Some((name.to_string(), listens)))
        }
        Some(Err(e)) => Err(e),
        None => Ok(None),
    }
}

/// Returns distinct years from `listen_events`, sorted descending. The UI
/// uses this to populate the year selector dropdown.
pub fn available_years(conn: &Connection) -> Result<Vec<i32>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT DISTINCT CAST(strftime('%Y', played_at, 'unixepoch') AS INTEGER) AS y \
         FROM listen_events \
         ORDER BY y DESC",
    )?;
    let rows = statement.query_map([], |row| row.get(0))?;
    rows.collect()
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
        insert_track_full(&conn, 2, "B", "Alb2", "", "", 2, 100_000, Some(T_2025_08_01));
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
        insert_track_full(&conn, 1, "Alpha", "A1", "Alpha", "", 10, 200_000, Some(T_2026_07_01));
        insert_track_full(&conn, 2, "Beta", "B1", "Beta", "", 8, 250_000, Some(T_2025_08_01));

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
        insert_track_full(&conn, 1, "Alpha", "A1", "Alpha", "", 10, 200_000, Some(T_2026_07_01));
        insert_track_full(&conn, 2, "Beta", "B1", "Beta", "", 8, 250_000, Some(T_2025_08_01));

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
        insert_track_full(&conn, 1, "Alpha", "A1", "", "", 10, 200_000, Some(T_2026_07_01));
        insert_track_full(&conn, 2, "Beta", "B1", "", "", 8, 250_000, Some(T_2025_08_01));

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
        insert_track_full(&conn, 1, "A", "Alb", "", "Rock", 10, 200_000, Some(T_2026_07_01));
        insert_track_full(&conn, 2, "B", "Alb", "", "Jazz", 5, 180_000, Some(T_2025_08_01));

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
        insert_track_full(&conn, 1, "Alpha", "A1", "", "", 10, 200_000, Some(T_2026_07_01));
        insert_track_full(&conn, 2, "Beta", "B1", "", "", 8, 250_000, Some(T_2025_08_01));

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
}
