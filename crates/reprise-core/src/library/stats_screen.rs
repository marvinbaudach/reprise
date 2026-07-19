//! Local, read-only primitives for the My Stats snapshot.
//!
//! Every local aggregate is projected from `listen_events` joined to tracks.
//! The running library counter remains available elsewhere, but never feeds
//! this module's stats-screen queries.

use std::collections::HashMap;

use chrono::{Datelike, TimeZone, Utc};
use rusqlite::{params, Connection};

use super::group_key::{fold_groups, normalize_group_key, Group, GroupInput, GroupKind};
use crate::queries::library_views::EFFECTIVE_ALBUM_ARTIST;

const CLAMPED_MS: &str =
    "CASE WHEN t.duration_ms > 0 THEN MIN(le.ms_played, t.duration_ms) ELSE le.ms_played END";
// Same fallback rule as `EFFECTIVE_ALBUM_ARTIST`, but deliberately preserves
// the raw spelling so the runtime fold can count and display tag variants.
const RAW_EFFECTIVE_ALBUM_ARTIST: &str =
    "CASE WHEN TRIM(album_artist) <> '' THEN album_artist ELSE artist END";

/// Compatibility payload retained for the unwired remote stats clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthlyListens {
    pub year_month: String,
    pub total_ms: i64,
    pub listens: i64,
}

