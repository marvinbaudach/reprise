//! The query layer that reads stored releases back out of the database and
//! annotates them with local-library presence. Split out of
//! `artist_news.rs` purely to stay under the project's 800-line rule;
//! re-exported from there so existing callers keep using
//! `artist_news::{query_releases, StoredRelease, LibraryPresence, ...}`.

use std::cmp::Ordering;
use std::path::PathBuf;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::artist_news::{normalize, AlbumNews, ArtistNews, NewsKind, OWNED_ALBUM_MIN_TRACKS};
use crate::artist_news_parsing::parse_partial_date;

/// How much of a release the local library already holds. A `bool` cannot
/// express the case this feature exists for: you own the lead single, so the
/// album is *relevant* to you — but calling that "in library" would send you
/// to the library instead of to the announcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryPresence {
    Absent,
    Partial,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRelease {
    pub release_group_mbid: String,
    pub artist_name: String,
    pub artist_mbid: String,
    pub title: String,
    pub release_type: String,
    pub first_release_date: String,
    pub fetched_at: i64,
    pub seen_at: Option<i64>,
    pub hidden: bool,
    pub fallback_accent: String,
    pub presence: LibraryPresence,
    pub announce_url: Option<String>,
}

/// `(normalized artist, normalized album) → track count` for the local
/// library. Shared by `query_releases`' presence annotation and
/// `query_history`'s identical need. Deliberately threshold-free: this
/// describes the library, it does not filter — the threshold lives in
/// `presence_for`.
pub(crate) fn local_album_track_counts(
    conn: &Connection,
) -> Result<std::collections::HashMap<(String, String), i64>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT artist, album FROM tracks
         WHERE removed_at IS NULL AND missing_since IS NULL AND trim(album) <> ''",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    // Aggregate in Rust under `normalize()`, not in SQL. SQL's
    // `lower(trim(x))` only lowercases and trims the ends, while `normalize()`
    // also collapses internal whitespace runs — grouping in SQL would split a
    // single album's tracks across separate groups whenever a tagging
    // inconsistency differs only by internal whitespace, undercounting it.
    let mut counts = std::collections::HashMap::new();
    for row in rows {
        let (artist, album) = row?;
        *counts
            .entry((normalize(&artist), normalize(&album)))
            .or_insert(0) += 1;
    }
    Ok(counts)
}

/// Maps a track count onto the presence states. `OWNED_ALBUM_MIN_TRACKS`
/// defines the sole query-time meaning of "counts as owned".
pub(crate) fn presence_for(
    counts: &std::collections::HashMap<(String, String), i64>,
    artist: &str,
    title: &str,
) -> LibraryPresence {
    match counts
        .get(&(normalize(artist), normalize(title)))
        .copied()
        .unwrap_or(0)
    {
        0 => LibraryPresence::Absent,
        count if count < OWNED_ALBUM_MIN_TRACKS => LibraryPresence::Partial,
        _ => LibraryPresence::Complete,
    }
}

pub fn query_releases(
    conn: &Connection,
    include_hidden: bool,
    today: NaiveDate,
) -> Result<Vec<StoredRelease>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT release_group_mbid, artist_name, artist_mbid, title, release_type,
                first_release_date, fetched_at, seen_at, hidden, fallback_accent,
                announce_url
         FROM new_releases
         WHERE ?1 OR hidden = 0",
    )?;
    let mut releases = statement
        .query_map([i64::from(include_hidden)], |row| {
            Ok(StoredRelease {
                release_group_mbid: row.get(0)?,
                artist_name: row.get(1)?,
                artist_mbid: row.get(2)?,
                title: row.get(3)?,
                release_type: row.get(4)?,
                first_release_date: row.get(5)?,
                fetched_at: row.get(6)?,
                seen_at: row.get(7)?,
                hidden: row.get::<_, i64>(8)? != 0,
                fallback_accent: row.get(9)?,
                announce_url: row.get(10)?,
                presence: LibraryPresence::Absent,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let counts = local_album_track_counts(conn)?;
    for release in &mut releases {
        release.presence = presence_for(&counts, &release.artist_name, &release.title);
    }
    releases.sort_by(|left, right| compare_stored_releases(left, right, today));
    Ok(releases)
}

pub fn unseen_release_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    let mut statement =
        conn.prepare("SELECT artist_name, title FROM new_releases WHERE seen_at IS NULL")?;
    let releases = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let counts = local_album_track_counts(conn)?;
    Ok(releases
        .into_iter()
        .filter(|(artist, title)| presence_for(&counts, artist, title) != LibraryPresence::Complete)
        .count() as i64)
}

pub fn hidden_release_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) FROM new_releases WHERE hidden = 1",
        [],
        |row| row.get(0),
    )
}

