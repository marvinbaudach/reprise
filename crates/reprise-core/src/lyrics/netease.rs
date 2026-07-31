use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::breaker::{Breaker, BreakerOutcome, HOST_BREAKER};
use super::{
    collapse_whitespace, parse_lrc, LyricsBody, LyricsError, LyricsHit, LyricsProvider,
    LyricsQuery, LyricsSource, SourceOutcome,
};

pub(super) const HOST: &str = "music.163.com";
const SEARCH_URL: &str = "https://music.163.com/api/search/get";
const LYRIC_URL: &str = "https://music.163.com/api/song/lyric";
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_INTERVAL: Duration = Duration::from_millis(250);
const FIXTURE_DIR_ENV: &str = "REPRISE_LYRICS_FIXTURE_DIR";
const DURATION_TOLERANCE_MS: i64 = 3_000;
static LAST_REQUEST: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone, PartialEq, Eq)]
enum FetchOutcome {
    Found(String),
    NotFound,
    Failed(bool),
}

trait NeteaseFetcher {
    fn search(&self, query: &LyricsQuery) -> FetchOutcome;
    fn lyric(&self, id: u64) -> FetchOutcome;
}

struct ProductionFetcher;

pub(super) struct FixtureFetcher<'a> {
    directory: &'a Path,
}

impl<'a> FixtureFetcher<'a> {
    pub(super) fn new(directory: &'a Path) -> Self {
        Self { directory }
    }
}

pub(super) struct NeteaseProvider<'a> {
    fetcher: &'a dyn NeteaseFetcher,
    breaker: &'a Breaker,
    now: i64,
    force: bool,
}

impl<'a> NeteaseProvider<'a> {
    fn new(fetcher: &'a dyn NeteaseFetcher, breaker: &'a Breaker, now: i64, force: bool) -> Self {
        Self {
            fetcher,
            breaker,
            now,
            force,
        }
    }

    fn failed(&self, counts_for_breaker: bool) -> SourceOutcome {
        if counts_for_breaker {
            self.breaker.record(HOST, BreakerOutcome::Failure, self.now);
        }
        SourceOutcome::Failed
    }

    fn not_found(&self) -> SourceOutcome {
        self.breaker
            .record(HOST, BreakerOutcome::NotFound, self.now);
        SourceOutcome::NotFound
    }
}

impl LyricsProvider for NeteaseProvider<'_> {
    fn source(&self) -> LyricsSource {
        LyricsSource::Netease
    }

    fn lookup(&self, query: &LyricsQuery, _track_path: Option<&Path>) -> SourceOutcome {
        if !self.breaker.can_attempt(HOST, self.now, self.force) {
            return SourceOutcome::Skipped;
        }
        let search = match self.fetcher.search(query) {
            FetchOutcome::Found(body) => body,
            FetchOutcome::NotFound => return self.not_found(),
            FetchOutcome::Failed(counts) => return self.failed(counts),
        };
        let id = match parse_search(&search, query) {
            Ok(Some(id)) => id,
            Ok(None) => return self.not_found(),
            Err(()) => return self.failed(false),
        };
        let lyric = match self.fetcher.lyric(id) {
            FetchOutcome::Found(body) => body,
            FetchOutcome::NotFound => return self.not_found(),
            FetchOutcome::Failed(counts) => return self.failed(counts),
        };
        match parse_lyric(&lyric) {
            Ok(Some(body)) => {
                self.breaker.record(HOST, BreakerOutcome::Success, self.now);
                SourceOutcome::Hit(LyricsHit {
                    body,
                    source: LyricsSource::Netease,
                })
            }
            Ok(None) => self.not_found(),
            Err(()) => self.failed(false),
        }
    }
}

pub(super) fn production_provider(now: i64, force: bool) -> NeteaseProvider<'static> {
    static FETCHER: ProductionFetcher = ProductionFetcher;
    NeteaseProvider::new(&FETCHER, &HOST_BREAKER, now, force)
}

#[derive(Deserialize)]
struct SearchEnvelope {
    result: Option<SearchResult>,
}

#[derive(Deserialize)]
struct SearchResult {
    #[serde(default)]
    songs: Vec<SearchSong>,
}

#[derive(Deserialize)]
struct SearchSong {
    id: u64,
    name: String,
    #[serde(default, alias = "ar")]
    artists: Vec<SearchArtist>,
    #[serde(default, alias = "dt")]
    duration: Option<i64>,
}

#[derive(Deserialize)]
struct SearchArtist {
    name: String,
}

#[derive(Deserialize)]
struct LyricEnvelope {
    #[serde(default)]
    nolyric: bool,
    lrc: Option<LyricText>,
    tlyric: Option<LyricText>,
    klyric: Option<LyricText>,
}

#[derive(Deserialize)]
struct LyricText {
    lyric: Option<String>,
}

