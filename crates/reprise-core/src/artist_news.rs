//! The single MusicBrainz-backed New Releases pipeline and its database query
//! layer. Network work is blocking and must be called from a worker thread.

use std::cmp::Ordering;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::musicbrainz::{self, FetchError};

const FETCH_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const MIN_ARTIST_SCORE: i64 = 95;
const NEWS_WINDOW_DAYS: i64 = 90;
const MAX_ITEMS: usize = 5;
const TOP_ARTIST_COUNT: usize = 20;
const DAILY_REST_COUNT: usize = 5;
const DEFAULT_FALLBACK_ACCENT: &str = "#3584E4";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewsKind {
    Upcoming,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumNews {
    pub release_group_mbid: String,
    pub title: String,
    pub first_release_date: String,
    pub primary_type: String,
    pub kind: NewsKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistNews {
    pub artist: String,
    pub artist_mbid: String,
    pub fetched_at: i64,
    pub items: Vec<AlbumNews>,
    pub stale: bool,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchScope {
    TopArtists,
    AllArtists { day_index: u64 },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RefreshReport {
    pub artists_queued: usize,
    pub artists_fetched: usize,
    pub releases_upserted: usize,
    pub unmatched: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtistCandidate {
    pub(crate) name: String,
    mbid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtistMatch {
    Found(String),
    Ambiguous,
    NotFound,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NewsError {
    #[error("artist could not be matched")]
    Unmatched,
    #[error("artist could not be matched unambiguously")]
    Ambiguous,
    #[error("MusicBrainz response was invalid")]
    InvalidResponse,
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("New Releases database operation failed: {0}")]
    Database(String),
}

pub fn artist_search_url(artist: &str) -> String {
    let escaped = artist.trim().replace('\\', "\\\\").replace('"', "\\\"");
    let query = format!("artist:\"{escaped}\"");
    format!(
        "https://musicbrainz.org/ws/2/artist/?query={}&fmt=json&limit=5",
        musicbrainz::urlencode(&query)
    )
}

pub fn release_groups_url(mbid: &str) -> String {
    format!(
        "https://musicbrainz.org/ws/2/release-group?artist={}&type=album%7Cep%7Csingle&release-group-status=website-default&limit=100&fmt=json",
        musicbrainz::urlencode(mbid)
    )
}

pub fn parse_artist_mbid(json: &str, artist: &str) -> ArtistMatch {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return ArtistMatch::NotFound;
    };
    let Some(artists) = value.get("artists").and_then(serde_json::Value::as_array) else {
        return ArtistMatch::NotFound;
    };
    let wanted = normalize(artist);
    let mut ids = artists
        .iter()
        .filter(|candidate| {
            candidate
                .get("score")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or_default()
                >= MIN_ARTIST_SCORE
        })
        .filter(|candidate| {
            candidate
                .get("name")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|name| normalize(name) == wanted)
        })
        .filter_map(|candidate| candidate.get("id").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    match ids.as_slice() {
        [id] => ArtistMatch::Found(id.clone()),
        [] => ArtistMatch::NotFound,
        _ => ArtistMatch::Ambiguous,
    }
}

pub fn parse_release_groups(
    json: &str,
    local_albums: &[String],
    today: NaiveDate,
) -> Vec<AlbumNews> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(groups) = value
        .get("release-groups")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let local = local_albums
        .iter()
        .map(|album| normalize(album))
        .collect::<std::collections::HashSet<_>>();
    let mut items = groups
        .iter()
        .filter_map(|group| parse_release_group(group, &local, today))
        .collect::<Vec<_>>();
    items.sort_by(|(left, left_date), (right, right_date)| {
        compare_news(left, *left_date, right, *right_date)
    });
    items.truncate(MAX_ITEMS);
    items.into_iter().map(|(item, _)| item).collect()
}

pub fn refresh(
    conn: &Connection,
    today: NaiveDate,
    scope: FetchScope,
    force: bool,
) -> Result<RefreshReport, NewsError> {
    refresh_with(
        conn,
        today,
        chrono::Utc::now().timestamp(),
        scope,
        force,
        &mut musicbrainz::get,
    )
}

pub(crate) fn refresh_with<F>(
    conn: &Connection,
    today: NaiveDate,
    now: i64,
    scope: FetchScope,
    force: bool,
    fetch: &mut F,
) -> Result<RefreshReport, NewsError>
where
    F: FnMut(&str) -> Result<String, FetchError>,
{
    let candidates = artists_for_fetch(conn, scope).map_err(database_error)?;
    let mut report = RefreshReport {
        artists_queued: candidates.len(),
        ..RefreshReport::default()
    };
    for candidate in candidates {
        let mbid = match resolve_artist_mbid(conn, &candidate, fetch, &mut report)? {
            Some(mbid) => mbid,
            None => continue,
        };
        if !force && artist_cache_is_fresh(conn, &mbid, now).map_err(database_error)? {
            continue;
        }
        let body = match fetch(&release_groups_url(&mbid)) {
            Ok(body) if release_payload_valid(&body) => body,
            Ok(_) | Err(_) => {
                report.failed += 1;
                continue;
            }
        };
        let local_albums = local_albums(conn, &candidate.name).map_err(database_error)?;
        let items = parse_release_groups(&body, &local_albums, today);
        upsert_releases(conn, &candidate.name, &mbid, now, &items).map_err(database_error)?;
        report.artists_fetched += 1;
        report.releases_upserted += items.len();
    }
    Ok(report)
}

pub(crate) fn artists_for_fetch(
    conn: &Connection,
    scope: FetchScope,
) -> Result<Vec<ArtistCandidate>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT MIN(trim(artist)), MAX(artist_mbid), SUM(play_count) AS plays
         FROM tracks
         WHERE removed_at IS NULL AND missing_since IS NULL AND trim(artist) <> ''
         GROUP BY lower(trim(artist))
         HAVING MAX(artist_mbid) IS NOT NULL OR MAX(artist_mbid_negative) = 0
         ORDER BY plays DESC, lower(MIN(trim(artist))) ASC",
    )?;
    let mut candidates = statement
        .query_map([], |row| {
            Ok(ArtistCandidate {
                name: row.get(0)?,
                mbid: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if candidates.len() <= TOP_ARTIST_COUNT {
        return Ok(candidates);
    }
    match scope {
        FetchScope::TopArtists => {
            candidates.truncate(TOP_ARTIST_COUNT);
            Ok(candidates)
        }
        FetchScope::AllArtists { day_index } => {
            let rest_len = candidates.len() - TOP_ARTIST_COUNT;
            let start = ((day_index as usize).saturating_mul(DAILY_REST_COUNT) % rest_len)
                + TOP_ARTIST_COUNT;
            let end = (start + DAILY_REST_COUNT).min(candidates.len());
            let daily = candidates[start..end].to_vec();
            candidates.truncate(TOP_ARTIST_COUNT);
            candidates.extend(daily);
            Ok(candidates)
        }
    }
}

fn resolve_artist_mbid<F>(
    conn: &Connection,
    candidate: &ArtistCandidate,
    fetch: &mut F,
    report: &mut RefreshReport,
) -> Result<Option<String>, NewsError>
where
    F: FnMut(&str) -> Result<String, FetchError>,
{
    if let Some(mbid) = candidate.mbid.clone() {
        return Ok(Some(mbid));
    }
    let body = match fetch(&artist_search_url(&candidate.name)) {
        Ok(body) if artist_payload_valid(&body) => body,
        Ok(_) | Err(_) => {
            report.failed += 1;
            return Ok(None);
        }
    };
    match parse_artist_mbid(&body, &candidate.name) {
        ArtistMatch::Found(mbid) => {
            persist_artist_match(conn, &candidate.name, Some(&mbid), false)
                .map_err(database_error)?;
            Ok(Some(mbid))
        }
        ArtistMatch::Ambiguous | ArtistMatch::NotFound => {
            persist_artist_match(conn, &candidate.name, None, true).map_err(database_error)?;
            report.unmatched += 1;
            Ok(None)
        }
    }
}

fn persist_artist_match(
    conn: &Connection,
    artist: &str,
    mbid: Option<&str>,
    negative: bool,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE tracks
         SET artist_mbid = ?1, artist_mbid_negative = ?2
         WHERE lower(trim(artist)) = lower(trim(?3))",
        rusqlite::params![mbid, i64::from(negative), artist],
    )?;
    Ok(())
}

fn artist_cache_is_fresh(
    conn: &Connection,
    artist_mbid: &str,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    let fetched_at = conn.query_row(
        "SELECT MAX(fetched_at) FROM new_releases WHERE artist_mbid = ?1",
        [artist_mbid],
        |row| row.get::<_, Option<i64>>(0),
    )?;
    Ok(fetched_at
        .is_some_and(|fetched_at| now.saturating_sub(fetched_at).max(0) <= FETCH_TTL_SECONDS))
}

fn local_albums(conn: &Connection, artist: &str) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT DISTINCT album FROM tracks
         WHERE lower(trim(artist)) = lower(trim(?1)) AND trim(album) <> ''",
    )?;
    let albums = statement
        .query_map([artist], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(albums)
}

fn upsert_releases(
    conn: &Connection,
    artist: &str,
    artist_mbid: &str,
    fetched_at: i64,
    items: &[AlbumNews],
) -> Result<(), rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    for item in items {
        transaction.execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, fallback_accent
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(release_group_mbid) DO UPDATE SET
               artist_name = excluded.artist_name,
               artist_mbid = excluded.artist_mbid,
               title = excluded.title,
               release_type = excluded.release_type,
               first_release_date = excluded.first_release_date,
               fetched_at = excluded.fetched_at",
            rusqlite::params![
                item.release_group_mbid,
                artist,
                artist_mbid,
                item.title,
                item.primary_type,
                item.first_release_date,
                fetched_at,
                DEFAULT_FALLBACK_ACCENT,
            ],
        )?;
    }
    transaction.commit()
}

pub fn query_releases(
    conn: &Connection,
    include_hidden: bool,
    today: NaiveDate,
) -> Result<Vec<StoredRelease>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT release_group_mbid, artist_name, artist_mbid, title, release_type,
                first_release_date, fetched_at, seen_at, hidden, fallback_accent
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
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut local_statement = conn.prepare(
        "SELECT artist, album FROM tracks
         WHERE removed_at IS NULL AND missing_since IS NULL AND trim(album) <> ''",
    )?;
    let local_albums = local_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .map(|row| row.map(|(artist, album)| (normalize(&artist), normalize(&album))))
        .collect::<Result<std::collections::HashSet<_>, _>>()?;
    releases.retain(|release| {
        !local_albums.contains(&(normalize(&release.artist_name), normalize(&release.title)))
    });
    releases.sort_by(|left, right| compare_stored_releases(left, right, today));
    Ok(releases)
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

fn database_error(error: rusqlite::Error) -> NewsError {
    let message = error.to_string();
    drop(error);
    NewsError::Database(message)
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

fn parse_release_group(
    group: &serde_json::Value,
    local: &std::collections::HashSet<String>,
    today: NaiveDate,
) -> Option<(AlbumNews, NaiveDate)> {
    let mbid = group.get("id")?.as_str()?.to_string();
    let title = group.get("title")?.as_str()?.trim().to_string();
    let date_text = group.get("first-release-date")?.as_str()?.to_string();
    let release_date = parse_partial_date(&date_text)?;
    let primary_type = group.get("primary-type")?.as_str()?.to_string();
    let primary_type_normalized = primary_type.to_ascii_lowercase();
    if !matches!(primary_type_normalized.as_str(), "album" | "ep" | "single")
        || title.is_empty()
        || local.contains(&normalize(&title))
        || has_excluded_secondary_type(group)
    {
        return None;
    }
    let delta = release_date.signed_duration_since(today).num_days();
    let kind = match primary_type_normalized.as_str() {
        "single" if date_text.len() == 10 && delta > 0 => NewsKind::Upcoming,
        "single" => return None,
        _ if delta >= 0 => NewsKind::Upcoming,
        _ if delta >= -NEWS_WINDOW_DAYS => NewsKind::New,
        _ => return None,
    };
    Some((
        AlbumNews {
            release_group_mbid: mbid,
            title,
            first_release_date: date_text,
            primary_type,
            kind,
        },
        release_date,
    ))
}

fn has_excluded_secondary_type(group: &serde_json::Value) -> bool {
    const EXCLUDED: &[&str] = &[
        "compilation",
        "live",
        "remix",
        "soundtrack",
        "mixtape/street",
        "dj-mix",
    ];
    group
        .get("secondary-types")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|types| {
            types.iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|kind| EXCLUDED.contains(&kind.to_ascii_lowercase().as_str()))
            })
        })
}

fn parse_partial_date(value: &str) -> Option<NaiveDate> {
    match value.len() {
        10 => NaiveDate::parse_from_str(value, "%Y-%m-%d").ok(),
        7 => NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d").ok(),
        4 => NaiveDate::parse_from_str(&format!("{value}-01-01"), "%Y-%m-%d").ok(),
        _ => None,
    }
}

fn compare_news(
    left: &AlbumNews,
    left_date: NaiveDate,
    right: &AlbumNews,
    right_date: NaiveDate,
) -> Ordering {
    match (left.kind, right.kind) {
        (NewsKind::Upcoming, NewsKind::New) => Ordering::Less,
        (NewsKind::New, NewsKind::Upcoming) => Ordering::Greater,
        (NewsKind::Upcoming, NewsKind::Upcoming) => left_date.cmp(&right_date),
        (NewsKind::New, NewsKind::New) => right_date.cmp(&left_date),
    }
    .then_with(|| left.title.cmp(&right.title))
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn artist_payload_valid(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("artists").cloned())
        .is_some_and(|artists| artists.is_array())
}

fn release_payload_valid(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("release-groups").cloned())
        .is_some_and(|groups| groups.is_array())
}
