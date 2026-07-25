//! The single MusicBrainz-backed New Releases pipeline and its database query
//! layer. Network work is blocking and must be called from a worker thread.

use std::cmp::Ordering;
use std::path::PathBuf;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::musicbrainz::{self, FetchError};

const FETCH_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const MIN_ARTIST_SCORE: i64 = 95;
const NEWS_WINDOW_DAYS: i64 = 90;
/// How many tracks of an album must be present before the album counts as
/// owned. One track is a single, not an album — treating it as ownership is
/// what used to suppress the very album the single announces.
const OWNED_ALBUM_MIN_TRACKS: i64 = 2;
const MAX_ITEMS: usize = 20;
const TOP_ARTIST_COUNT: usize = 20;
const REST_ARTISTS_PER_RUN: usize = 30;
const DEFAULT_FALLBACK_ACCENT: &str = "#3584E4";
const FETCH_ALL_ARTISTS_KEY: &str = "module.new_releases.all_artists";
const INCLUDE_SINGLES_KEY: &str = "module.new_releases.include_singles";

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
    pub announce_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistNews {
    pub artist: String,
    pub artist_mbid: String,
    pub fetched_at: i64,
    pub items: Vec<AlbumNews>,
    pub stale: bool,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchScope {
    TopArtists,
    AllArtists,
}

pub fn configured_fetch_scope(conn: &Connection) -> Result<FetchScope, rusqlite::Error> {
    if crate::library::settings::get_bool(conn, FETCH_ALL_ARTISTS_KEY, false)? {
        Ok(FetchScope::AllArtists)
    } else {
        Ok(FetchScope::TopArtists)
    }
}

pub fn set_fetch_all_artists(conn: &Connection, all_artists: bool) -> Result<(), rusqlite::Error> {
    crate::library::settings::set_bool(conn, FETCH_ALL_ARTISTS_KEY, all_artists)
}

/// Whether already-released singles count as news. Off by default: singles
/// are the most common release type, so switching this on noticeably
/// increases how much the badge reports.
pub fn include_singles(conn: &Connection) -> Result<bool, rusqlite::Error> {
    crate::library::settings::get_bool(conn, INCLUDE_SINGLES_KEY, false)
}

pub fn set_include_singles(conn: &Connection, include: bool) -> Result<(), rusqlite::Error> {
    crate::library::settings::set_bool(conn, INCLUDE_SINGLES_KEY, include)
}

/// Staleness policy (when a refresh is due, the per-install jitter, and the
/// latest fetch timestamp) lives in `artist_news_refresh`; re-exported here
/// so existing callers keep using `artist_news::{refresh_due, jitter_seconds,
/// latest_fetched_at}`.
pub use crate::artist_news_refresh::{jitter_seconds, latest_fetched_at, refresh_due};

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

/// Outcome of resolving an artist's MBID for a refresh attempt. Distinct from
/// `ArtistMatch` (which only describes what a *successful* search response
/// contained): this also carries the "the search request itself failed or
/// came back invalid" case, which `ArtistMatch`/`Option<String>` cannot
/// express and which the ledger must record as `failed`, not `unmatched`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MbidResolution {
    Found(String),
    /// The artist-search request failed or returned an invalid payload.
    Failed,
    /// The search succeeded but matched nothing, or matched ambiguously.
    Unmatched,
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
        "https://musicbrainz.org/ws/2/release-group?artist={}&type=album%7Cep%7Csingle&release-group-status=website-default&limit=100&inc=url-rels&fmt=json",
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
    include_singles: bool,
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
        .filter_map(|group| parse_release_group(group, &local, today, include_singles))
        .collect::<Vec<_>>();
    items.sort_by(|(left, left_date), (right, right_date)| {
        compare_news(left, *left_date, right, *right_date)
    });
    items.truncate(MAX_ITEMS);
    items.into_iter().map(|(item, _)| item).collect()
}