fn parse_search(body: &str, query: &LyricsQuery) -> Result<Option<u64>, ()> {
    let response: SearchEnvelope = serde_json::from_str(body).map_err(|_| ())?;
    let title = normalized(&query.title);
    let artist = normalized(&query.artist);
    let duration = query.duration_ms.max(0);
    Ok(response
        .result
        .into_iter()
        .flat_map(|result| result.songs)
        .find(|song| {
            normalized(&song.name) == title
                && song
                    .artists
                    .iter()
                    .any(|candidate| normalized(&candidate.name) == artist)
                && song.duration.is_some_and(|candidate| {
                    candidate.saturating_sub(duration).abs() <= DURATION_TOLERANCE_MS
                })
        })
        .map(|song| song.id))
}

fn parse_lyric(body: &str) -> Result<Option<LyricsBody>, ()> {
    let response: LyricEnvelope = serde_json::from_str(body).map_err(|_| ())?;
    if response.nolyric {
        return Ok(Some(LyricsBody::Instrumental));
    }
    let text = [response.lrc, response.tlyric, response.klyric]
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.lyric)
        .find(|text| !text.trim().is_empty());
    let Some(text) = text else {
        return Ok(None);
    };
    let lines = parse_lrc(&text);
    if lines.is_empty() {
        Ok(Some(LyricsBody::Plain(text.trim().to_string())))
    } else {
        Ok(Some(LyricsBody::Synced(lines)))
    }
}

fn search_url(query: &LyricsQuery) -> Result<String, LyricsError> {
    if !query.has_required_metadata() {
        return Err(LyricsError::MissingMetadata);
    }
    let terms = format!(
        "{} {}",
        collapse_whitespace(&query.artist),
        collapse_whitespace(&query.title)
    );
    let mut url = url::Url::parse(SEARCH_URL).map_err(|_| LyricsError::InvalidResponse)?;
    url.query_pairs_mut()
        .append_pair("s", &terms)
        .append_pair("type", "1")
        .append_pair("limit", "5");
    Ok(url.into())
}

fn lyric_url(id: u64) -> Result<String, LyricsError> {
    let mut url = url::Url::parse(LYRIC_URL).map_err(|_| LyricsError::InvalidResponse)?;
    url.query_pairs_mut()
        .append_pair("id", &id.to_string())
        .append_pair("lv", "1")
        .append_pair("kv", "1")
        .append_pair("tv", "-1");
    Ok(url.into())
}

fn search_fixture_filename(query: &LyricsQuery) -> String {
    let terms = format!(
        "{} {}",
        collapse_whitespace(&query.artist),
        collapse_whitespace(&query.title)
    );
    format!(
        "netease-search-{}.json",
        crate::musicbrainz::urlencode(&terms)
    )
}

fn lyric_fixture_filename(id: u64) -> String {
    format!("netease-lyric-{id}.json")
}

impl NeteaseFetcher for FixtureFetcher<'_> {
    fn search(&self, query: &LyricsQuery) -> FetchOutcome {
        read_fixture(self.directory.join(search_fixture_filename(query)))
    }

    fn lyric(&self, id: u64) -> FetchOutcome {
        read_fixture(self.directory.join(lyric_fixture_filename(id)))
    }
}

impl NeteaseFetcher for ProductionFetcher {
    fn search(&self, query: &LyricsQuery) -> FetchOutcome {
        fixture_directory().map_or_else(
            || search_url(query).map_or(FetchOutcome::Failed(false), |url| fetch_url(&url)),
            |directory| FixtureFetcher::new(&directory).search(query),
        )
    }

    fn lyric(&self, id: u64) -> FetchOutcome {
        fixture_directory().map_or_else(
            || lyric_url(id).map_or(FetchOutcome::Failed(false), |url| fetch_url(&url)),
            |directory| FixtureFetcher::new(&directory).lyric(id),
        )
    }
}

fn fetch_url(url: &str) -> FetchOutcome {
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
        Err(ureq::Error::StatusCode(code)) => return FetchOutcome::Failed(code >= 500),
        Err(_) => return FetchOutcome::Failed(true),
    };
    crate::http_body::read_bounded_string(response.into_body().into_reader())
        .map_or(FetchOutcome::Failed(false), FetchOutcome::Found)
}

fn read_fixture(path: PathBuf) -> FetchOutcome {
    std::fs::File::open(path)
        .map_err(|_| ())
        .and_then(|file| crate::http_body::read_bounded_string(file).map_err(|_| ()))
        .map_or(FetchOutcome::Failed(false), FetchOutcome::Found)
}

fn fixture_directory() -> Option<PathBuf> {
    std::env::var(FIXTURE_DIR_ENV).ok().map(PathBuf::from)
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

fn normalized(value: &str) -> String {
    collapse_whitespace(value).to_lowercase()
}

#[cfg(test)]
#[path = "netease_tests.rs"]
mod tests;
