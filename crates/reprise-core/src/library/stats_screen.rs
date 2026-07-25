//! Local, read-only primitives for the My Stats snapshot.
//!
//! Every local aggregate is projected from self-contained `listen_events`.
//! The current catalog and its running counter remain available elsewhere,
//! but never feed this module's stats-screen queries.

use std::collections::HashMap;

use rusqlite::{params, Connection};

use super::group_key::{
    fold_groups, normalize_group_key, Group, GroupInput, GroupKind, KeyResolver,
};

const CLAMPED_MS: &str =
    "CASE WHEN le.duration_ms > 0 THEN MIN(le.ms_played, le.duration_ms) ELSE le.ms_played END";
// Same fallback rule as `EFFECTIVE_ALBUM_ARTIST`, but deliberately preserves
// the raw spelling so the runtime fold can count and display tag variants.
const RAW_EFFECTIVE_ALBUM_ARTIST: &str =
    "CASE WHEN TRIM(le.album_artist) <> '' THEN le.album_artist ELSE le.artist END";
const CURRENT_EFFECTIVE_ALBUM_ARTIST: &str =
    "CASE WHEN TRIM(t.album_artist) <> '' THEN t.album_artist ELSE t.artist END";

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
    pub effective_artist: String,
    pub play_count: i64,
    pub total_ms: i64,
    pub track_path: String,
}

/// Metadata owned by an in-flight play. It deliberately contains every
/// field My Stats needs so a track removed from the catalog before the play
/// qualifies can still become a complete historical event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenEventSnapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub duration_ms: i64,
    pub path: String,
    pub artist_mbid: Option<String>,
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
pub(crate) struct GenreArtistRow {
    pub genre_raw: String,
    pub artist: NamedRow,
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
}

/// `tracks.artist_mbid` is keyed to the raw `artist` column, but the stats
/// screen groups by the effective *album* artist. On a compilation row those
/// two name different acts, and using the MBID there would fold a guest into
/// the host's numbers (and into the host's "Play"). Only an album artist that
/// is absent, or the same act under another spelling, keeps the MBID.
fn eligible_artist_mbid<'a>(
    artist: &str,
    album_artist: &str,
    mbid: Option<&'a str>,
) -> Option<&'a str> {
    let mbid = mbid.map(str::trim).filter(|value| !value.is_empty())?;
    let is_same_act = album_artist.trim().is_empty()
        || normalize_group_key(album_artist) == normalize_group_key(artist);
    is_same_act.then_some(mbid)
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
    snapshot: &ListenEventSnapshot,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO listen_events
         (track_id, played_at, ms_played, title, artist, album, album_artist,
          genre, duration_ms, path, artist_mbid)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            track_id,
            played_at,
            ms_played,
            snapshot.title,
            snapshot.artist,
            snapshot.album,
            snapshot.album_artist,
            snapshot.genre,
            snapshot.duration_ms,
            snapshot.path,
            snapshot.artist_mbid,
        ],
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
         FROM listen_events le \
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
         FROM listen_events le \
         WHERE le.played_at >= ?1 AND le.played_at < ?2"
    );
    conn.query_row(&sql, params![start_unix, end_unix], |row| row.get(0))
}

