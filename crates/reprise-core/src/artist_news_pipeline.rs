//! The refresh pipeline that talks to MusicBrainz and writes fetched
//! releases to the database. Split out of `artist_news.rs` purely to stay
//! under the project's 800-line rule; re-exported from there so existing
//! callers keep using `artist_news::{refresh, RefreshReport, NewsError}`.

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension};

use crate::artist_news::{normalize, AlbumNews};
use crate::artist_news_candidates::{artists_for_fetch, ArtistCandidate, FetchScope};
use crate::artist_news_parsing::{
    artist_payload_valid, artist_search_url, parse_artist_mbid,
    parse_release_group_page_for_primary_artist, parse_release_track_count,
    release_group_detail_url, release_groups_page_url, sort_release_groups, ArtistMatch,
};
use crate::musicbrainz::{self, FetchError};
use crate::source_error::{SourceError, SourceErrorKind};

const FETCH_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshReport {
    pub artists_queued: usize,
    pub artists_fetched: usize,
    pub releases_upserted: usize,
    pub unmatched: usize,
    pub failed: usize,
    pub failures: Vec<SourceError>,
}

/// Completed artist candidates within one bounded New Releases refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshProgress {
    /// Candidates whose refresh attempt or cache check has finished.
    pub checked: usize,
    /// Candidates selected for this refresh run.
    pub total: usize,
}

pub(crate) struct RefreshHooks<'a, F, P, C> {
    pub fetch: &'a mut F,
    pub on_progress: &'a mut P,
    pub completion_time: &'a mut C,
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
    Failed(SourceError),
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

impl NewsError {
    #[must_use]
    pub fn into_source_error(self) -> SourceError {
        match self {
            Self::Fetch(error) => source_error_for_fetch(&error),
            error => SourceError::new(
                SourceErrorKind::Unreachable,
                "New Releases refresh failed",
                error.to_string(),
            ),
        }
    }
}

pub fn refresh(
    db: &crate::db::Db,
    today: NaiveDate,
    scope: FetchScope,
    force: bool,
) -> Result<RefreshReport, NewsError> {
    refresh_with_progress(db, today, scope, force, |_| {})
}

pub fn refresh_with_progress<P>(
    db: &crate::db::Db,
    today: NaiveDate,
    scope: FetchScope,
    force: bool,
    mut on_progress: P,
) -> Result<RefreshReport, NewsError>
where
    P: FnMut(RefreshProgress),
{
    let started_at = chrono::Utc::now().timestamp();
    let mut completion_time = || chrono::Utc::now().timestamp();
    refresh_with_progress_at(
        db,
        today,
        started_at,
        scope,
        force,
        &mut RefreshHooks {
            fetch: &mut musicbrainz::get,
            on_progress: &mut on_progress,
            completion_time: &mut completion_time,
        },
    )
}

#[cfg(test)]
pub(crate) fn refresh_with<F>(
    db: &crate::db::Db,
    today: NaiveDate,
    now: i64,
    scope: FetchScope,
    force: bool,
    fetch: &mut F,
) -> Result<RefreshReport, NewsError>
where
    F: FnMut(&str) -> Result<String, FetchError>,
{
    let mut on_progress = |_| {};
    let mut completion_time = || now;
    refresh_with_progress_at(
        db,
        today,
        now,
        scope,
        force,
        &mut RefreshHooks {
            fetch,
            on_progress: &mut on_progress,
            completion_time: &mut completion_time,
        },
    )
}

