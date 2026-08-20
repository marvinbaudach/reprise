//! The query layer that reads stored releases back out of the database and
//! annotates them with local-library presence. Split out of
//! `artist_news.rs` purely to stay under the project's 800-line rule;
//! re-exported from there so existing callers keep using
//! `artist_news::{StoredRelease, LibraryPresence, ...}`.

use std::cmp::Ordering;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::artist_news::{normalize, AlbumNews, ArtistNews, NewsKind};
use crate::artist_news_parsing::{is_announcement_candidate, parse_partial_date, release_kind};
use crate::artist_news_scope::{catalog_type, collapse_duplicates, counts_as_owned, ScopedRelease};

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
    pub presence: LibraryPresence,
    pub announce_url: Option<String>,
    pub track_count: Option<i64>,
    pub local_track_count: i64,
}

impl ScopedRelease for StoredRelease {
    fn artist_name(&self) -> &str {
        &self.artist_name
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn first_release_date(&self) -> &str {
        &self.first_release_date
    }

    fn release_type(&self) -> &str {
        &self.release_type
    }

    fn track_count(&self) -> Option<i64> {
        self.track_count
    }

    fn release_group_mbid(&self) -> &str {
        &self.release_group_mbid
    }
}

/// The two library identities the release catalog matches against.
///
/// Albums are keyed by `(album artist, album)`, singles by `(album artist,
/// track title)` — two indexes over the very same rows. Building them in
/// separate queries cost a second full scan of `tracks` on every catalog
/// render, every badge count and every popover open, which is why they now
/// share one pass.
pub(crate) struct LocalLibraryIndex {
    pub(crate) album_track_counts: std::collections::HashMap<(String, String), i64>,
    pub(crate) track_titles: std::collections::HashSet<(String, String)>,
}

/// Reads both indexes in a single pass over the present library.
///
/// Numbered disc/track slots prevent duplicate files from inventing album
/// ownership; title identity is the conservative fallback for unnumbered
/// files. A track without an album contributes its title but no album slot —
/// that used to be an `AND trim(album) <> ''` in SQL and is now the same
/// decision in Rust, because the title index needs those rows.
pub(crate) fn local_library_index(conn: &Connection) -> Result<LocalLibraryIndex, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT path, title, artist, album_artist, album, disc_no, track_no
         FROM tracks
         WHERE removed_at IS NULL AND missing_since IS NULL",
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
    let mut track_titles = std::collections::HashSet::new();
    for row in rows {
        let (path, title, artist, album_artist, album, disc_no, track_no) = row?;
        let release_artist = if album_artist.trim().is_empty() {
            artist
        } else {
            album_artist
        };
        if !release_artist.trim().is_empty() && !title.trim().is_empty() {
            track_titles.insert((normalize(&release_artist), normalize(&title)));
        }
        if album.trim().is_empty() {
            continue;
        }
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

    let album_track_counts = slots
        .into_iter()
        .map(|(key, tracks)| {
            i64::try_from(tracks.len())
                .map(|count| (key, count))
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
        })
        .collect::<Result<_, _>>()?;
    Ok(LocalLibraryIndex {
        album_track_counts,
        track_titles,
    })
}

/// Album ownership alone, for the fetch pipeline — it never asks about
/// singles, and it runs once per refresh rather than once per render.
pub(crate) fn local_album_track_counts(
    conn: &Connection,
) -> Result<std::collections::HashMap<(String, String), i64>, rusqlite::Error> {
    Ok(local_library_index(conn)?.album_track_counts)
}

pub(crate) fn local_count_for_release(
    album_counts: &std::collections::HashMap<(String, String), i64>,
    track_titles: &std::collections::HashSet<(String, String)>,
    artist: &str,
    title: &str,
    release_type: &str,
) -> i64 {
    if release_type.eq_ignore_ascii_case("single") {
        i64::from(track_titles.contains(&(normalize(artist), normalize(title))))
    } else {
        local_track_count(album_counts, artist, title)
    }
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

pub(crate) fn query_releases_in(
    conn: &Connection,
    include_hidden: bool,
    today: NaiveDate,
) -> Result<Vec<StoredRelease>, rusqlite::Error> {
    let mut releases = load_releases_in(conn, include_hidden, today)?;
    releases.retain(|release| {
        parse_partial_date(&release.first_release_date)
            .and_then(|release_date| {
                release_kind(
                    &release.release_type.to_ascii_lowercase(),
                    &release.first_release_date,
                    release_date,
                    today,
                )
            })
            .is_some_and(|kind| kind != NewsKind::Catalog)
    });
    cap_releases_per_artist(&mut releases);
    Ok(releases)
}

fn load_releases_in(
    conn: &Connection,
    include_hidden: bool,
    today: NaiveDate,
) -> Result<Vec<StoredRelease>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT release_group_mbid, artist_name, artist_mbid, title, release_type,
                first_release_date, fetched_at, seen_at, hidden,
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
                announce_url: row.get(9)?,
                track_count: row.get(10)?,
                local_track_count: 0,
                presence: LibraryPresence::Absent,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let library = local_library_index(conn)?;
    let (counts, track_titles) = (library.album_track_counts, library.track_titles);
    for release in &mut releases {
        release.local_track_count = local_count_for_release(
            &counts,
            &track_titles,
            &release.artist_name,
            &release.title,
            &release.release_type,
        );
        release.presence = release_presence(
            &counts,
            &release.artist_name,
            &release.title,
            release.track_count,
            &release.first_release_date,
            today,
        );
    }
    releases.sort_by(|left, right| compare_stored_releases(left, right, today));
    Ok(releases)
}

pub(crate) fn release_notification_candidates(
    db: &crate::db::Db,
    run_started_at: i64,
    today: NaiveDate,
) -> Result<Vec<StoredRelease>, rusqlite::Error> {
    let conn = db.conn();
    let mut statement = conn.prepare(
        "SELECT release_group_mbid
           FROM new_releases
          WHERE fetched_at < ?1
            AND notified_released_at IS NULL",
    )?;
    let eligible = statement
        .query_map([run_started_at], |row| row.get::<_, String>(0))?
        .collect::<Result<std::collections::HashSet<_>, _>>()?;
    Ok(load_releases_in(conn, false, today)?
        .into_iter()
        .filter(|release| eligible.contains(&release.release_group_mbid))
        .collect())
}

pub(crate) fn mark_release_notified_at(
    db: &crate::db::Db,
    release_group_mbid: &str,
    notified_at: i64,
) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    conn.execute(
        "UPDATE new_releases
            SET notified_released_at = ?1
          WHERE release_group_mbid = ?2
            AND notified_released_at IS NULL",
        rusqlite::params![notified_at, release_group_mbid],
    )?;
    Ok(())
}

