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
/// always gets exactly `TIMESERIES_MONTHS` contiguous entries. Events
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
        statement.query_map(params![year_str], map_row)?.collect()
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
        let dow: i64 = row.get(0)?;
        let listens: i64 = row.get(1)?;
        Ok((usize::try_from(dow).unwrap_or_default(), listens))
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
#[path = "stats_screen_tests.rs"]
mod tests;