pub(crate) fn refresh_with_progress_at<F, P, C>(
    db: &crate::db::Db,
    today: NaiveDate,
    now: i64,
    scope: FetchScope,
    force: bool,
    hooks: &mut RefreshHooks<'_, F, P, C>,
) -> Result<RefreshReport, NewsError>
where
    F: FnMut(&str) -> Result<String, FetchError>,
    P: FnMut(RefreshProgress),
    C: FnMut() -> i64,
{
    let conn = db.conn();
    crate::artist_news_refresh::seed_completion_from_legacy_ledger(db).map_err(database_error)?;
    let candidates = artists_for_fetch(conn, scope).map_err(database_error)?;
    let total = candidates.len();
    let mut report = RefreshReport {
        artists_queued: total,
        ..RefreshReport::default()
    };
    (hooks.on_progress)(RefreshProgress { checked: 0, total });
    let refresh_result = (|| -> Result<(), NewsError> {
        let local_track_counts =
            crate::artist_news_query::local_album_track_counts(conn).map_err(database_error)?;
        for (index, candidate) in candidates.into_iter().enumerate() {
            let completed = RefreshProgress {
                checked: index + 1,
                total,
            };
            'candidate: {
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
                if !force
                    && artist_cache_is_fresh(conn, &artist_key, now).map_err(database_error)?
                {
                    break 'candidate;
                }
                let mbid = match resolve_artist_mbid(conn, &candidate, hooks.fetch, &mut report)? {
                    MbidResolution::Found(mbid) => mbid,
                    MbidResolution::Failed(error) => {
                        tracing::warn!(
                            artist = %candidate.name,
                            %error,
                            "New Releases: artist check failed"
                        );
                        record_failure(&mut report, error);
                        crate::artist_news_ledger::record_attempt(
                            conn,
                            &artist_key,
                            None,
                            now,
                            crate::artist_news_ledger::FetchOutcome::Failed,
                            0,
                        )
                        .map_err(database_error)?;
                        break 'candidate;
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
                        break 'candidate;
                    }
                };
                let discography = match fetch_release_discography(&mbid, today, hooks.fetch) {
                    Ok(discography) => discography,
                    Err(error) => {
                        tracing::warn!(
                            artist = %candidate.name,
                            %error,
                            "New Releases: artist check failed"
                        );
                        record_failure(&mut report, error);
                        crate::artist_news_ledger::record_attempt(
                            conn,
                            &artist_key,
                            Some(&mbid),
                            now,
                            crate::artist_news_ledger::FetchOutcome::Failed,
                            0,
                        )
                        .map_err(database_error)?;
                        break 'candidate;
                    }
                };
                sync_releases(
                    conn,
                    &candidate.name,
                    &mbid,
                    now,
                    &discography.items,
                    &discography.excluded_release_group_mbids,
                )
                .map_err(database_error)?;
                enrich_local_release_track_counts(
                    conn,
                    &candidate.name,
                    &discography.items,
                    &local_track_counts,
                    hooks.fetch,
                )
                .map_err(database_error)?;
                crate::artist_news_ledger::record_attempt(
                    conn,
                    &artist_key,
                    Some(&mbid),
                    now,
                    crate::artist_news_ledger::FetchOutcome::Ok,
                    discography.items.len(),
                )
                .map_err(database_error)?;
                report.artists_fetched += 1;
                report.releases_upserted += discography.items.len();
            }
            (hooks.on_progress)(completed);
        }
        Ok(())
    })();
    let reconciliation_result = (|| -> Result<(), NewsError> {
        let transaction = conn.unchecked_transaction().map_err(database_error)?;
        crate::deleted_releases::apply_deleted_release_memory(&transaction)
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)
    })();
    refresh_result?;
    reconciliation_result?;
    crate::artist_news_history::enforce_retention(db, now).map_err(database_error)?;
    if report.failures.is_empty() || report.artists_fetched > 0 {
        crate::library::settings::set_new_releases_last_completed_at(db, (hooks.completion_time)())
            .map_err(database_error)?;
    }
    tracing::info!(
        queued = report.artists_queued,
        fetched = report.artists_fetched,
        unmatched = report.unmatched,
        failed = report.failed,
        "New Releases: check finished"
    );
    Ok(report)
}

struct FetchedDiscography {
    items: Vec<AlbumNews>,
    excluded_release_group_mbids: Vec<String>,
}

fn fetch_release_discography<F>(
    artist_mbid: &str,
    today: NaiveDate,
    fetch: &mut F,
) -> Result<FetchedDiscography, SourceError>
where
    F: FnMut(&str) -> Result<String, FetchError>,
{
    let mut offset = 0;
    let mut items = Vec::new();
    let mut excluded_release_group_mbids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    loop {
        let body = fetch(&release_groups_page_url(artist_mbid, offset))
            .map_err(|error| source_error_for_fetch(&error))?;
        let page = parse_release_group_page_for_primary_artist(&body, today, artist_mbid)
            .ok_or_else(invalid_response_source_error)?;
        excluded_release_group_mbids.extend(page.excluded_release_group_mbids);
        items.extend(
            page.page
                .items
                .into_iter()
                .filter(|item| seen.insert(item.release_group_mbid.clone())),
        );
        let Some(next_offset) = page.page.next_offset else {
            break;
        };
        if next_offset <= offset {
            return Err(invalid_response_source_error());
        }
        offset = next_offset;
    }
    sort_release_groups(&mut items);
    Ok(FetchedDiscography {
        items,
        excluded_release_group_mbids,
    })
}