pub(crate) fn artist_rows(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<NamedRow>, rusqlite::Error> {
    // Grouped one level finer than the fold needs, because MBID eligibility is
    // a per-row question (see `eligible_artist_mbid`) and SQLite cannot answer
    // it: its `lower()` folds no diacritics. Rust decides, then folds.
    let sql = format!(
        "SELECT {RAW_EFFECTIVE_ALBUM_ARTIST} AS raw, le.artist, le.album_artist, \
                NULLIF(TRIM(le.artist_mbid), ''), COUNT(le.id), \
                COALESCE(SUM({CLAMPED_MS}), 0), MAX(le.played_at), MIN(le.path) \
         FROM listen_events le \
         WHERE le.played_at >= ?1 AND le.played_at < ?2 \
           AND TRIM({RAW_EFFECTIVE_ALBUM_ARTIST}) <> '' \
         GROUP BY raw, le.artist, le.album_artist, le.artist_mbid"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(params![start_unix, end_unix], |row| {
            let artist: String = row.get(1)?;
            let album_artist: String = row.get(2)?;
            let mbid: Option<String> = row.get(3)?;
            Ok(NamedRow {
                raw: row.get(0)?,
                mbid: eligible_artist_mbid(&artist, &album_artist, mbid.as_deref())
                    .map(str::to_string),
                plays: row.get(4)?,
                ms: row.get(5)?,
                last_played_at: row.get(6)?,
                path: row.get(7)?,
            })
        })?
        .collect();
    rows
}

pub(crate) fn genre_rows(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<NamedRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT le.genre, NULL, COUNT(le.id), COALESCE(SUM({CLAMPED_MS}), 0), \
                MAX(le.played_at), MIN(le.path) \
         FROM listen_events le \
         WHERE le.played_at >= ?1 AND le.played_at < ?2 AND TRIM(le.genre) <> '' \
         GROUP BY le.genre"
    );
    query_named_rows(conn, &sql, start_unix, end_unix)
}

pub(crate) fn genre_artist_rows(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<GenreArtistRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT le.genre, {RAW_EFFECTIVE_ALBUM_ARTIST} AS raw, le.artist, le.album_artist, \
                NULLIF(TRIM(le.artist_mbid), ''), COUNT(le.id), \
                COALESCE(SUM({CLAMPED_MS}), 0), MAX(le.played_at), le.path \
         FROM listen_events le \
         WHERE le.played_at >= ?1 AND le.played_at < ?2 \
           AND TRIM(le.genre) <> '' \
         GROUP BY le.genre, raw, le.artist, le.album_artist, le.artist_mbid, le.path"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(params![start_unix, end_unix], |row| {
            let artist: String = row.get(2)?;
            let album_artist: String = row.get(3)?;
            let mbid: Option<String> = row.get(4)?;
            Ok(GenreArtistRow {
                genre_raw: row.get(0)?,
                artist: NamedRow {
                    raw: row.get(1)?,
                    mbid: eligible_artist_mbid(&artist, &album_artist, mbid.as_deref())
                        .map(str::to_string),
                    plays: row.get(5)?,
                    ms: row.get(6)?,
                    last_played_at: row.get(7)?,
                    path: row.get(8)?,
                },
            })
        })?
        .collect();
    rows
}

pub(crate) fn album_rows(
    conn: &Connection,
    start_unix: i64,
    end_unix: i64,
) -> Result<Vec<AlbumRow>, rusqlite::Error> {
    let sql = format!(
        "SELECT le.album, {RAW_EFFECTIVE_ALBUM_ARTIST} AS raw, NULL, COUNT(le.id), \
                COALESCE(SUM({CLAMPED_MS}), 0), MAX(le.played_at), MIN(le.path) \
         FROM listen_events le \
         WHERE le.played_at >= ?1 AND le.played_at < ?2 \
           AND TRIM(le.album) <> '' AND TRIM({RAW_EFFECTIVE_ALBUM_ARTIST}) <> '' \
         GROUP BY le.album, raw"
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
        "WITH aggregate AS ( \
           SELECT le.track_id, COUNT(le.id) AS plays, \
                  COALESCE(SUM({CLAMPED_MS}), 0) AS total_ms \
           FROM listen_events le \
           WHERE le.played_at >= ?1 AND le.played_at < ?2 \
           GROUP BY le.track_id \
         ) \
         SELECT latest.track_id, latest.title, latest.artist, latest.album, \
                aggregate.plays, aggregate.total_ms, latest.path, \
                CASE WHEN TRIM(latest.album_artist) <> '' \
                     THEN latest.album_artist ELSE latest.artist END \
         FROM aggregate \
         JOIN listen_events latest ON latest.id = ( \
           SELECT candidate.id FROM listen_events candidate \
           WHERE candidate.track_id = aggregate.track_id \
             AND candidate.played_at >= ?1 AND candidate.played_at < ?2 \
           ORDER BY candidate.played_at DESC, candidate.id DESC LIMIT 1 \
         )"
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
                    effective_artist: row.get(7)?,
                    play_count: row.get(4)?,
                    total_ms: row.get(5)?,
                    track_path: row.get(6)?,
                },
                effective_artist: row.get(7)?,
            })
        })?
        .collect();
    rows
}