pub fn refresh<A>(
    conn: &Connection,
    today: NaiveDate,
    scope: FetchScope,
    force: bool,
    mut fallback_accent: A,
) -> Result<RefreshReport, NewsError>
where
    A: FnMut(&Connection, &str) -> Option<String>,
{
    refresh_with(
        conn,
        today,
        chrono::Utc::now().timestamp(),
        scope,
        force,
        &mut musicbrainz::get,
        &mut fallback_accent,
    )
}

pub(crate) fn refresh_with<F, A>(
    conn: &Connection,
    today: NaiveDate,
    now: i64,
    scope: FetchScope,
    force: bool,
    fetch: &mut F,
    fallback_accent: &mut A,
) -> Result<RefreshReport, NewsError>
where
    F: FnMut(&str) -> Result<String, FetchError>,
    A: FnMut(&Connection, &str) -> Option<String>,
{
    let candidates = artists_for_fetch(conn, scope).map_err(database_error)?;
    let mut report = RefreshReport {
        artists_queued: candidates.len(),
        ..RefreshReport::default()
    };
    let include_singles = include_singles(conn).map_err(database_error)?;
    for candidate in candidates {
        // `normalize()` is the authoritative form of the ledger key: every
        // runtime read and write (`record_attempt`, `last_attempt_at`,
        // `artist_cache_is_fresh`) goes through it, and it collapses *inner*
        // whitespace runs via `split_whitespace` in addition to trimming and
        // lowercasing. The migration's SQL backfill
        // (`db_artist_news_fetch.rs`) seeded the same table with
        // `lower(trim(artist_name))` instead, which SQLite cannot make
        // collapse inner runs generically. So "Pink   Floyd" backfills as
        // "pink   floyd" but normalizes here to "pink floyd" — the keys
        // differ and the backfilled row is never matched. The practical
        // effect is that such an artist is treated as "never checked" once,
        // costs one extra fetch, and then the runtime key is what every
        // later run reads and writes, so the mismatch cannot recur. That
        // one-time, self-healing cost for a rare edge case (multiple inner
        // spaces in an artist name) is why this divergence is accepted
        // rather than chasing exact parity in raw SQL.
        let artist_key = normalize(&candidate.name);
        // Checked before resolving the MBID: a fresh artist must cost zero
        // requests, and the search request would otherwise be spent before
        // we ever consult the cache.
        if !force && artist_cache_is_fresh(conn, &artist_key, now).map_err(database_error)? {
            continue;
        }
        let mbid = match resolve_artist_mbid(conn, &candidate, fetch, &mut report)? {
            MbidResolution::Found(mbid) => mbid,
            MbidResolution::Failed => {
                crate::artist_news_ledger::record_attempt(
                    conn,
                    &artist_key,
                    None,
                    now,
                    crate::artist_news_ledger::FetchOutcome::Failed,
                    0,
                )
                .map_err(database_error)?;
                continue;
            }
            MbidResolution::Unmatched => {
                crate::artist_news_ledger::record_attempt(
                    conn,
                    &artist_key,
                    None,
                    now,
                    crate::artist_news_ledger::FetchOutcome::Unmatched,
                    0,
                )
                .map_err(database_error)?;
                continue;
            }
        };
        let body = match fetch(&release_groups_url(&mbid)) {
            Ok(body) if release_payload_valid(&body) => body,
            Ok(_) | Err(_) => {
                report.failed += 1;
                crate::artist_news_ledger::record_attempt(
                    conn,
                    &artist_key,
                    Some(&mbid),
                    now,
                    crate::artist_news_ledger::FetchOutcome::Failed,
                    0,
                )
                .map_err(database_error)?;
                continue;
            }
        };
        let local_albums = local_albums(conn, &candidate.name).map_err(database_error)?;
        let items = parse_release_groups(&body, &local_albums, today, include_singles);
        let accent = normalize_fallback_accent(fallback_accent(conn, &candidate.name));
        upsert_releases(conn, &candidate.name, &mbid, now, &accent, &items)
            .map_err(database_error)?;
        crate::artist_news_ledger::record_attempt(
            conn,
            &artist_key,
            Some(&mbid),
            now,
            crate::artist_news_ledger::FetchOutcome::Ok,
            items.len(),
        )
        .map_err(database_error)?;
        report.artists_fetched += 1;
        report.releases_upserted += items.len();
    }
    crate::artist_news_history::enforce_retention(conn, now).map_err(database_error)?;
    Ok(report)
}

