use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::breaker::{Breaker, BreakerOutcome, HOST_BREAKER};
use super::{
    parse_lrc, rounded_duration_seconds, LyricsBody, LyricsError, LyricsHit, LyricsProvider,
    LyricsQuery, LyricsSource, SourceOutcome,
};

pub(super) const HOST: &str = "lrclib.net";
const API_URL: &str = "https://lrclib.net/api/get";
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_INTERVAL: Duration = Duration::from_millis(250);
const FIXTURE_DIR_ENV: &str = "REPRISE_LYRICS_FIXTURE_DIR";
const LEGACY_FIXTURE_DIR_ENV: &str = "REPRISE_LRCLIB_FIXTURE_DIR";
const FIXTURE_LOG_ENV: &str = "REPRISE_LYRICS_FIXTURE_LOG";
const LEGACY_FIXTURE_LOG_ENV: &str = "REPRISE_LRCLIB_FIXTURE_LOG";
static LAST_REQUEST: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FetchOutcome {
    Found(String),
    NotFound,
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
                Ok(body) => {
                    self.breaker.record(HOST, BreakerOutcome::Success, self.now);
                    SourceOutcome::Hit(LyricsHit {
                        body,
                        source: LyricsSource::Lrclib,
                    })
                }
                Err(_) => SourceOutcome::Failed,
            },
            FetchOutcome::NotFound => {
                self.breaker
                    .record(HOST, BreakerOutcome::NotFound, self.now);
                SourceOutcome::NotFound
            }
            FetchOutcome::Failed(counts_for_breaker) => {
                if counts_for_breaker {
                    self.breaker.record(HOST, BreakerOutcome::Failure, self.now);
                }
                SourceOutcome::Failed
            }
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
    if response.instrumental {
        return Ok(LyricsBody::Instrumental);
    }
    if let Some(synced) = response.synced_lyrics {
        let lines = parse_lrc(&synced);
        if !lines.is_empty() {
            return Ok(LyricsBody::Synced(lines));
        }
    }
    if let Some(plain) = response.plain_lyrics {
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
        .build()
        .new_agent()
        .get(url)
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return FetchOutcome::NotFound,
        Err(ureq::Error::StatusCode(code)) => {
            return FetchOutcome::Failed(code >= 500);
        }
        Err(_) => return FetchOutcome::Failed(true),
    };
    crate::http_body::read_bounded_string(response.into_body().into_reader())
        .map_or(FetchOutcome::Failed(false), FetchOutcome::Found)
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