/// Compatibility payload retained for remote stats and the old composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlineTotals {
    pub total_ms: i64,
    pub total_plays: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopArtist {
    pub artist: String,
    pub plays: i64,
    pub total_ms: i64,
    pub representative_track_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopAlbum {
    pub album: String,
    pub album_artist: String,
    pub plays: i64,
    pub total_ms: i64,
    pub track_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopTrack {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub play_count: i64,
    pub total_ms: i64,
    pub track_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopGenre {
    pub genre: String,
    pub plays: i64,
    pub total_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourlyListens {
    pub hour: i32,
    pub listens: i64,
    pub total_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedGroup {
    pub group: Group,
    pub representative_track_path: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ListenRow {
    pub played_at: i64,
    pub ms: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct NamedRow {
    pub raw: String,
    pub mbid: Option<String>,
    pub plays: i64,
    pub ms: i64,
    pub last_played_at: i64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AlbumRow {
    pub album: String,
    pub artist: NamedRow,
}

#[derive(Debug, Clone)]
pub(crate) struct TrackAggregate {
    pub track: TopTrack,
    pub effective_artist: String,
    pub artist_mbid: Option<String>,
}

/// One named home for the play threshold shared by playback and local stats.
pub fn counts_as_play(position_ms: i64, duration_ms: i64) -> bool {
    crate::scrobbling::should_scrobble(position_ms, duration_ms)
}

/// Records one qualifying local play. The caller applies [`counts_as_play`].
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

pub(crate) fn first_event_unix(conn: &Connection) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row("SELECT MIN(played_at) FROM listen_events", [], |row| {
        row.get(0)
    })
}

pub(crate) fn listen_rows(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<ListenRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT le.played_at, {CLAMPED_MS} \
         FROM listen_events le JOIN tracks t ON t.id = le.track_id \
         WHERE le.played_at >= ?1 AND le.played_at < ?2 \
         ORDER BY le.played_at, le.id"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(params![start_unix, end_unix], |row| {
            Ok(ListenRow {
                played_at: row.get(0)?,
                ms: row.get(1)?,
            })
        })?
        .collect();
    rows
}

pub(crate) fn total_ms_in_range(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<i64, rusqlite::Error> {
    let sql = format!(
        "SELECT COALESCE(SUM({CLAMPED_MS}), 0) \
         FROM listen_events le JOIN tracks t ON t.id = le.track_id \
         WHERE le.played_at >= ?1 AND le.played_at < ?2"
    );
    conn.query_row(&sql, params![start_unix, end_unix], |row| row.get(0))
}

pub(crate) fn artist_rows(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<NamedRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT {RAW_EFFECTIVE_ALBUM_ARTIST} AS raw, \
                MAX(NULLIF(TRIM(t.artist_mbid), '')), COUNT(le.id), \
                COALESCE(SUM({CLAMPED_MS}), 0), MAX(le.played_at), MIN(t.path) \
         FROM listen_events le JOIN tracks t ON t.id = le.track_id \
         WHERE le.played_at >= ?1 AND le.played_at < ?2 \
           AND TRIM({EFFECTIVE_ALBUM_ARTIST}) <> '' \
         GROUP BY raw"
    );
    query_named_rows(conn, &sql, start_unix, end_unix)
}

pub(crate) fn genre_rows(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<NamedRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT t.genre, NULL, COUNT(le.id), COALESCE(SUM({CLAMPED_MS}), 0), \
                MAX(le.played_at), MIN(t.path) \
         FROM listen_events le JOIN tracks t ON t.id = le.track_id \
         WHERE le.played_at >= ?1 AND le.played_at < ?2 AND TRIM(t.genre) <> '' \
         GROUP BY t.genre"
    );
    query_named_rows(conn, &sql, start_unix, end_unix)
}

pub(crate) fn album_rows(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<AlbumRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT t.album, {RAW_EFFECTIVE_ALBUM_ARTIST} AS raw, NULL, COUNT(le.id), \
                COALESCE(SUM({CLAMPED_MS}), 0), MAX(le.played_at), MIN(t.path) \
         FROM listen_events le JOIN tracks t ON t.id = le.track_id \
         WHERE le.played_at >= ?1 AND le.played_at < ?2 \
           AND TRIM(t.album) <> '' AND TRIM({EFFECTIVE_ALBUM_ARTIST}) <> '' \
         GROUP BY t.album, raw"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(params![start_unix, end_unix], |row| {
            Ok(AlbumRow {
                album: row.get(0)?,
                artist: NamedRow {
                    raw: row.get(1)?,
                    mbid: row.get(2)?,
                    plays: row.get(3)?,
                    ms: row.get(4)?,
                    last_played_at: row.get(5)?,
                    path: row.get(6)?,
                },
            })
        })?
        .collect();
    rows
}

pub(crate) fn track_rows(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<TrackAggregate>, rusqlite::Error> {
    let sql = format!(
        "SELECT t.id, t.title, t.artist, t.album, COUNT(le.id), \
                COALESCE(SUM({CLAMPED_MS}), 0), t.path, \
                {RAW_EFFECTIVE_ALBUM_ARTIST}, t.artist_mbid \
         FROM listen_events le JOIN tracks t ON t.id = le.track_id \
         WHERE le.played_at >= ?1 AND le.played_at < ?2 \
         GROUP BY t.id"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(params![start_unix, end_unix], |row| {
            Ok(TrackAggregate {
                track: TopTrack {
                    track_id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    album: row.get(3)?,
                    play_count: row.get(4)?,
                    total_ms: row.get(5)?,
                    track_path: row.get(6)?,
                },
                effective_artist: row.get(7)?,
                artist_mbid: row.get(8)?,
            })
        })?
        .collect();
    rows
}

pub(crate) fn discovered_count(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) FROM ( \
           SELECT track_id, MIN(played_at) AS first_played_at \
           FROM listen_events GROUP BY track_id \
           HAVING first_played_at >= ?1 AND first_played_at < ?2 \
         )",
        params![start_unix, end_unix],
        |row| row.get(0),
    )
}

/// Track ids belonging to an exact runtime metadata group.
pub fn group_track_ids(
    conn: &Connection,
    kind: GroupKind,
    key: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    let raw_expression = match kind {
        GroupKind::Artist | GroupKind::AlbumArtist => RAW_EFFECTIVE_ALBUM_ARTIST,
        GroupKind::Genre => "t.genre",
    };
    let mbid_expression = if kind == GroupKind::Artist {
        "t.artist_mbid"
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT t.id, {raw_expression}, {mbid_expression} FROM tracks t \
         WHERE t.missing_since IS NULL AND t.removed_at IS NULL \
           AND TRIM({raw_expression}) <> '' ORDER BY t.id"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut mbid_by_raw = HashMap::<String, String>::new();
    if kind == GroupKind::Artist {
        for (_, raw, mbid) in &rows {
            let Some(mbid) = mbid
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let stored = mbid_by_raw.entry(raw.clone()).or_default();
            if mbid > stored.as_str() {
                *stored = mbid.to_string();
            }
        }
    }
    let mut ids = Vec::new();
    for (id, raw, mbid) in rows {
        let effective_mbid = mbid_by_raw
            .get(&raw)
            .map(String::as_str)
            .or(mbid.as_deref());
        if key_for(&raw, effective_mbid) == key {
            ids.push(id);
        }
    }
    Ok(ids)
}

pub(crate) fn ranked_groups(rows: &[NamedRow]) -> Vec<RankedGroup> {
    let inputs = rows
        .iter()
        .filter(|row| !normalize_group_key(&row.raw).is_empty())
        .map(|row| GroupInput {
            raw: &row.raw,
            mbid: row.mbid.as_deref(),
            plays: row.plays,
            ms: row.ms,
            last_played_at: row.last_played_at,
        })
        .collect::<Vec<_>>();
    let mut paths = HashMap::<String, String>::new();
    for row in rows {
        let key = key_for(&row.raw, row.mbid.as_deref());
        let path = paths.entry(key).or_insert_with(|| row.path.clone());
        if row.path < *path {
            *path = row.path.clone();
        }
    }
    fold_groups(&inputs)
        .into_iter()
        .map(|group| RankedGroup {
            representative_track_path: paths.get(&group.key).cloned().unwrap_or_default(),
            group,
        })
        .collect()
}

pub(crate) fn key_for(raw: &str, mbid: Option<&str>) -> String {
    fold_groups(&[GroupInput {
        raw,
        mbid,
        plays: 0,
        ms: 0,
        last_played_at: 0,
    }])
    .into_iter()
    .next()
    .map(|group| group.key)
    .unwrap_or_default()
}

fn query_named_rows(
    conn: &Connection,
    sql: &str,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<NamedRow>, rusqlite::Error> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement
        .query_map(params![start_unix, end_unix], |row| {
            Ok(NamedRow {
                raw: row.get(0)?,
                mbid: row.get(1)?,
                plays: row.get(2)?,
                ms: row.get(3)?,
                last_played_at: row.get(4)?,
                path: row.get(5)?,
            })
        })?
        .collect();
    rows
}

fn utc_range(year: Option<i32>) -> (i64, i64) {
    match year {
        Some(year) => {
            let start = Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0).single().unwrap();
            let end = Utc
                .with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
                .single()
                .unwrap();
            (start.timestamp(), end.timestamp())
        }
        None => (i64::MIN, i64::MAX),
    }
}

// Temporary compatibility for the pre-editorial GTK composer. T8 removes
// these wrappers together with that composer; all are event-backed already.
pub fn headline_totals(
    conn: &Connection,
    year: Option<i32>,
) -> Result<HeadlineTotals, rusqlite::Error> {
    let (start, end) = utc_range(year);
    let rows = listen_rows(conn, start, end)?;
    Ok(HeadlineTotals {
        total_ms: rows.iter().map(|row| row.ms).sum(),
        total_plays: rows.len() as i64,
    })
}

pub fn top_artists(
    conn: &Connection,
    limit: usize,
    year: Option<i32>,
) -> Result<Vec<TopArtist>, rusqlite::Error> {
    let (start, end) = utc_range(year);
    Ok(ranked_groups(&artist_rows(conn, start, end)?)
        .into_iter()
        .take(limit)
        .map(|row| TopArtist {
            artist: row.group.label,
            plays: row.group.plays,
            total_ms: row.group.ms,
            representative_track_path: row.representative_track_path,
        })
        .collect())
}

pub fn top_albums(
    conn: &Connection,
    limit: usize,
    year: Option<i32>,
) -> Result<Vec<TopAlbum>, rusqlite::Error> {
    let (start, end) = utc_range(year);
    Ok(fold_album_rows(&album_rows(conn, start, end)?)
        .into_iter()
        .take(limit)
        .collect())
}

pub fn top_tracks(
    conn: &Connection,
    limit: usize,
    year: Option<i32>,
) -> Result<Vec<TopTrack>, rusqlite::Error> {
    let (start, end) = utc_range(year);
    let mut tracks = track_rows(conn, start, end)?
        .into_iter()
        .map(|row| row.track)
        .collect::<Vec<_>>();
    tracks.sort_by(|left, right| {
        right
            .play_count
            .cmp(&left.play_count)
            .then_with(|| right.total_ms.cmp(&left.total_ms))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.track_id.cmp(&right.track_id))
    });
    tracks.truncate(limit);
    Ok(tracks)
}

pub fn monthly_listen_timeseries(
    conn: &Connection,
    now_unix: i64,
) -> Result<Vec<MonthlyListens>, rusqlite::Error> {
    let now = Utc.timestamp_opt(now_unix, 0).earliest();
    let Some(now) = now else {
        return Ok(Vec::new());
    };
    let mut months = Vec::new();
    for offset in (0..12).rev() {
        let zero_based = i64::from(now.year()) * 12 + i64::from(now.month0()) - offset;
        let year = i32::try_from(zero_based.div_euclid(12)).unwrap_or_default();
        let month = u32::try_from(zero_based.rem_euclid(12) + 1).unwrap_or(1);
        let start = Utc
            .with_ymd_and_hms(year, month, 1, 0, 0, 0)
            .single()
            .unwrap();
        let (next_year, next_month) = if month == 12 {
            (year + 1, 1)
        } else {
            (year, month + 1)
        };
        let end = Utc
            .with_ymd_and_hms(next_year, next_month, 1, 0, 0, 0)
            .single()
            .unwrap();
        let rows = listen_rows(conn, start.timestamp(), end.timestamp())?;
        months.push(MonthlyListens {
            year_month: format!("{year:04}-{month:02}"),
            total_ms: rows.iter().map(|row| row.ms).sum(),
            listens: rows.len() as i64,
        });
    }
    Ok(months)
}

pub fn available_years(conn: &Connection) -> Result<Vec<i32>, rusqlite::Error> {
    let mut statement =
        conn.prepare("SELECT played_at FROM listen_events ORDER BY played_at DESC")?;
    let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
    let mut years = Vec::new();
    for row in rows {
        let Some(value) = Utc.timestamp_opt(row?, 0).earliest() else {
            continue;
        };
        if !years.contains(&value.year()) {
            years.push(value.year());
        }
    }
    Ok(years)
}

pub(crate) fn fold_album_rows(rows: &[AlbumRow]) -> Vec<TopAlbum> {
    let mut by_album = HashMap::<String, Vec<&NamedRow>>::new();
    for row in rows {
        by_album
            .entry(row.album.clone())
            .or_default()
            .push(&row.artist);
    }
    let mut albums = Vec::new();
    for (album, artists) in by_album {
        let owned = artists.into_iter().cloned().collect::<Vec<_>>();
        for artist in ranked_groups(&owned) {
            albums.push(TopAlbum {
                album: album.clone(),
                album_artist: artist.group.label,
                plays: artist.group.plays,
                total_ms: artist.group.ms,
                track_path: artist.representative_track_path,
            });
        }
    }
    albums.sort_by(|left, right| {
        right
            .total_ms
            .cmp(&left.total_ms)
            .then_with(|| right.plays.cmp(&left.plays))
            .then_with(|| left.album.cmp(&right.album))
            .then_with(|| left.album_artist.cmp(&right.album_artist))
    });
    albums
}

#[cfg(test)]
#[path = "stats_screen_tests.rs"]
mod tests;