/// Track ids belonging to an exact runtime metadata group.
///
/// Deliberately spans the whole catalog, not the selected period: pressing
/// "Play" on a stats row plays the artist, not the slice of them that happened
/// to be heard this year. Missing and removed tracks are excluded because they
/// cannot be played, even though their listen events still feed the numbers.
pub fn group_track_ids(
    conn: &Connection,
    kind: GroupKind,
    key: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    let raw_expression = match kind {
        GroupKind::Artist | GroupKind::AlbumArtist => CURRENT_EFFECTIVE_ALBUM_ARTIST,
        GroupKind::Genre => "t.genre",
    };
    let sql = format!(
        "SELECT t.id, {raw_expression}, t.artist, t.album_artist, t.artist_mbid FROM tracks t \
         WHERE t.missing_since IS NULL AND t.removed_at IS NULL \
           AND TRIM({raw_expression}) <> '' ORDER BY t.id"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map([], |row| {
            Ok(CatalogRow {
                id: row.get(0)?,
                raw: row.get(1)?,
                artist: row.get(2)?,
                album_artist: row.get(3)?,
                mbid: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Same resolution as the snapshot, over a different population. Each track
    // weighs one play, because the catalog carries no listen counts here.
    let uses_mbid = kind == GroupKind::Artist;
    let resolver = KeyResolver::build(rows.iter().map(|row| {
        GroupInput {
            raw: &row.raw,
            mbid: uses_mbid
                .then(|| eligible_artist_mbid(&row.artist, &row.album_artist, row.mbid.as_deref()))
                .flatten(),
            plays: 1,
            ms: 0,
            last_played_at: 0,
        }
    }));

    let ids = rows
        .iter()
        .filter(|row| resolver.key_for(&row.raw) == key)
        .map(|row| row.id)
        .collect::<Vec<_>>();
    if !ids.is_empty() {
        return Ok(ids);
    }

    // The key came from a period-scoped fold; the catalog may have resolved the
    // same act to another MBID, or to none. Recover it by name before giving up.
    let names = resolver.names_for_key(key);
    let ids = rows
        .iter()
        .filter(|row| names.contains(&normalize_group_key(&row.raw)))
        .map(|row| row.id)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        tracing::warn!(
            ?kind,
            key,
            "stats group key resolved to no playable track; the group is empty or entirely missing"
        );
    } else {
        tracing::debug!(?kind, key, "stats group key recovered by name fallback");
    }
    Ok(ids)
}

struct CatalogRow {
    id: i64,
    raw: String,
    artist: String,
    album_artist: String,
    mbid: Option<String>,
}

/// The key resolution behind one set of aggregate rows. Callers that need to
/// key something else against the same aggregates (the spotlight keying tracks,
/// say) must reuse this rather than resolve a key of their own.
pub(crate) fn key_resolver(rows: &[NamedRow]) -> KeyResolver {
    KeyResolver::build(rows.iter().map(|row| GroupInput {
        raw: &row.raw,
        mbid: row.mbid.as_deref(),
        plays: row.plays,
        ms: row.ms,
        last_played_at: row.last_played_at,
    }))
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
    let resolver = key_resolver(rows);
    let mut paths = HashMap::<String, String>::new();
    for row in rows {
        let path = paths
            .entry(resolver.key_for(&row.raw))
            .or_insert_with(|| row.path.clone());
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

pub(crate) fn fold_album_rows(rows: &[AlbumRow]) -> Vec<TopAlbum> {
    // The title folds by the same rule as the artist half of the row, or
    // "Immortal" and "immortal " would stay two rows of one artist.
    let mut by_album = HashMap::<String, Vec<&AlbumRow>>::new();
    for row in rows {
        let key = normalize_group_key(&row.album);
        if key.is_empty() {
            continue;
        }
        by_album.entry(key).or_default().push(row);
    }
    let mut albums = Vec::new();
    for variants in by_album.into_values() {
        let album = album_label(&variants);
        let owned = variants
            .iter()
            .map(|row| row.artist.clone())
            .collect::<Vec<_>>();
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

/// The displayed spelling of one folded album title, chosen by the same rule
/// STATS-9 gives artist labels: most played, then last played, then alphabetic.
fn album_label(variants: &[&AlbumRow]) -> String {
    let inputs = variants
        .iter()
        .map(|row| GroupInput {
            raw: &row.album,
            mbid: None,
            plays: row.artist.plays,
            ms: row.artist.ms,
            last_played_at: row.artist.last_played_at,
        })
        .collect::<Vec<_>>();
    fold_groups(&inputs)
        .into_iter()
        .next()
        .map(|group| group.label)
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "stats_screen_tests.rs"]
mod tests;