fn cap_releases_per_artist(releases: &mut Vec<StoredRelease>) {
    let mut per_artist = std::collections::HashMap::<String, usize>::new();
    releases.retain(|release| {
        let count = per_artist.entry(release.artist_mbid.clone()).or_default();
        if *count >= MAX_NEWS_ITEMS_PER_ARTIST {
            return false;
        }
        *count += 1;
        true
    });
}

/// The visible release candidates for the Updates popover: upcoming releases
/// and releases from the last 90 days under the persisted type scope,
/// excluding anything the library counts as owned.
///
/// This is also the set [`unseen_release_count`] counts. Keeping the filter in
/// one query prevents the popover list and its badge from disagreeing.
pub fn delta_candidates(
    db: &crate::db::Db,
    today: NaiveDate,
) -> Result<Vec<StoredRelease>, rusqlite::Error> {
    let conn = db.conn();
    let filter = crate::artist_news_view::persisted_releases_filter(db)?;
    let mut releases = load_releases_in(conn, false, today)?
        .into_iter()
        .filter(|release| {
            !counts_as_owned(
                release.presence,
                &release.release_type,
                &release.first_release_date,
                release.track_count,
                release.local_track_count,
                today,
            )
        })
        .filter(|release| catalog_type(&release.release_type))
        .filter(|release| filter.release_types.includes(&release.release_type))
        .filter(|release| {
            is_announcement_candidate(&release.release_type, &release.first_release_date, today)
        })
        .collect();
    releases = collapse_duplicates(releases);
    cap_releases_per_artist(&mut releases);
    Ok(releases)
}