/// Candidates for this run: the `TOP_ARTIST_COUNT` most-played artists
/// always, plus — in `AllArtists` scope — the `REST_ARTISTS_PER_RUN` artists
/// that have gone longest without an attempt, never-checked ones first.
///
/// Ordering the tail by staleness rather than by a date-derived rotation
/// window is what lets an artist you own a single track of ever come up at
/// all: play count decides who is *preferred*, not who is *reachable*. A run
/// that never happens costs nothing now — the skipped artists are simply the
/// oldest next time.
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
        FetchScope::AllArtists => {
            let mut rest = candidates.split_off(TOP_ARTIST_COUNT);
            let mut keyed = Vec::with_capacity(rest.len());
            for candidate in rest.drain(..) {
                let last_attempt =
                    crate::artist_news_ledger::last_attempt_at(conn, &normalize(&candidate.name))?;
                keyed.push((last_attempt, candidate));
            }
            // `None` sorts before `Some` — never-checked artists come first.
            keyed.sort_by_key(|(last_attempt, _)| *last_attempt);
            candidates.extend(
                keyed
                    .into_iter()
                    .take(REST_ARTISTS_PER_RUN)
                    .map(|(_, candidate)| candidate),
            );
            Ok(candidates)
        }
    }
}

fn resolve_artist_mbid<F>(
    conn: &Connection,
    candidate: &ArtistCandidate,
    fetch: &mut F,
    report: &mut RefreshReport,
) -> Result<MbidResolution, NewsError>
where
    F: FnMut(&str) -> Result<String, FetchError>,
{
    if let Some(mbid) = candidate.mbid.clone() {
        return Ok(MbidResolution::Found(mbid));
    }
    let body = match fetch(&artist_search_url(&candidate.name)) {
        Ok(body) if artist_payload_valid(&body) => body,
        Ok(_) | Err(_) => {
            report.failed += 1;
            return Ok(MbidResolution::Failed);
        }
    };
    match parse_artist_mbid(&body, &candidate.name) {
        ArtistMatch::Found(mbid) => {
            persist_artist_match(conn, &candidate.name, Some(&mbid), false)
                .map_err(database_error)?;
            Ok(MbidResolution::Found(mbid))
        }
        ArtistMatch::Ambiguous | ArtistMatch::NotFound => {
            persist_artist_match(conn, &candidate.name, None, true).map_err(database_error)?;
            report.unmatched += 1;
            Ok(MbidResolution::Unmatched)
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

/// Freshness is judged by the last *attempt* recorded in the ledger, not by
/// the newest release we happened to store. An artist with nothing to report
/// stores no release — judging by releases meant re-fetching them forever.
fn artist_cache_is_fresh(
    conn: &Connection,
    artist_key: &str,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    let last_attempt = crate::artist_news_ledger::last_attempt_at(conn, artist_key)?;
    Ok(last_attempt.is_some_and(|attempt| now.saturating_sub(attempt).max(0) <= FETCH_TTL_SECONDS))
}

fn local_albums(conn: &Connection, artist: &str) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT album FROM tracks
         WHERE lower(trim(artist)) = lower(trim(?1)) AND trim(album) <> ''
           AND removed_at IS NULL AND missing_since IS NULL
         GROUP BY lower(trim(album))
         HAVING COUNT(*) >= ?2",
    )?;
    let albums = statement
        .query_map(rusqlite::params![artist, OWNED_ALBUM_MIN_TRACKS], |row| {
            row.get(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(albums)
}

#[cfg(test)]
pub(crate) fn local_albums_for_test(
    conn: &Connection,
    artist: &str,
) -> Result<Vec<String>, rusqlite::Error> {
    local_albums(conn, artist)
}

fn upsert_releases(
    conn: &Connection,
    artist: &str,
    artist_mbid: &str,
    fetched_at: i64,
    fallback_accent: &str,
    items: &[AlbumNews],
) -> Result<(), rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    for item in items {
        transaction.execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, fallback_accent, first_seen, announce_url
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?7, ?9)
             ON CONFLICT(release_group_mbid) DO UPDATE SET
               artist_name = excluded.artist_name,
               artist_mbid = excluded.artist_mbid,
               title = excluded.title,
               release_type = excluded.release_type,
               first_release_date = excluded.first_release_date,
               fetched_at = excluded.fetched_at,
               announce_url = COALESCE(excluded.announce_url, new_releases.announce_url)",
            rusqlite::params![
                item.release_group_mbid,
                artist,
                artist_mbid,
                item.title,
                item.primary_type,
                item.first_release_date,
                fetched_at,
                fallback_accent,
                item.announce_url,
            ],
        )?;
    }
    transaction.commit()
}

fn normalize_fallback_accent(accent: Option<String>) -> String {
    accent
        .filter(|value| {
            value.len() == 7
                && value.starts_with('#')
                && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .map_or_else(
            || DEFAULT_FALLBACK_ACCENT.to_string(),
            |value| value.to_ascii_uppercase(),
        )
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

/// Maps a track count onto the presence states. `OWNED_ALBUM_MIN_TRACKS` is
/// the same threshold `local_albums` filters by, so "counts as owned" means
/// the same thing on both sides.
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
    conn.query_row(
        "SELECT COUNT(*) FROM new_releases WHERE seen_at IS NULL",
        [],
        |row| row.get(0),
    )
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
    include_singles: bool,
) -> Option<(AlbumNews, NaiveDate)> {
    let mbid = group.get("id")?.as_str()?.to_string();
    let title = group.get("title")?.as_str()?.trim().to_string();
    let date_text = group.get("first-release-date")?.as_str()?.to_string();
    let release_date = parse_partial_date(&date_text)?;
    let primary_type = group.get("primary-type")?.as_str()?.to_string();
    let primary_type_normalized = primary_type.to_ascii_lowercase();
    if !matches!(primary_type_normalized.as_str(), "album" | "ep" | "single")
        || title.is_empty()
        || has_excluded_secondary_type(group)
    {
        return None;
    }
    let delta = release_date.signed_duration_since(today).num_days();
    let kind = match primary_type_normalized.as_str() {
        // An announced single needs an exact date to be trustworthy; that
        // rule predates the switch and stays on unconditionally, so turning
        // the switch off never shows *less* than before.
        "single" if date_text.len() == 10 && delta > 0 => NewsKind::Upcoming,
        "single" if !include_singles => return None,
        "single" if delta >= -NEWS_WINDOW_DAYS => NewsKind::New,
        "single" => return None,
        _ if delta >= 0 => NewsKind::Upcoming,
        _ if delta >= -NEWS_WINDOW_DAYS => NewsKind::New,
        _ => return None,
    };
    // An unreleased album cannot be owned. A title match here is by
    // definition a mis-tagged pre-release track — typically the lead single
    // tagged with the forthcoming album's name — so the library check is
    // skipped outright rather than merely relaxed.
    if kind == NewsKind::New && local.contains(&normalize(&title)) {
        return None;
    }
    Some((
        AlbumNews {
            release_group_mbid: mbid,
            title,
            first_release_date: date_text,
            primary_type,
            kind,
            announce_url: crate::artist_news_links::parse_announce_url(group),
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

pub(crate) fn parse_partial_date(value: &str) -> Option<NaiveDate> {
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

pub(crate) fn normalize(value: &str) -> String {
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
