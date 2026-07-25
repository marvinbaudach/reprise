//! The refresh pipeline that talks to MusicBrainz and writes fetched
//! releases to the database. Split out of `artist_news.rs` purely to stay
//! under the project's 800-line rule; re-exported from there so existing
//! callers keep using `artist_news::{refresh, RefreshReport, NewsError}`.

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::artist_news::{include_singles, normalize, AlbumNews, OWNED_ALBUM_MIN_TRACKS};
use crate::artist_news_candidates::{artists_for_fetch, ArtistCandidate, FetchScope};
use crate::artist_news_parsing::{
    artist_payload_valid, artist_search_url, parse_artist_mbid, parse_release_groups,
    release_groups_url, release_payload_valid, ArtistMatch,
};
use crate::musicbrainz::{self, FetchError};

const FETCH_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const DEFAULT_FALLBACK_ACCENT: &str = "#3584E4";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RefreshReport {
    pub artists_queued: usize,
    pub artists_fetched: usize,
    pub releases_upserted: usize,
    pub unmatched: usize,
    pub failed: usize,
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

fn database_error(error: rusqlite::Error) -> NewsError {
    let message = error.to_string();
    drop(error);
    NewsError::Database(message)
}
