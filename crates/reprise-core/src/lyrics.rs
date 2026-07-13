//! Provider-neutral lyrics model and blocking LRCLIB cache boundary.
//!
//! The frontend calls [`load_or_fetch`] from a dedicated worker thread. This
//! module never sees GTK or playback objects and never writes beside music
//! files: its only persistent output is versioned JSON below the XDG cache.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const API_URL: &str = "https://lrclib.net/api/get";
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const CACHE_VERSION: u32 = 1;
const FIXTURE_DIR_ENV: &str = "REPRISE_LRCLIB_FIXTURE_DIR";
const FIXTURE_LOG_ENV: &str = "REPRISE_LRCLIB_FIXTURE_LOG";
pub(crate) const NEGATIVE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricsQuery {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
}

impl LyricsQuery {
    fn canonical(&self) -> Self {
        Self {
            title: collapse_whitespace(&self.title),
            artist: collapse_whitespace(&self.artist),
            album: collapse_whitespace(&self.album),
            duration_ms: self.duration_ms.max(0),
        }
    }

    fn cache_identity(&self) -> String {
        let query = self.canonical();
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            query.artist.to_lowercase(),
            query.title.to_lowercase(),
            query.album.to_lowercase(),
            rounded_duration_seconds(query.duration_ms)
        )
    }

    fn has_required_metadata(&self) -> bool {
        !self.title.trim().is_empty() && !self.artist.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedLine {
    pub start_ms: i64,
    pub text: String,
}

impl TimedLine {
    pub fn new(start_ms: i64, text: impl Into<String>) -> Self {
        Self {
            start_ms,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricsBody {
    Synced(Vec<TimedLine>),
    Plain(String),
    Instrumental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LyricsError {
    #[error("track title and artist are required for a lyrics lookup")]
    MissingMetadata,
    #[error("no lyrics were found")]
    NotFound,
    #[error("the lyrics service is temporarily unavailable")]
    Temporary,
    #[error("the lyrics service returned an invalid response")]
    InvalidResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HttpOutcome {
    Found(String),
    NotFound,
    Temporary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FixtureRequest {
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) album: String,
    pub(crate) duration_seconds: i64,
}

impl FixtureRequest {
    pub(crate) fn filename(&self) -> String {
        format!(
            "lyrics-{}--{}--{}--{}.json",
            crate::musicbrainz::urlencode(&self.title),
            crate::musicbrainz::urlencode(&self.artist),
            crate::musicbrainz::urlencode(&self.album),
            self.duration_seconds
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheRecord {
    version: u32,
    query: LyricsQuery,
    fetched_at: i64,
    result: CachedResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CachedResult {
    Found(LyricsBody),
    NotFound,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderResponse {
    #[serde(default)]
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
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

pub(crate) fn fixture_request(url: &str) -> Option<FixtureRequest> {
    let url = url::Url::parse(url).ok()?;
    if url.scheme() != "https" || url.host_str() != Some("lrclib.net") || url.path() != "/api/get" {
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
    let title = title.filter(|value| !value.trim().is_empty())?;
    let artist = artist.filter(|value| !value.trim().is_empty())?;
    let album = album.unwrap_or_default();
    let duration_seconds = duration?.parse::<i64>().ok()?;
    if duration_seconds < 0 {
        return None;
    }
    Some(FixtureRequest {
        title,
        artist,
        album,
        duration_seconds,
    })
}

pub(crate) fn fixture_get_at(url: &str, directory: &Path, log_path: Option<&Path>) -> HttpOutcome {
    let Some(request) = fixture_request(url) else {
        return HttpOutcome::Temporary;
    };
    if let Some(log_path) = log_path {
        let line = match serde_json::to_string(&request) {
            Ok(line) => line,
            Err(_) => return HttpOutcome::Temporary,
        };
        let written = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .and_then(|mut file| writeln!(file, "{line}"));
        if written.is_err() {
            return HttpOutcome::Temporary;
        }
    }
    let path = directory.join(request.filename());
    if let Ok(delay) = std::fs::read_to_string(path.with_extension("delay-ms")) {
        let millis = delay.trim().parse::<u64>().unwrap_or_default();
        std::thread::sleep(Duration::from_millis(millis));
    }
    std::fs::read_to_string(path).map_or(HttpOutcome::Temporary, HttpOutcome::Found)
}

pub fn parse_lrc(input: &str) -> Vec<TimedLine> {
    let mut lines = Vec::new();
    for raw_line in input.lines() {
        let mut rest = raw_line.trim_start();
        let mut timestamps = Vec::new();
        while let Some(after_open) = rest.strip_prefix('[') {
            let Some(end) = after_open.find(']') else {
                break;
            };
            let tag = &after_open[..end];
            if let Some(timestamp) = parse_timestamp(tag) {
                timestamps.push(timestamp);
            }
            rest = &after_open[end + 1..];
        }
        if timestamps.is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        for start_ms in timestamps {
            lines.push(TimedLine {
                start_ms,
                text: text.clone(),
            });
        }
    }
    lines.sort_by_key(|line| line.start_ms);
    lines
}

pub fn active_line_index(lines: &[TimedLine], position_ms: i64) -> Option<usize> {
    let insertion = lines.partition_point(|line| line.start_ms <= position_ms);
    insertion.checked_sub(1)
}

pub fn load_or_fetch(query: &LyricsQuery) -> Result<LyricsBody, LyricsError> {
    load_or_fetch_at(&cache_dir(), unix_timestamp(), query, false, http_get)
}

pub(crate) fn parse_response(body: &str) -> Result<LyricsBody, LyricsError> {
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(LyricsError::InvalidResponse);
    }
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

pub(crate) fn load_or_fetch_at<F>(
    cache_dir: &Path,
    now: i64,
    query: &LyricsQuery,
    force: bool,
    mut fetch: F,
) -> Result<LyricsBody, LyricsError>
where
    F: FnMut(&str) -> HttpOutcome,
{
    let url = request_url(query)?;
    let query = query.canonical();
    let cached = read_cache(cache_dir, &query);
    if !force {
        match cached.as_ref().map(|record| &record.result) {
            Some(CachedResult::Found(body)) => return Ok(body.clone()),
            Some(CachedResult::NotFound)
                if cached
                    .as_ref()
                    .is_some_and(|record| negative_is_fresh(record, now)) =>
            {
                return Err(LyricsError::NotFound);
            }
            _ => {}
        }
    }

    match fetch(&url) {
        HttpOutcome::Found(body) => match parse_response(&body) {
            Ok(lyrics) => {
                write_cache(
                    cache_dir,
                    &query,
                    &CacheRecord {
                        version: CACHE_VERSION,
                        query: query.clone(),
                        fetched_at: now,
                        result: CachedResult::Found(lyrics.clone()),
                    },
                );
                Ok(lyrics)
            }
            Err(error) => cached_positive_or(cached.as_ref(), error),
        },
        HttpOutcome::NotFound => {
            write_cache(
                cache_dir,
                &query,
                &CacheRecord {
                    version: CACHE_VERSION,
                    query: query.clone(),
                    fetched_at: now,
                    result: CachedResult::NotFound,
                },
            );
            Err(LyricsError::NotFound)
        }
        HttpOutcome::Temporary => cached_positive_or(cached.as_ref(), LyricsError::Temporary),
    }
}

pub(crate) fn cache_file(cache_dir: &Path, query: &LyricsQuery) -> PathBuf {
    let key = crate::cover::hash_hex(query.cache_identity().as_bytes());
    cache_dir.join(format!("{key}.json"))
}

fn parse_timestamp(tag: &str) -> Option<i64> {
    let (minutes, seconds_fraction) = tag.split_once(':')?;
    let minutes = minutes.parse::<i64>().ok()?;
    let (seconds, fraction) = match seconds_fraction.split_once('.') {
        Some((seconds, fraction)) => (seconds, Some(fraction)),
        None => (seconds_fraction, None),
    };
    let seconds = seconds.parse::<i64>().ok()?;
    if minutes < 0 || !(0..60).contains(&seconds) {
        return None;
    }
    let fraction_ms = match fraction {
        None => 0,
        Some(value) if !value.is_empty() && value.len() <= 3 => {
            let parsed = value.parse::<i64>().ok()?;
            parsed * 10_i64.pow(u32::try_from(3 - value.len()).ok()?)
        }
        Some(_) => return None,
    };
    minutes
        .checked_mul(60_000)?
        .checked_add(seconds.checked_mul(1_000)?)?
        .checked_add(fraction_ms)
}

fn rounded_duration_seconds(duration_ms: i64) -> i64 {
    duration_ms.max(0).saturating_add(500) / 1_000
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("reprise/lyrics")
}

fn read_cache(cache_dir: &Path, query: &LyricsQuery) -> Option<CacheRecord> {
    let path = cache_file(cache_dir, query);
    let body = match std::fs::read(&path) {
        Ok(body) => body,
        Err(_) => return None,
    };
    let record = serde_json::from_slice::<CacheRecord>(&body).ok();
    let valid = record.filter(|record| {
        record.version == CACHE_VERSION && record.query.cache_identity() == query.cache_identity()
    });
    if valid.is_none() {
        let _ = std::fs::remove_file(path);
    }
    valid
}

fn negative_is_fresh(record: &CacheRecord, now: i64) -> bool {
    matches!(record.result, CachedResult::NotFound)
        && now.saturating_sub(record.fetched_at).max(0) <= NEGATIVE_TTL_SECONDS
}

fn cached_positive_or(
    cached: Option<&CacheRecord>,
    error: LyricsError,
) -> Result<LyricsBody, LyricsError> {
    match cached.map(|record| &record.result) {
        Some(CachedResult::Found(body)) => Ok(body.clone()),
        _ => Err(error),
    }
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

fn http_get(url: &str) -> HttpOutcome {
    if let Ok(directory) = std::env::var(FIXTURE_DIR_ENV) {
        let log_path = std::env::var(FIXTURE_LOG_ENV).ok().map(PathBuf::from);
        return fixture_get_at(url, Path::new(&directory), log_path.as_deref());
    }
    let response = match ureq::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(&crate::musicbrainz::user_agent())
        .build()
        .get(url)
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(404, _)) => return HttpOutcome::NotFound,
        Err(_) => return HttpOutcome::Temporary,
    };
    let mut body = Vec::new();
    use std::io::Read;
    if response
        .into_reader()
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .is_err()
        || body.len() > MAX_RESPONSE_BYTES
    {
        return HttpOutcome::Temporary;
    }
    String::from_utf8(body).map_or(HttpOutcome::Temporary, HttpOutcome::Found)
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}
