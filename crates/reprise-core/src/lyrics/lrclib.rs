use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::breaker::{Breaker, BreakerOutcome, HOST_BREAKER};
use super::{
    parse_lrc, rounded_duration_seconds, LyricsBody, LyricsError, LyricsHit, LyricsProvider,
    LyricsQuery, LyricsSource, SourceOutcome,
};

pub(super) const HOST: &str = "lrclib.net";
const API_URL: &str = "https://lrclib.net/api/get";
const SEARCH_API_URL: &str = "https://lrclib.net/api/search";
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_INTERVAL: Duration = Duration::from_millis(250);
const SEARCH_DURATION_TOLERANCE_SECONDS: f64 = 2.0;
const SEARCH_DURATION_TOLERANCE_MILLIS: u16 = 2_000;
const FIXTURE_DIR_ENV: &str = "REPRISE_LYRICS_FIXTURE_DIR";
const LEGACY_FIXTURE_DIR_ENV: &str = "REPRISE_LRCLIB_FIXTURE_DIR";
const FIXTURE_LOG_ENV: &str = "REPRISE_LYRICS_FIXTURE_LOG";
const LEGACY_FIXTURE_LOG_ENV: &str = "REPRISE_LRCLIB_FIXTURE_LOG";
static LAST_REQUEST: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FetchOutcome {
    Found(String),
    NotFound,
    RateLimited(Option<i64>),
    Failed(bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct FixtureRequest {
    pub(super) title: String,
    pub(super) artist: String,
    pub(super) album: String,
    pub(super) duration_seconds: i64,
}

impl FixtureRequest {
    pub(super) fn filename(&self) -> String {
        format!("lrclib-{}", self.identity_suffix())
    }

    pub(super) fn legacy_filename(&self) -> String {
        format!("lyrics-{}", self.identity_suffix())
    }

    fn identity_suffix(&self) -> String {
        format!(
            "{}--{}--{}--{}.json",
            crate::musicbrainz::urlencode(&self.title),
            crate::musicbrainz::urlencode(&self.artist),
            crate::musicbrainz::urlencode(&self.album),
            self.duration_seconds
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderResponse {
    #[serde(default)]
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    track_name: String,
    artist_name: String,
    #[serde(default)]
    album_name: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(flatten)]
    lyrics: ProviderResponse,
}

#[derive(Debug)]
enum ResolvedOutcome {
    Hit(LyricsBody),
    NotFound,
    Failed,
}

#[derive(Clone, Copy, Debug)]
enum BreakerTransition {
    Record(BreakerOutcome),
    RateLimited(Option<i64>),
    Preserve,
}

#[derive(Debug)]
struct LookupResolution {
    outcome: ResolvedOutcome,
    breaker: BreakerTransition,
}

impl LookupResolution {
    fn successful_hit(body: LyricsBody) -> Self {
        Self {
            outcome: ResolvedOutcome::Hit(body),
            breaker: BreakerTransition::Record(BreakerOutcome::Success),
        }
    }

    fn not_found() -> Self {
        Self {
            outcome: ResolvedOutcome::NotFound,
            breaker: BreakerTransition::Record(BreakerOutcome::NotFound),
        }
    }

    fn failed(breaker: BreakerTransition) -> Self {
        Self {
            outcome: ResolvedOutcome::Failed,
            breaker,
        }
    }

    fn retain_exact_plain(body: LyricsBody, breaker: BreakerTransition) -> Self {
        Self {
            outcome: ResolvedOutcome::Hit(body),
            breaker,
        }
    }
}

pub(super) struct LrclibProvider<'a> {
    fetch: &'a dyn Fn(&str) -> FetchOutcome,
    breaker: &'a Breaker,
    now: i64,
    force: bool,
}

impl<'a> LrclibProvider<'a> {
    pub(super) fn new(
        fetch: &'a dyn Fn(&str) -> FetchOutcome,
        breaker: &'a Breaker,
        now: i64,
        force: bool,
    ) -> Self {
        Self {
            fetch,
            breaker,
            now,
            force,
        }
    }

    fn search(&self, query: &LyricsQuery, exact_plain: Option<LyricsBody>) -> SourceOutcome {
        let Ok(url) = search_url(query) else {
            let resolution = exact_plain.map_or_else(
                || LookupResolution::failed(BreakerTransition::Preserve),
                LookupResolution::successful_hit,
            );
            return self.finish(resolution);
        };
        let resolution = resolve_search((self.fetch)(&url), query, exact_plain);
        self.finish(resolution)
    }

    fn finish(&self, resolution: LookupResolution) -> SourceOutcome {
        match resolution.breaker {
            BreakerTransition::Record(outcome) => self.breaker.record(HOST, outcome, self.now),
            BreakerTransition::RateLimited(retry_after) => {
                self.breaker
                    .record_rate_limited_until(HOST, self.now, retry_after);
            }
            BreakerTransition::Preserve => {}
        }
        match resolution.outcome {
            ResolvedOutcome::Hit(body) => SourceOutcome::Hit(LyricsHit {
                body,
                source: LyricsSource::Lrclib,
            }),
            ResolvedOutcome::NotFound => SourceOutcome::NotFound,
            ResolvedOutcome::Failed => SourceOutcome::Failed,
        }
    }
}

impl LyricsProvider for LrclibProvider<'_> {
    fn source(&self) -> LyricsSource {
        LyricsSource::Lrclib
    }

    fn lookup(&self, query: &LyricsQuery, _track_path: Option<&Path>) -> SourceOutcome {
        if !self.breaker.can_attempt(HOST, self.now, self.force) {
            return SourceOutcome::Skipped;
        }
        let Ok(url) = request_url(query) else {
            return SourceOutcome::Failed;
        };
        match (self.fetch)(&url) {
            FetchOutcome::Found(body) => match parse_response(&body) {
                Ok(body @ LyricsBody::Plain(_)) => self.search(query, Some(body)),
                Ok(body) => self.finish(LookupResolution::successful_hit(body)),
                Err(_) => self.finish(LookupResolution::failed(BreakerTransition::Preserve)),
            },
            FetchOutcome::NotFound => self.search(query, None),
            FetchOutcome::RateLimited(retry_after) => self.finish(LookupResolution::failed(
                BreakerTransition::RateLimited(retry_after),
            )),
            FetchOutcome::Failed(true) => self.finish(LookupResolution::failed(
                BreakerTransition::Record(BreakerOutcome::Failure),
            )),
            FetchOutcome::Failed(false) => {
                self.finish(LookupResolution::failed(BreakerTransition::Preserve))
            }
        }
    }
}

fn resolve_search(
    fetched: FetchOutcome,
    query: &LyricsQuery,
    exact_plain: Option<LyricsBody>,
) -> LookupResolution {
    match fetched {
        FetchOutcome::Found(body) => {
            let parsed = parse_search_response(&body, query);
            if let Ok(Some(body @ LyricsBody::Synced(_))) = parsed {
                return LookupResolution::successful_hit(body);
            }
            if let Some(body) = exact_plain {
                return LookupResolution::successful_hit(body);
            }
            match parsed {
                Ok(Some(body)) => LookupResolution::successful_hit(body),
                Ok(None) => LookupResolution::not_found(),
                Err(_) => LookupResolution::failed(BreakerTransition::Preserve),
            }
        }
        FetchOutcome::NotFound => exact_plain.map_or_else(
            LookupResolution::not_found,
            LookupResolution::successful_hit,
        ),
        FetchOutcome::RateLimited(retry_after) => exact_plain.map_or_else(
            || LookupResolution::failed(BreakerTransition::RateLimited(retry_after)),
            |body| {
                LookupResolution::retain_exact_plain(
                    body,
                    BreakerTransition::RateLimited(retry_after),
                )
            },
        ),
        FetchOutcome::Failed(counts_for_breaker) => {
            let breaker = if counts_for_breaker {
                BreakerTransition::Record(BreakerOutcome::Failure)
            } else {
                BreakerTransition::Preserve
            };
            exact_plain.map_or_else(
                || LookupResolution::failed(breaker),
                |body| LookupResolution::retain_exact_plain(body, breaker),
            )
        }
    }
}

pub(super) fn production_provider(now: i64, force: bool) -> LrclibProvider<'static> {
    LrclibProvider::new(&fetch, &HOST_BREAKER, now, force)
}

pub fn request_url(query: &LyricsQuery) -> Result<String, LyricsError> {
    if !query.has_required_metadata() {
        return Err(LyricsError::MissingMetadata);
    }
    let query = query.canonical();
    let mut url = url::Url::parse(API_URL).map_err(|_| LyricsError::InvalidResponse)?;
    url.query_pairs_mut()
        .append_pair("track_name", &query.title)
        .append_pair("artist_name", &query.artist)
        .append_pair("album_name", &query.album)
        .append_pair(
            "duration",
            &rounded_duration_seconds(query.duration_ms).to_string(),
        );
    Ok(url.into())
}

fn search_url(query: &LyricsQuery) -> Result<String, LyricsError> {
    if !query.has_required_metadata() {
        return Err(LyricsError::MissingMetadata);
    }
    let query = query.canonical();
    let mut url = url::Url::parse(SEARCH_API_URL).map_err(|_| LyricsError::InvalidResponse)?;
    url.query_pairs_mut()
        .append_pair("track_name", &query.title)
        .append_pair("artist_name", &query.artist);
    Ok(url.into())
}

pub(super) fn fixture_request(url: &str) -> Option<FixtureRequest> {
    let url = url::Url::parse(url).ok()?;
    if url.scheme() != "https" || url.host_str() != Some(HOST) || url.path() != "/api/get" {
        return None;
    }
    let mut title = None;
    let mut artist = None;
    let mut album = None;
    let mut duration = None;
    for (key, value) in url.query_pairs() {
        let slot = match key.as_ref() {
            "track_name" => &mut title,
            "artist_name" => &mut artist,
            "album_name" => &mut album,
            "duration" => &mut duration,
            _ => return None,
        };
        if slot.replace(value.into_owned()).is_some() {
            return None;
        }
    }
    Some(FixtureRequest {
        title: title.filter(|value| !value.trim().is_empty())?,
        artist: artist.filter(|value| !value.trim().is_empty())?,
        album: album.unwrap_or_default(),
        duration_seconds: duration?.parse::<i64>().ok().filter(|value| *value >= 0)?,
    })
}

pub(super) fn fixture_get_at(url: &str, directory: &Path, log_path: Option<&Path>) -> FetchOutcome {
    let Some(request) = fixture_request(url) else {
        return FetchOutcome::Failed(false);
    };
    if !append_fixture_log(&request, log_path) {
        return FetchOutcome::Failed(false);
    }
    for filename in [request.filename(), request.legacy_filename()] {
        let path = directory.join(filename);
        if path.is_file() {
            return std::fs::File::open(path)
                .map_err(|_| ())
                .and_then(|file| crate::http_body::read_bounded_string(file).map_err(|_| ()))
                .map_or(FetchOutcome::Failed(false), FetchOutcome::Found);
        }
    }
    FetchOutcome::Failed(false)
}

pub(super) fn parse_response(body: &str) -> Result<LyricsBody, LyricsError> {
    let response: ProviderResponse =
        serde_json::from_str(body).map_err(|_| LyricsError::InvalidResponse)?;
    body_from_response(&response)
}

fn parse_search_response(
    body: &str,
    query: &LyricsQuery,
) -> Result<Option<LyricsBody>, LyricsError> {
    let responses: Vec<SearchResponse> =
        serde_json::from_str(body).map_err(|_| LyricsError::InvalidResponse)?;
    let mut best = None;
    let mut tied = false;
    for response in responses
        .iter()
        .filter(|response| search_candidate_matches(response, query))
    {
        let Ok(body) = body_from_response(&response.lyrics) else {
            continue;
        };
        let score = search_candidate_score(response, query, &body);
        match best.as_ref() {
            None => best = Some((score, body)),
            Some((best_score, _best_body)) if score > *best_score => {
                best = Some((score, body));
                tied = false;
            }
            Some((best_score, _best_body)) if score == *best_score => tied = true,
            Some(_) => {}
        }
    }
    Ok(match best {
        Some((_score, body)) if !tied => Some(body),
        _ => None,
    })
}

fn search_candidate_matches(response: &SearchResponse, query: &LyricsQuery) -> bool {
    let duration_seconds = query.duration_ms as f64 / 1_000.0;
    query.duration_ms > 0
        && normalized(&response.track_name) == normalized(&query.title)
        && normalized(&response.artist_name) == normalized(&query.artist)
        && response.duration.is_some_and(|candidate| {
            candidate.is_finite()
                && (candidate - duration_seconds).abs() <= SEARCH_DURATION_TOLERANCE_SECONDS
        })
}

fn normalized(value: &str) -> String {
    super::collapse_whitespace(value).to_lowercase()
}

fn search_candidate_score(
    response: &SearchResponse,
    query: &LyricsQuery,
    body: &LyricsBody,
) -> (u8, u8, u16) {
    let album_match = (!query.album.trim().is_empty()
        && response
            .album_name
            .as_deref()
            .is_some_and(|album| normalized(album) == normalized(&query.album)))
    .into();
    let duration_closeness = response.duration.map_or(0, |duration| {
        let query_duration_seconds = query.duration_ms as f64 / 1_000.0;
        let delta_millis = ((duration - query_duration_seconds).abs() * 1_000.0).round() as u16;
        SEARCH_DURATION_TOLERANCE_MILLIS.saturating_sub(delta_millis)
    });
    (body_preference(body), album_match, duration_closeness)
}

fn body_preference(body: &LyricsBody) -> u8 {
    match body {
        LyricsBody::Synced(_) => 2,
        LyricsBody::Plain(_) => 1,
        LyricsBody::Instrumental => 0,
    }
}

fn body_from_response(response: &ProviderResponse) -> Result<LyricsBody, LyricsError> {
    if response.instrumental {
        return Ok(LyricsBody::Instrumental);
    }
    if let Some(synced) = &response.synced_lyrics {
        let lines = parse_lrc(synced);
        if !lines.is_empty() {
            return Ok(LyricsBody::Synced(lines));
        }
    }
    if let Some(plain) = &response.plain_lyrics {
        let plain = plain.trim();
        if !plain.is_empty() {
            return Ok(LyricsBody::Plain(plain.to_string()));
        }
    }
    Err(LyricsError::InvalidResponse)
}

fn fetch(url: &str) -> FetchOutcome {
    if let Some(directory) = fixture_directory() {
        return fixture_get_at(url, &directory, fixture_log().as_deref());
    }
    wait_for_request_slot();
    let response = match ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(crate::musicbrainz::user_agent())
        .http_status_as_error(false)
        .build()
        .new_agent()
        .get(url)
        .call()
    {
        Ok(response) => response,
        Err(_) => return FetchOutcome::Failed(true),
    };
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("Retry-After")
        .and_then(|value| value.to_str().ok());
    if let Some(outcome) = http_status_outcome(status, retry_after, SystemTime::now()) {
        return outcome;
    }
    crate::http_body::read_bounded_string(response.into_body().into_reader())
        .map_or(FetchOutcome::Failed(false), FetchOutcome::Found)
}

fn http_status_outcome(
    status: u16,
    retry_after: Option<&str>,
    observed_at: SystemTime,
) -> Option<FetchOutcome> {
    match status {
        200..=299 => None,
        404 => Some(FetchOutcome::NotFound),
        429 => Some(FetchOutcome::RateLimited(retry_after_deadline(
            retry_after,
            observed_at,
        ))),
        500..=599 => Some(FetchOutcome::Failed(true)),
        _ => Some(FetchOutcome::Failed(false)),
    }
}

fn retry_after_deadline(value: Option<&str>, observed_at: SystemTime) -> Option<i64> {
    let delay = crate::source_error::parse_retry_after_at(value, observed_at)?;
    let deadline = observed_at.checked_add(delay)?;
    let since_epoch = deadline.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = i64::try_from(since_epoch.as_secs()).unwrap_or(i64::MAX);
    Some(if since_epoch.subsec_nanos() == 0 {
        seconds
    } else {
        seconds.saturating_add(1)
    })
}

fn append_fixture_log(request: &FixtureRequest, log_path: Option<&Path>) -> bool {
    let Some(log_path) = log_path else {
        return true;
    };
    let Ok(line) = serde_json::to_string(request) else {
        return false;
    };
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .and_then(|mut file| writeln!(file, "{line}"))
        .is_ok()
}

fn fixture_directory() -> Option<PathBuf> {
    std::env::var(FIXTURE_DIR_ENV)
        .or_else(|_| std::env::var(LEGACY_FIXTURE_DIR_ENV))
        .ok()
        .map(PathBuf::from)
}

fn fixture_log() -> Option<PathBuf> {
    std::env::var(FIXTURE_LOG_ENV)
        .or_else(|_| std::env::var(LEGACY_FIXTURE_LOG_ENV))
        .ok()
        .map(PathBuf::from)
}

fn wait_for_request_slot() {
    let mut last = LAST_REQUEST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(remaining) =
        last.and_then(|instant| REQUEST_INTERVAL.checked_sub(instant.elapsed()))
    {
        std::thread::sleep(remaining);
    }
    *last = Some(Instant::now());
}

#[cfg(test)]
#[path = "lrclib_tests.rs"]
mod tests;
