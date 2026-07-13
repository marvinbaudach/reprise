//! Conservative, cached MusicBrainz album news for an explicitly selected
//! artist. This module is blocking and must be called from a worker thread.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::musicbrainz::{self, FetchError};

const CACHE_VERSION: u8 = 1;
const POSITIVE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const NEGATIVE_TTL_SECONDS: i64 = 24 * 60 * 60;
const MIN_ARTIST_SCORE: i64 = 95;
const NEWS_WINDOW_DAYS: i64 = 365;
const MAX_ITEMS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewsKind {
    Upcoming,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumNews {
    pub release_group_mbid: String,
    pub title: String,
    pub first_release_date: String,
    pub primary_type: String,
    pub kind: NewsKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistNews {
    pub artist: String,
    pub artist_mbid: String,
    pub fetched_at: i64,
    pub items: Vec<AlbumNews>,
    pub stale: bool,
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum CachedMatch {
    Found,
    Unmatched,
    Ambiguous,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheRecord {
    version: u8,
    artist: String,
    artist_mbid: Option<String>,
    fetched_at: i64,
    items: Vec<AlbumNews>,
    matched: CachedMatch,
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
        "https://musicbrainz.org/ws/2/release-group?artist={}&type=album%7Cep&release-group-status=website-default&limit=100&fmt=json",
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

pub fn load_or_refresh(
    artist: &str,
    local_albums: &[String],
    today: NaiveDate,
    force: bool,
) -> Result<ArtistNews, NewsError> {
    let now = chrono::Utc::now().timestamp();
    let cache_dir = cache_dir();
    load_or_refresh_with(
        artist,
        local_albums,
        today,
        force,
        now,
        &cache_dir,
        &mut musicbrainz::get,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn load_or_refresh_with<F>(
    artist: &str,
    local_albums: &[String],
    today: NaiveDate,
    force: bool,
    now: i64,
    cache_dir: &Path,
    fetch: &mut F,
) -> Result<ArtistNews, NewsError>
where
    F: FnMut(&str) -> Result<String, FetchError>,
{
    let artist = artist.trim();
    if artist.is_empty() {
        return Err(NewsError::Unmatched);
    }
    let cached = read_cache(cache_dir, artist);
    if !force {
        if let Some(record) = cached.as_ref().filter(|record| is_fresh(record, now)) {
            return cached_result(record, local_albums);
        }
    }

    let mbid = match cached
        .as_ref()
        .filter(|record| matches!(record.matched, CachedMatch::Found))
        .and_then(|record| record.artist_mbid.clone())
    {
        Some(mbid) => mbid,
        None => match fetch(&artist_search_url(artist)) {
            Ok(body) if artist_payload_valid(&body) => match parse_artist_mbid(&body, artist) {
                ArtistMatch::Found(mbid) => mbid,
                ArtistMatch::Ambiguous => {
                    write_negative(cache_dir, artist, now, CachedMatch::Ambiguous);
                    return Err(NewsError::Ambiguous);
                }
                ArtistMatch::NotFound => {
                    write_negative(cache_dir, artist, now, CachedMatch::Unmatched);
                    return Err(NewsError::Unmatched);
                }
            },
            Ok(_) => return stale_or(cached.as_ref(), local_albums, NewsError::InvalidResponse),
            Err(error) => return stale_or(cached.as_ref(), local_albums, error.into()),
        },
    };

    let body = match fetch(&release_groups_url(&mbid)) {
        Ok(body) if release_payload_valid(&body) => body,
        Ok(_) => return stale_or(cached.as_ref(), local_albums, NewsError::InvalidResponse),
        Err(error) => return stale_or(cached.as_ref(), local_albums, error.into()),
    };
    let items = parse_release_groups(&body, local_albums, today);
    let record = CacheRecord {
        version: CACHE_VERSION,
        artist: artist.to_string(),
        artist_mbid: Some(mbid.clone()),
        fetched_at: now,
        items: items.clone(),
        matched: CachedMatch::Found,
    };
    write_cache(cache_dir, artist, &record);
    Ok(ArtistNews {
        artist: artist.to_string(),
        artist_mbid: mbid,
        fetched_at: now,
        items,
        stale: false,
    })
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
    if !matches!(primary_type.to_ascii_lowercase().as_str(), "album" | "ep")
        || title.is_empty()
        || local.contains(&normalize(&title))
        || has_excluded_secondary_type(group)
    {
        return None;
    }
    let delta = release_date.signed_duration_since(today).num_days();
    let kind = if (0..=NEWS_WINDOW_DAYS).contains(&delta) {
        NewsKind::Upcoming
    } else if (-NEWS_WINDOW_DAYS..=-1).contains(&delta) {
        NewsKind::New
    } else {
        return None;
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

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("reprise/artist-news")
}

pub(crate) fn cache_file(cache_dir: &Path, artist: &str) -> PathBuf {
    let key = crate::cover::hash_hex(normalize(artist).as_bytes());
    cache_dir.join(format!("{key}.json"))
}

fn read_cache(cache_dir: &Path, artist: &str) -> Option<CacheRecord> {
    let body = std::fs::read(cache_file(cache_dir, artist)).ok()?;
    let record = serde_json::from_slice::<CacheRecord>(&body).ok()?;
    (record.version == CACHE_VERSION && normalize(&record.artist) == normalize(artist))
        .then_some(record)
}

fn is_fresh(record: &CacheRecord, now: i64) -> bool {
    let age = now.saturating_sub(record.fetched_at).max(0);
    let ttl = match record.matched {
        CachedMatch::Found => POSITIVE_TTL_SECONDS,
        CachedMatch::Unmatched | CachedMatch::Ambiguous => NEGATIVE_TTL_SECONDS,
    };
    age <= ttl
}

fn cached_result(record: &CacheRecord, local_albums: &[String]) -> Result<ArtistNews, NewsError> {
    match record.matched {
        CachedMatch::Unmatched => Err(NewsError::Unmatched),
        CachedMatch::Ambiguous => Err(NewsError::Ambiguous),
        CachedMatch::Found => {
            let local = local_albums
                .iter()
                .map(|album| normalize(album))
                .collect::<std::collections::HashSet<_>>();
            Ok(ArtistNews {
                artist: record.artist.clone(),
                artist_mbid: record.artist_mbid.clone().unwrap_or_default(),
                fetched_at: record.fetched_at,
                items: record
                    .items
                    .iter()
                    .filter(|item| !local.contains(&normalize(&item.title)))
                    .cloned()
                    .collect(),
                stale: false,
            })
        }
    }
}

fn stale_or(
    cached: Option<&CacheRecord>,
    local_albums: &[String],
    error: NewsError,
) -> Result<ArtistNews, NewsError> {
    let Some(record) = cached.filter(|record| matches!(record.matched, CachedMatch::Found)) else {
        return Err(error);
    };
    let mut news = cached_result(record, local_albums)?;
    news.stale = true;
    Ok(news)
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

fn write_negative(cache_dir: &Path, artist: &str, now: i64, matched: CachedMatch) {
    write_cache(
        cache_dir,
        artist,
        &CacheRecord {
            version: CACHE_VERSION,
            artist: artist.to_string(),
            artist_mbid: None,
            fetched_at: now,
            items: Vec::new(),
            matched,
        },
    );
}

fn write_cache(cache_dir: &Path, artist: &str, record: &CacheRecord) {
    let Ok(body) = serde_json::to_vec(record) else {
        return;
    };
    if std::fs::create_dir_all(cache_dir).is_err() {
        tracing::warn!("could not create Artist News cache directory");
        return;
    }
    let destination = cache_file(cache_dir, artist);
    let temporary = cache_dir.join(format!(".artist-news-{}.tmp", fastrand::u64(..)));
    if std::fs::write(&temporary, body).is_err()
        || std::fs::rename(&temporary, destination).is_err()
    {
        let _ = std::fs::remove_file(temporary);
        tracing::warn!("could not publish Artist News cache entry");
    }
}
