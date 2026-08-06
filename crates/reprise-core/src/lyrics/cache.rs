use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{LyricsBody, LyricsHit, LyricsQuery};

const CACHE_VERSION: u32 = 3;
pub(crate) const NEGATIVE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeedsFetch {
    Skip,
    Fetch,
    RetryForSynced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CacheRecord {
    version: u32,
    query: LyricsQuery,
    pub(super) fetched_at: i64,
    pub(super) result: CachedResult,
    #[serde(default)]
    synced_retry_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum CachedResult {
    Found(LyricsHit),
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CacheDecision {
    record: Option<CacheRecord>,
    classification: NeedsFetch,
    classified_at: i64,
}

impl CacheDecision {
    pub(super) fn classification(&self) -> NeedsFetch {
        self.classification
    }

    pub(super) fn still_applies_to(&self, record: Option<&CacheRecord>, now: i64) -> bool {
        self.classified_at == now && self.record.as_ref() == record
    }
}

pub(super) fn decision(query: &LyricsQuery) -> CacheDecision {
    decision_at(&cache_dir(), super::unix_timestamp(), query)
}

#[cfg(test)]
pub(super) fn needs_fetch_at(cache_dir: &Path, now: i64, query: &LyricsQuery) -> NeedsFetch {
    decision_at(cache_dir, now, query).classification()
}

pub(super) fn decision_at(cache_dir: &Path, now: i64, query: &LyricsQuery) -> CacheDecision {
    let record = read_cache(cache_dir, query);
    let classification = classify(record.as_ref(), now);
    CacheDecision {
        record,
        classification,
        classified_at: now,
    }
}

pub(super) fn classify(record: Option<&CacheRecord>, now: i64) -> NeedsFetch {
    match record.map(|record| &record.result) {
        None => NeedsFetch::Fetch,
        Some(CachedResult::NotFound)
            if record.is_some_and(|record| negative_is_fresh(record, now)) =>
        {
            NeedsFetch::Skip
        }
        Some(CachedResult::NotFound) => NeedsFetch::Fetch,
        Some(CachedResult::Found(LyricsHit {
            body: LyricsBody::Plain(_),
            ..
        })) if record.is_some_and(|record| plain_retry_is_fresh(record, now)) => NeedsFetch::Skip,
        Some(CachedResult::Found(LyricsHit {
            body: LyricsBody::Plain(_),
            ..
        })) => NeedsFetch::RetryForSynced,
        Some(CachedResult::Found(_)) => NeedsFetch::Skip,
    }
}

#[cfg(test)]
pub(super) fn cached_hit(cache_dir: &Path, query: &LyricsQuery) -> Option<LyricsHit> {
    match read_cache(cache_dir, query)?.result {
        CachedResult::Found(hit) => Some(hit),
        CachedResult::NotFound => None,
    }
}

pub(super) fn read_cache(cache_dir: &Path, query: &LyricsQuery) -> Option<CacheRecord> {
    let path = cache_file(cache_dir, query);
    let body = std::fs::read(&path).ok()?;
    let record = serde_json::from_slice::<CacheRecord>(&body).ok();
    let valid = record.filter(|record| {
        record.version == CACHE_VERSION && record.query.cache_identity() == query.cache_identity()
    });
    if valid.is_none() {
        let _ = std::fs::remove_file(path);
    }
    valid
}

pub(super) fn write_found(
    cache_dir: &Path,
    now: i64,
    query: &LyricsQuery,
    hit: &LyricsHit,
    synced_retry_attempted: bool,
) {
    write_cache(
        cache_dir,
        query,
        &CacheRecord {
            version: CACHE_VERSION,
            query: query.canonical(),
            fetched_at: now,
            result: CachedResult::Found(hit.clone()),
            synced_retry_at: (synced_retry_attempted && matches!(&hit.body, LyricsBody::Plain(_)))
                .then_some(now),
        },
    );
}

pub(super) fn write_not_found(cache_dir: &Path, now: i64, query: &LyricsQuery) {
    write_cache(
        cache_dir,
        query,
        &CacheRecord {
            version: CACHE_VERSION,
            query: query.canonical(),
            fetched_at: now,
            result: CachedResult::NotFound,
            synced_retry_at: None,
        },
    );
}

pub(super) fn negative_is_fresh(record: &CacheRecord, now: i64) -> bool {
    matches!(record.result, CachedResult::NotFound) && is_fresh(record.fetched_at, now)
}

pub(super) fn plain_retry_is_fresh(record: &CacheRecord, now: i64) -> bool {
    matches!(
        &record.result,
        CachedResult::Found(LyricsHit {
            body: LyricsBody::Plain(_),
            ..
        })
    ) && record
        .synced_retry_at
        .is_some_and(|retried_at| is_fresh(retried_at, now))
}

pub(super) fn cache_file(cache_dir: &Path, query: &LyricsQuery) -> PathBuf {
    let key = crate::cover::hash_hex(query.cache_identity().as_bytes());
    cache_dir.join(format!("{key}.json"))
}

pub(super) fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("reprise/lyrics")
}

fn is_fresh(timestamp: i64, now: i64) -> bool {
    now.saturating_sub(timestamp).max(0) <= NEGATIVE_TTL_SECONDS
}

fn write_cache(cache_dir: &Path, query: &LyricsQuery, record: &CacheRecord) {
    let Ok(body) = serde_json::to_vec(record) else {
        return;
    };
    if std::fs::create_dir_all(cache_dir).is_err() {
        tracing::warn!("could not create lyrics cache directory");
        return;
    }
    let destination = cache_file(cache_dir, query);
    let temporary = cache_dir.join(format!(".lyrics-{}.tmp", fastrand::u64(..)));
    if std::fs::write(&temporary, body).is_err()
        || std::fs::rename(&temporary, destination).is_err()
    {
        let _ = std::fs::remove_file(temporary);
        tracing::warn!("could not publish lyrics cache entry");
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
