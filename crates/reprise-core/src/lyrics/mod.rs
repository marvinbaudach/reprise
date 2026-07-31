//! Local-first, provider-neutral lyrics lookup and cache boundary.
//!
//! Frontends call [`load_or_fetch`] from a worker thread. Local tag and
//! sidecar reads always run before the cache and online providers; this module
//! never writes beside music files.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

mod breaker;
mod cache;
mod chain;
mod local;
mod lrc;
mod lrclib;
mod model;
mod netease;

pub use cache::{needs_fetch, NeedsFetch};
pub use local::local_hit;
pub use lrc::{active_line_index, parse_lrc};
pub use lrclib::request_url;
pub use model::{
    LyricsBody, LyricsError, LyricsHit, LyricsProvider, LyricsQuery, LyricsSource, SourceHit,
    SourceOutcome, TimedLine,
};

use cache::CachedResult;
use chain::run_chain;
use local::LocalProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupOptions {
    pub allow_network: bool,
    pub force: bool,
}

impl Default for LookupOptions {
    fn default() -> Self {
        Self {
            allow_network: true,
            force: false,
        }
    }
}

pub fn load_or_fetch(
    query: &LyricsQuery,
    track_path: Option<&Path>,
) -> Result<LyricsHit, LyricsError> {
    load_or_fetch_with_options(query, track_path, LookupOptions::default())
}

pub fn load_or_fetch_with_options(
    query: &LyricsQuery,
    track_path: Option<&Path>,
    options: LookupOptions,
) -> Result<LyricsHit, LyricsError> {
    let now = unix_timestamp();
    let local = LocalProvider;
    let lrclib = lrclib::production_provider(now, options.force);
    let netease = netease::production_provider(now, options.force);
    let network: [&dyn LyricsProvider; 2] = [&lrclib, &netease];
    let network = if options.allow_network {
        network.as_slice()
    } else {
        &[]
    };
    load_or_fetch_at(
        &cache::cache_dir(),
        now,
        query,
        track_path,
        options,
        &[&local],
        network,
    )
}

pub fn all_network_breakers_open() -> bool {
    breaker::HOST_BREAKER.all_open(&[lrclib::HOST, netease::HOST], unix_timestamp())
}

fn load_or_fetch_at(
    cache_dir: &Path,
    now: i64,
    query: &LyricsQuery,
    track_path: Option<&Path>,
    options: LookupOptions,
    local_providers: &[&dyn LyricsProvider],
    network_providers: &[&dyn LyricsProvider],
) -> Result<LyricsHit, LyricsError> {
    let local_plain = match best_local(query, track_path, local_providers) {
        LocalLookup::Final(hit) => return Ok(hit),
        LocalLookup::Plain(hit) => hit,
    };
    let cached = cache::read_cache(cache_dir, query);

    if !options.force {
        if let Some(result) = cached_result(cached.as_ref(), local_plain.as_ref(), now) {
            return result;
        }
    }
    if !options.allow_network {
        return local_plain.ok_or(LyricsError::Temporary);
    }
    if !query.has_required_metadata() {
        return local_plain.ok_or(LyricsError::MissingMetadata);
    }

    let report = run_chain(query, track_path, local_providers, network_providers);
    match report.result {
        Ok(hit) => {
            if is_local(hit.source) {
                if report.network_consensus_not_found {
                    cache::write_not_found(cache_dir, now, query);
                }
            } else {
                cache::write_found(cache_dir, now, query, &hit, true);
            }
            Ok(hit)
        }
        Err(error) => {
            if let Some(CachedResult::Found(hit)) = cached.as_ref().map(|record| &record.result) {
                let fallback = prefer_local_plain(local_plain, hit.clone());
                cache::write_found(cache_dir, now, query, &fallback, true);
                return Ok(fallback);
            }
            if report.network_consensus_not_found {
                cache::write_not_found(cache_dir, now, query);
            }
            Err(error)
        }
    }
}

enum LocalLookup {
    Final(LyricsHit),
    Plain(Option<LyricsHit>),
}

fn best_local(
    query: &LyricsQuery,
    track_path: Option<&Path>,
    providers: &[&dyn LyricsProvider],
) -> LocalLookup {
    let mut plain = None;
    for provider in providers {
        let SourceOutcome::Hit(hit) = provider.lookup(query, track_path) else {
            continue;
        };
        match &hit.body {
            LyricsBody::Synced(_) | LyricsBody::Instrumental => {
                return LocalLookup::Final(hit);
            }
            LyricsBody::Plain(_) if plain.is_none() => plain = Some(hit),
            LyricsBody::Plain(_) => {}
        }
    }
    LocalLookup::Plain(plain)
}

fn cached_result(
    cached: Option<&cache::CacheRecord>,
    local_plain: Option<&LyricsHit>,
    now: i64,
) -> Option<Result<LyricsHit, LyricsError>> {
    match cached.map(|record| &record.result) {
        Some(CachedResult::Found(
            hit @ LyricsHit {
                body: LyricsBody::Synced(_) | LyricsBody::Instrumental,
                ..
            },
        )) => Some(Ok(prefer_local_plain(local_plain.cloned(), hit.clone()))),
        Some(CachedResult::Found(hit))
            if cached.is_some_and(|record| cache::plain_retry_is_fresh(record, now)) =>
        {
            Some(Ok(prefer_local_plain(local_plain.cloned(), hit.clone())))
        }
        Some(CachedResult::Found(_)) => None,
        Some(CachedResult::NotFound)
            if cached.is_some_and(|record| cache::negative_is_fresh(record, now)) =>
        {
            Some(local_plain.cloned().ok_or(LyricsError::NotFound))
        }
        _ => None,
    }
}

fn prefer_local_plain(local_plain: Option<LyricsHit>, cached: LyricsHit) -> LyricsHit {
    match (&cached.body, local_plain) {
        (LyricsBody::Synced(_), _) => cached,
        (_, Some(local)) => local,
        (_, None) => cached,
    }
}

fn is_local(source: LyricsSource) -> bool {
    matches!(source, LyricsSource::Tag | LyricsSource::Sidecar)
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn rounded_duration_seconds(duration_ms: i64) -> i64 {
    duration_ms.max(0).saturating_add(500) / 1_000
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