fn enrich_local_release_track_counts<F>(
    conn: &Connection,
    artist: &str,
    items: &[AlbumNews],
    local_track_counts: &std::collections::HashMap<(String, String), i64>,
    fetch: &mut F,
) -> Result<(), rusqlite::Error>
where
    F: FnMut(&str) -> Result<String, FetchError>,
{
    for item in items {
        if !matches!(
            item.primary_type.to_ascii_lowercase().as_str(),
            "album" | "ep"
        ) || crate::artist_news_query::local_track_count(local_track_counts, artist, &item.title)
            == 0
            || stored_track_count(conn, &item.release_group_mbid)?.is_some()
        {
            continue;
        }
        let Ok(body) = fetch(&release_group_detail_url(&item.release_group_mbid)) else {
            continue;
        };
        let Some(track_count) = parse_release_track_count(&body) else {
            continue;
        };
        conn.execute(
            "UPDATE new_releases SET track_count = ?1 WHERE release_group_mbid = ?2",
            rusqlite::params![track_count, item.release_group_mbid],
        )?;
    }
    Ok(())
}

fn stored_track_count(
    conn: &Connection,
    release_group_mbid: &str,
) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT track_count FROM new_releases WHERE release_group_mbid = ?1",
        [release_group_mbid],
        |row| row.get(0),
    )
    .optional()
    .map(Option::flatten)
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
        Ok(_) => return Ok(MbidResolution::Failed(invalid_response_source_error())),
        Err(error) => {
            return Ok(MbidResolution::Failed(source_error_for_fetch(&error)));
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

fn record_failure(report: &mut RefreshReport, error: SourceError) {
    report.failed += 1;
    report.failures.push(error);
}

fn source_error_for_fetch(error: &FetchError) -> SourceError {
    let kind = match error {
        FetchError::HttpStatus(429) => SourceErrorKind::RateLimited { retry_after: None },
        FetchError::Timeout
        | FetchError::Transport
        | FetchError::HttpStatus(_)
        | FetchError::Body
        | FetchError::BodyTooLarge => SourceErrorKind::Unreachable,
    };
    SourceError::new(kind, "New Releases fetch failed", error.to_string())
}

fn invalid_response_source_error() -> SourceError {
    SourceError::new(
        SourceErrorKind::Unreachable,
        "New Releases fetch failed",
        "MusicBrainz response was invalid",
    )
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

/// Freshness is judged by the last successful or unmatched attempt recorded
/// in the ledger, not by the newest release we happened to store. An artist
/// with nothing to report stores no release, while a failed artist must be due
/// again at the next check.
fn artist_cache_is_fresh(
    conn: &Connection,
    artist_key: &str,
    now: i64,
) -> Result<bool, rusqlite::Error> {
    let last_attempt = crate::artist_news_ledger::last_attempt(conn, artist_key)?;
    Ok(last_attempt.is_some_and(|attempt| {
        matches!(
            attempt.outcome,
            crate::artist_news_ledger::FetchOutcome::Ok
                | crate::artist_news_ledger::FetchOutcome::Unmatched
        ) && now.saturating_sub(attempt.at).max(0) <= FETCH_TTL_SECONDS
    }))
}

fn sync_releases(
    conn: &Connection,
    artist: &str,
    artist_mbid: &str,
    fetched_at: i64,
    items: &[AlbumNews],
    excluded_release_group_mbids: &[String],
) -> Result<(), rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    for release_group_mbid in excluded_release_group_mbids {
        transaction.execute(
            "DELETE FROM new_releases
             WHERE release_group_mbid = ?1 AND artist_mbid = ?2",
            rusqlite::params![release_group_mbid, artist_mbid],
        )?;
    }
    for item in items {
        transaction.execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, first_seen, announce_url
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)
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
                item.announce_url,
            ],
        )?;
    }
    let release_group_mbids = items
        .iter()
        .map(|item| item.release_group_mbid.clone())
        .collect::<Vec<_>>();
    crate::deleted_releases::hide_deleted_release_rows(&transaction, &release_group_mbids)?;
    transaction.commit()
}

fn database_error(error: rusqlite::Error) -> NewsError {
    let message = error.to_string();
    drop(error);
    NewsError::Database(message)
}
