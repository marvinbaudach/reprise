//! Local-first, provider-neutral lyrics lookup and cache boundary.
//!
//! Frontends call [`load_or_fetch`] from a worker thread. Local tag and
//! sidecar reads always run before the cache and online providers. A
//! synchronized network hit is also written best-effort to a new `.lrc`
//! derived from an existing track path: publication is atomic, an existing
//! sidecar is never overwritten, and every write failure leaves lookup and
//! cache results unchanged.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

mod batch;
mod breaker;
mod cache;
mod chain;
mod local;
mod lrc;
mod lrclib;
mod model;
mod netease;
mod sidecar_write;

pub use batch::{
    run_batch, run_batch_with_source, BatchProgress, BatchRunStatus, BatchState, BatchTrack,
};
pub use cache::NeedsFetch;
pub use local::local_hit;
pub use lrc::{active_line_index, parse_lrc};
pub use lrclib::request_url;
pub use model::{
    LyricsBody, LyricsError, LyricsHit, LyricsProvider, LyricsQuery, LyricsSource, SourceHit,
    SourceOutcome, TimedLine,
};

use crate::library::source::{LibrarySource, UnixLibrarySource};
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
    load_or_fetch_with_source(&UnixLibrarySource, query, track_path)
}

pub fn load_or_fetch_with_source(
    source: &dyn LibrarySource,
    query: &LyricsQuery,
    track_path: Option<&Path>,
) -> Result<LyricsHit, LyricsError> {
    load_or_fetch_with_options_and_source(source, query, track_path, LookupOptions::default())
}

pub fn load_or_fetch_with_options(
    query: &LyricsQuery,
    track_path: Option<&Path>,
    options: LookupOptions,
) -> Result<LyricsHit, LyricsError> {
    load_or_fetch_with_options_and_source(&UnixLibrarySource, query, track_path, options)
}

pub fn load_or_fetch_with_options_and_source(
    source: &dyn LibrarySource,
    query: &LyricsQuery,
    track_path: Option<&Path>,
    options: LookupOptions,
) -> Result<LyricsHit, LyricsError> {
    load_or_fetch_with_cache_context(source, query, track_path, options, None)
}

fn load_or_fetch_with_cache_decision(
    source: &dyn LibrarySource,
    query: &LyricsQuery,
    track_path: Option<&Path>,
    decision: &cache::CacheDecision,
) -> Result<LyricsHit, LyricsError> {
    load_or_fetch_with_cache_context(
        source,
        query,
        track_path,
        LookupOptions::default(),
        Some(decision),
    )
}

fn load_or_fetch_with_cache_context(
    source: &dyn LibrarySource,
    query: &LyricsQuery,
    track_path: Option<&Path>,
    options: LookupOptions,
    cache_decision: Option<&cache::CacheDecision>,
) -> Result<LyricsHit, LyricsError> {
    let now = unix_timestamp();
    let local = LocalProvider { source };
    let lrclib = lrclib::production_provider(now, options.force);
    let netease = netease::production_provider(now, options.force);
    let network: [&dyn LyricsProvider; 2] = [&lrclib, &netease];
    let network = if options.allow_network {
        network.as_slice()
    } else {
        &[]
    };
    load_or_fetch_with_cache_context_at_from(
        &cache::cache_dir(),
        now,
        query,
        track_path,
        options,
        cache_decision,
        LookupProviders {
            source,
            local: &[&local],
            network,
        },
    )
}

pub fn all_network_breakers_open() -> bool {
    breaker::HOST_BREAKER.all_open(&[lrclib::HOST, netease::HOST], unix_timestamp())
}

#[cfg(test)]
fn load_or_fetch_at(
    cache_dir: &Path,
    now: i64,
    query: &LyricsQuery,
    track_path: Option<&Path>,
    options: LookupOptions,
    local_providers: &[&dyn LyricsProvider],
    network_providers: &[&dyn LyricsProvider],
) -> Result<LyricsHit, LyricsError> {
    load_or_fetch_with_cache_context_at(
        cache_dir,
        now,
        query,
        track_path,
        options,
        None,
        LookupProviders {
            source: &UnixLibrarySource,
            local: local_providers,
            network: network_providers,
        },
    )
}

#[cfg(test)]
fn load_or_fetch_with_cache_context_at(
    cache_dir: &Path,
    now: i64,
    query: &LyricsQuery,
    track_path: Option<&Path>,
    options: LookupOptions,
    cache_decision: Option<&cache::CacheDecision>,
    providers: LookupProviders<'_>,
) -> Result<LyricsHit, LyricsError> {
    load_or_fetch_with_cache_context_at_from(
        cache_dir,
        now,
        query,
        track_path,
        options,
        cache_decision,
        providers,
    )
}

fn load_or_fetch_with_cache_context_at_from(
    cache_dir: &Path,
    now: i64,
    query: &LyricsQuery,
    track_path: Option<&Path>,
    options: LookupOptions,
    cache_decision: Option<&cache::CacheDecision>,
    providers: LookupProviders<'_>,
) -> Result<LyricsHit, LyricsError> {
    let local_plain = match best_local(query, track_path, providers.local) {
        LocalLookup::Final(hit) => return Ok(hit),
        LocalLookup::Plain(hit) => hit,
    };
    let cached = cache::read_cache(cache_dir, query);

    if !options.force {
        let classification = cache_decision
            .filter(|decision| decision.still_applies_to(cached.as_ref(), now))
            .map_or_else(
                || cache::classify(cached.as_ref(), now),
                cache::CacheDecision::classification,
            );
        if classification == NeedsFetch::Skip {
            if let Some(result) = skipped_result(cached.as_ref(), local_plain.as_ref()) {
                return result;
            }
        }
    }
    if !options.allow_network {
        return local_plain.ok_or(LyricsError::Temporary);
    }
    if !query.has_required_metadata() {
        return local_plain.ok_or(LyricsError::MissingMetadata);
    }

    let report = run_chain(query, track_path, local_plain.clone(), providers.network);
    match report.result {
        Ok(hit) => {
            if is_local(hit.source) {
                if report.network_consensus_not_found {
                    cache::write_not_found(cache_dir, now, query);
                }
            } else {
                cache::write_found(cache_dir, now, query, &hit, true);
                if let (Some(track_path), LyricsBody::Synced(lines)) = (track_path, &hit.body) {
                    sidecar_write::write_sidecar_with_source(providers.source, track_path, lines);
                }
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

#[derive(Clone, Copy)]
struct LookupProviders<'a> {
    source: &'a dyn LibrarySource,
    local: &'a [&'a dyn LyricsProvider],
    network: &'a [&'a dyn LyricsProvider],
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

fn skipped_result(
    cached: Option<&cache::CacheRecord>,
    local_plain: Option<&LyricsHit>,
) -> Option<Result<LyricsHit, LyricsError>> {
    match &cached?.result {
        CachedResult::Found(hit) => Some(Ok(prefer_local_plain(local_plain.cloned(), hit.clone()))),
        CachedResult::NotFound => Some(local_plain.cloned().ok_or(LyricsError::NotFound)),
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