pub fn unseen_release_count(db: &crate::db::Db, today: NaiveDate) -> Result<i64, rusqlite::Error> {
    Ok(delta_candidates(db, today)?
        .into_iter()
        .filter(|release| release.seen_at.is_none())
        .count() as i64)
}

pub fn hidden_release_count(db: &crate::db::Db) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    conn.query_row(
        "SELECT COUNT(*) FROM new_releases WHERE hidden = 1",
        [],
        |row| row.get(0),
    )
}

pub fn set_release_hidden(
    db: &crate::db::Db,
    release_group_mbid: &str,
    hidden: bool,
) -> Result<(), rusqlite::Error> {
    set_releases_hidden(
        db,
        std::slice::from_ref(&release_group_mbid.to_owned()),
        hidden,
    )
}

pub fn set_releases_hidden(
    db: &crate::db::Db,
    release_group_mbids: &[String],
    hidden: bool,
) -> Result<(), rusqlite::Error> {
    if release_group_mbids.is_empty() {
        return Ok(());
    }
    let conn = db.conn();
    let transaction = conn.unchecked_transaction()?;
    for mbid in release_group_mbids {
        apply_release_hidden_in(&transaction, mbid, hidden)?;
    }
    transaction.commit()
}

/// One row's visibility, without a transaction of its own. The caller owns
/// the bracket -- `set_releases_hidden` opens exactly one for the whole batch,
/// and nesting `unchecked_transaction()` inside it would fail outright.
pub(crate) fn apply_release_hidden_in(
    conn: &Connection,
    release_group_mbid: &str,
    hidden: bool,
) -> Result<(), rusqlite::Error> {
    if !hidden {
        for mbid in
            crate::deleted_releases::forget_deleted_release_memory(conn, release_group_mbid)?
        {
            update_release_hidden_in(conn, &mbid, false)?;
        }
        return Ok(());
    }
    update_release_hidden_in(conn, release_group_mbid, true)
}

pub(crate) fn update_release_hidden_in(
    conn: &Connection,
    release_group_mbid: &str,
    hidden: bool,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE new_releases
            SET hidden = ?1,
                hidden_at = CASE WHEN ?1 = 1 THEN strftime('%s', 'now') ELSE NULL END,
                hidden_by_deleted_memory = 0
          WHERE release_group_mbid = ?2",
        rusqlite::params![i64::from(hidden), release_group_mbid],
    )?;
    Ok(())
}

pub(crate) fn hide_release_by_deleted_memory_in(
    conn: &Connection,
    release_group_mbid: &str,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE new_releases
            SET hidden = 1,
                hidden_at = strftime('%s', 'now'),
                hidden_by_deleted_memory = 1
          WHERE release_group_mbid = ?1 AND hidden = 0",
        [release_group_mbid],
    )? > 0)
}

pub(crate) fn unhide_release_by_deleted_memory_in(
    conn: &Connection,
    release_group_mbid: &str,
) -> Result<bool, rusqlite::Error> {
    Ok(conn.execute(
        "UPDATE new_releases
            SET hidden = 0,
                hidden_at = NULL,
                hidden_by_deleted_memory = 0
          WHERE release_group_mbid = ?1 AND hidden_by_deleted_memory = 1",
        [release_group_mbid],
    )? > 0)
}

pub fn mark_releases_seen(
    db: &crate::db::Db,
    release_group_mbids: &[String],
    seen_at: i64,
) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
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
    db: &crate::db::Db,
    artist_mbid: &str,
    today: NaiveDate,
) -> Result<Option<ArtistNews>, rusqlite::Error> {
    let conn = db.conn();
    query_artist_news_in(conn, artist_mbid, today)
}

fn query_artist_news_in(
    conn: &Connection,
    artist_mbid: &str,
    today: NaiveDate,
) -> Result<Option<ArtistNews>, rusqlite::Error> {
    let releases = query_releases_in(conn, false, today)?
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
    db: &crate::db::Db,
    artist: &str,
    today: NaiveDate,
) -> Result<Option<ArtistNews>, rusqlite::Error> {
    let conn = db.conn();
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
    query_artist_news_in(conn, &artist_mbid, today)
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
