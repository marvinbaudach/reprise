//! The query layer that reads stored releases back out of the database and
//! annotates them with local-library presence. Split out of
//! `artist_news.rs` purely to stay under the project's 800-line rule;
//! re-exported from there so existing callers keep using
//! `artist_news::{query_releases, StoredRelease, LibraryPresence, ...}`.

use std::cmp::Ordering;
use std::path::PathBuf;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::artist_news::{normalize, AlbumNews, ArtistNews, NewsKind};
use crate::artist_news_parsing::{parse_partial_date, release_kind};

const MAX_NEWS_ITEMS_PER_ARTIST: usize = 20;

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
    pub track_count: Option<i64>,
    pub local_track_count: i64,
}

/// `(normalized album artist, normalized album) → distinct track count` for
/// the local library. Numbered disc/track slots prevent duplicate files from
/// inventing ownership; title identity is the conservative fallback for
/// unnumbered files.
pub(crate) fn local_album_track_counts(
    conn: &Connection,
) -> Result<std::collections::HashMap<(String, String), i64>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT path, title, artist, album_artist, album, disc_no, track_no
         FROM tracks
         WHERE removed_at IS NULL AND missing_since IS NULL AND trim(album) <> ''",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<i64>>(6)?,
        ))
    })?;

    let mut slots =
        std::collections::HashMap::<(String, String), std::collections::HashSet<String>>::new();
    for row in rows {
        let (path, title, artist, album_artist, album, disc_no, track_no) = row?;
        let release_artist = if album_artist.trim().is_empty() {
            artist
        } else {
            album_artist
        };
        let slot = match track_no.filter(|value| *value > 0) {
            Some(track_no) => format!("position:{}:{track_no}", disc_no.unwrap_or(1).max(1)),
            None if !title.trim().is_empty() => format!("title:{}", normalize(&title)),
            None => format!("path:{path}"),
        };
        slots
            .entry((normalize(&release_artist), normalize(&album)))
            .or_default()
            .insert(slot);
    }
    slots
        .into_iter()
        .map(|(key, tracks)| {
            i64::try_from(tracks.len())
                .map(|count| (key, count))
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
        })
        .collect()
}

pub(crate) fn local_track_count(
    counts: &std::collections::HashMap<(String, String), i64>,
    artist: &str,
    title: &str,
) -> i64 {
    counts
        .get(&(normalize(artist), normalize(title)))
        .copied()
        .unwrap_or(0)
}

/// Complete ownership requires a positive local match and a trusted official
/// Album/EP length. Unknown or single-sized remote metadata remains partial
/// so an advance single can never hide its later release.
pub(crate) fn presence_for(
    counts: &std::collections::HashMap<(String, String), i64>,
    artist: &str,
    title: &str,
    official_track_count: Option<i64>,
) -> LibraryPresence {
    let local_count = local_track_count(counts, artist, title);
    match local_count {
        0 => LibraryPresence::Absent,
        count
            if official_track_count
                .filter(|expected| *expected >= 2)
                .is_some_and(|expected| count >= expected) =>
        {
            LibraryPresence::Complete
        }
        _ => LibraryPresence::Partial,
    }
}

pub(crate) fn release_presence(
    counts: &std::collections::HashMap<(String, String), i64>,
    artist: &str,
    title: &str,
    official_track_count: Option<i64>,
    first_release_date: &str,
    today: NaiveDate,
) -> LibraryPresence {
    let presence = presence_for(counts, artist, title, official_track_count);
    if presence == LibraryPresence::Complete
        && parse_partial_date(first_release_date).is_some_and(|date| date >= today)
    {
        LibraryPresence::Partial
    } else {
        presence
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
                announce_url, track_count
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
                track_count: row.get(11)?,
                local_track_count: 0,
                presence: LibraryPresence::Absent,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let counts = local_album_track_counts(conn)?;
    for release in &mut releases {
        release.local_track_count =
            local_track_count(&counts, &release.artist_name, &release.title);
        release.presence = release_presence(
            &counts,
            &release.artist_name,
            &release.title,
            release.track_count,
            &release.first_release_date,
            today,
        );
    }
    releases.retain(|release| {
        parse_partial_date(&release.first_release_date)
            .and_then(|release_date| {
                release_kind(
                    &release.release_type.to_ascii_lowercase(),
                    &release.first_release_date,
                    release_date,
                    today,
                    true,
                )
            })
            .is_some_and(|kind| kind != NewsKind::Catalog)
    });
    releases.sort_by(|left, right| compare_stored_releases(left, right, today));
    let mut per_artist = std::collections::HashMap::<String, usize>::new();
    releases.retain(|release| {
        let count = per_artist.entry(release.artist_mbid.clone()).or_default();
        if *count >= MAX_NEWS_ITEMS_PER_ARTIST {
            return false;
        }
        *count += 1;
        true
    });
    Ok(releases)
}

pub fn unseen_release_count(conn: &Connection, today: NaiveDate) -> Result<i64, rusqlite::Error> {
    Ok(query_releases(conn, true, today)?
        .into_iter()
        .filter(|release| {
            release.seen_at.is_none() && release.presence != LibraryPresence::Complete
        })
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
            kind: parse_partial_date(&release.first_release_date)
                .and_then(|release_date| {
                    release_kind(
                        &release.release_type.to_ascii_lowercase(),
                        &release.first_release_date,
                        release_date,
                        today,
                        true,
                    )
                })
                .unwrap_or(NewsKind::New),
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
    match (left_date >= today, right_date >= today) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => left_date.cmp(&right_date),
        (false, false) => right_date.cmp(&left_date),
    }
    .then_with(|| left.title.cmp(&right.title))
}