pub fn set_release_hidden(
    conn: &Connection,
    release_group_mbid: &str,
    hidden: bool,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE new_releases
            SET hidden = ?1,
                hidden_at = CASE WHEN ?1 = 1 THEN strftime('%s', 'now') ELSE NULL END
          WHERE release_group_mbid = ?2",
        rusqlite::params![i64::from(hidden), release_group_mbid],
    )?;
    Ok(())
}

pub fn mark_releases_seen(
    conn: &Connection,
    release_group_mbids: &[String],
    seen_at: i64,
) -> Result<(), rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    for mbid in release_group_mbids {
        transaction.execute(
            "UPDATE new_releases SET seen_at = ?1
             WHERE release_group_mbid = ?2 AND seen_at IS NULL",
            rusqlite::params![seen_at, mbid],
        )?;
    }
    transaction.commit()
}

pub fn query_artist_news(
    conn: &Connection,
    artist_mbid: &str,
    today: NaiveDate,
) -> Result<Option<ArtistNews>, rusqlite::Error> {
    let releases = query_releases(conn, false, today)?
        .into_iter()
        .filter(|release| release.artist_mbid == artist_mbid)
        .collect::<Vec<_>>();
    let Some(first) = releases.first() else {
        return Ok(None);
    };
    let artist = first.artist_name.clone();
    let fetched_at = releases
        .iter()
        .map(|release| release.fetched_at)
        .max()
        .unwrap_or_default();
    let items = releases
        .into_iter()
        .map(|release| AlbumNews {
            release_group_mbid: release.release_group_mbid,
            title: release.title,
            kind: parse_partial_date(&release.first_release_date).map_or(NewsKind::New, |date| {
                if date >= today {
                    NewsKind::Upcoming
                } else {
                    NewsKind::New
                }
            }),
            first_release_date: release.first_release_date,
            primary_type: release.release_type,
            announce_url: release.announce_url,
        })
        .collect();
    Ok(Some(ArtistNews {
        artist,
        artist_mbid: artist_mbid.to_string(),
        fetched_at,
        items,
        stale: false,
    }))
}

pub fn query_artist_news_by_name(
    conn: &Connection,
    artist: &str,
    today: NaiveDate,
) -> Result<Option<ArtistNews>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT artist_mbid FROM tracks
         WHERE lower(trim(artist)) = lower(trim(?1)) AND artist_mbid IS NOT NULL
         ORDER BY play_count DESC, id ASC
         LIMIT 1",
    )?;
    let mut rows = statement.query([artist])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let artist_mbid = row.get::<_, String>(0)?;
    query_artist_news(conn, &artist_mbid, today)
}

pub fn most_played_album_track_path(
    conn: &Connection,
    artist: &str,
) -> Result<Option<PathBuf>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT MIN(path), SUM(play_count) AS album_plays
         FROM tracks
         WHERE lower(trim(artist)) = lower(trim(?1))
           AND removed_at IS NULL AND missing_since IS NULL AND trim(album) <> ''
         GROUP BY lower(trim(album))
         ORDER BY album_plays DESC, lower(trim(album)) ASC
         LIMIT 1",
    )?;
    let mut rows = statement.query([artist])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(PathBuf::from(row.get::<_, String>(0)?)))
}

fn compare_stored_releases(
    left: &StoredRelease,
    right: &StoredRelease,
    today: NaiveDate,
) -> Ordering {
    let left_date = parse_partial_date(&left.first_release_date).unwrap_or(today);
    let right_date = parse_partial_date(&right.first_release_date).unwrap_or(today);
    let left_kind = if left_date >= today {
        NewsKind::Upcoming
    } else {
        NewsKind::New
    };
    let right_kind = if right_date >= today {
        NewsKind::Upcoming
    } else {
        NewsKind::New
    };
    match (left_kind, right_kind) {
        (NewsKind::Upcoming, NewsKind::New) => Ordering::Less,
        (NewsKind::New, NewsKind::Upcoming) => Ordering::Greater,
        (NewsKind::Upcoming, NewsKind::Upcoming) => left_date.cmp(&right_date),
        (NewsKind::New, NewsKind::New) => right_date.cmp(&left_date),
    }
    .then_with(|| left.title.cmp(&right.title))
}
