//! Blocking HTTP boundary for podcast feeds and Apple Podcasts search.
//!
//! Callers must keep this boundary off the UI thread. The process-wide limiter
//! applies equally to search, preview, and refresh requests.

#[cfg(not(any(test, feature = "test-fixtures")))]
use std::path::Path;
#[cfg(any(test, feature = "test-fixtures"))]
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[cfg(any(test, feature = "test-fixtures"))]
use url::Url;

use super::PodcastError;
use crate::http_body::{self, BoundedReadError};

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(any(test, feature = "test-fixtures"))]
const FIXTURE_DIR_ENV: &str = "REPRISE_PODCASTS_FIXTURE_DIR";

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

#[cfg(test)]
thread_local! {
    static TEST_FIXTURE_DIR: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub body: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FixtureRoute {
    AppleSearch(String),
    Feed(String),
    Download(String),
}

impl FixtureRoute {
    #[cfg(any(test, feature = "test-fixtures"))]
    fn filename(&self) -> String {
        match self {
            Self::AppleSearch(terms) => {
                format!("itunes-search-{}.json", fixture_component(terms))
            }
            Self::Feed(url) => format!("feed-{}.xml", fixture_component(url)),
            Self::Download(url) => format!("download-{}", fixture_component(url)),
        }
    }
}

#[must_use]
pub fn user_agent() -> String {
    format!(
        "Reprise/{} ( {} )",
        env!("CARGO_PKG_VERSION"),
        crate::musicbrainz::CONTACT_URL
    )
}

pub fn get(url: &str) -> Result<Response, PodcastError> {
    get_conditional(url, None, None)
}

pub fn get_conditional(
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<Response, PodcastError> {
    respect_rate_limit();
    #[cfg(any(test, feature = "test-fixtures"))]
    if let Some(directory) = fixture_directory() {
        return fixture_get(url, etag, last_modified, &directory);
    }

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(user_agent())
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut request = agent.get(url);
    if let Some(value) = etag {
        request = request.header("If-None-Match", value);
    }
    if let Some(value) = last_modified {
        request = request.header("If-Modified-Since", value);
    }
    let response = request.call().map_err(classify_transport)?;
    let status = response.status().as_u16();
    if status == 304 {
        return Err(PodcastError::NotModified);
    }
    if !(200..300).contains(&status) {
        return Err(PodcastError::HttpStatus(status));
    }
    let response_etag = header(&response, "ETag");
    let response_last_modified = header(&response, "Last-Modified");
    let body = http_body::read_bounded_string(response.into_body().into_reader())
        .map_err(map_body_error)?;
    Ok(Response {
        body,
        etag: response_etag,
        last_modified: response_last_modified,
    })
}

pub fn download(url: &str, destination: &Path) -> Result<(), PodcastError> {
    respect_rate_limit();
    #[cfg(any(test, feature = "test-fixtures"))]
    if let Some(directory) = fixture_directory() {
        return fixture_download(url, destination, &directory);
    }
    let response = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(user_agent())
        .http_status_as_error(false)
        .build()
        .new_agent()
        .get(url)
        .call()
        .map_err(classify_transport)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(PodcastError::HttpStatus(status));
    }
    let mut reader = response.into_body().into_reader();
    let mut file = std::fs::File::create(destination)
        .map_err(|error| PodcastError::Body(error.to_string()))?;
    std::io::copy(&mut reader, &mut file).map_err(|error| PodcastError::Body(error.to_string()))?;
    Ok(())
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_download(url: &str, destination: &Path, directory: &Path) -> Result<(), PodcastError> {
    std::fs::copy(
        directory.join(FixtureRoute::Download(url.to_owned()).filename()),
        destination,
    )
    .map(|_| ())
    .map_err(|error| PodcastError::Transport(error.to_string()))
}

fn header(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_directory() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(directory) = TEST_FIXTURE_DIR.with(|slot| slot.borrow().clone()) {
        return Some(directory);
    }
    std::env::var(FIXTURE_DIR_ENV).ok().map(PathBuf::from)
}

#[cfg(test)]
pub(crate) fn with_fixture_dir<T>(directory: &Path, operation: impl FnOnce() -> T) -> T {
    struct Reset(Option<PathBuf>);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_FIXTURE_DIR.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let previous = TEST_FIXTURE_DIR.with(|slot| slot.borrow_mut().replace(directory.to_path_buf()));
    let _reset = Reset(previous);
    operation()
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_route(value: &str) -> Option<FixtureRoute> {
    let url = Url::parse(value).ok()?;
    if url.host_str() == Some("itunes.apple.com") {
        let terms = url
            .query_pairs()
            .find_map(|(key, value)| (key == "term").then(|| value.into_owned()))?;
        Some(FixtureRoute::AppleSearch(terms))
    } else if matches!(url.scheme(), "http" | "https") {
        Some(FixtureRoute::Feed(value.to_owned()))
    } else {
        None
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_get(
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
    directory: &Path,
) -> Result<Response, PodcastError> {
    let route = fixture_route(url)
        .ok_or_else(|| PodcastError::Transport("no fixture route for request".to_owned()))?;
    let path = directory.join(route.filename());
    let status_path = path.with_extension("status");
    if std::fs::read_to_string(status_path)
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok())
        == Some(304)
    {
        return Err(PodcastError::NotModified);
    }
    let stored_etag = read_sidecar(&path, "etag");
    let stored_last_modified = read_sidecar(&path, "last-modified");
    let unchanged = match etag {
        Some(etag) => stored_etag.as_deref() == Some(etag),
        None => last_modified.is_some() && stored_last_modified.as_deref() == last_modified,
    };
    if unchanged {
        return Err(PodcastError::NotModified);
    }
    let file =
        std::fs::File::open(path).map_err(|error| PodcastError::Transport(error.to_string()))?;
    let body = http_body::read_bounded_string(file).map_err(map_body_error)?;
    Ok(Response {
        body,
        etag: stored_etag,
        last_modified: stored_last_modified,
    })
}

#[cfg(any(test, feature = "test-fixtures"))]
fn read_sidecar(path: &Path, extension: &str) -> Option<String> {
    std::fs::read_to_string(path.with_extension(extension))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn map_body_error(error: BoundedReadError) -> PodcastError {
    match error {
        BoundedReadError::Read => PodcastError::Body("response read failed".to_owned()),
        BoundedReadError::TooLarge => PodcastError::Body("response is too large".to_owned()),
    }
}

fn classify_transport(error: ureq::Error) -> PodcastError {
    match error {
        ureq::Error::Timeout(_) => PodcastError::Timeout,
        other if other.to_string().to_ascii_lowercase().contains("timeout") => {
            PodcastError::Timeout
        }
        other => PodcastError::Transport(other.to_string()),
    }
}

fn respect_rate_limit() {
    let mut previous = lock_unpoisoned(&LAST_REQUEST);
    let delay = previous.map_or(Duration::ZERO, |instant| {
        MIN_REQUEST_INTERVAL.saturating_sub(instant.elapsed())
    });
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
    *previous = Some(Instant::now());
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_component(value: &str) -> String {
    let component = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if component.len() <= 160 {
        component
    } else {
        format!(
            "{}-{:016x}",
            &component[..140],
            crate::artist_news_refresh::fnv1a_64(value.as_bytes())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_identifies_reprise_and_contact() {
        let value = user_agent();
        assert!(value.contains(env!("CARGO_PKG_VERSION")));
        assert!(value.contains(crate::musicbrainz::CONTACT_URL));
    }

    #[test]
    fn fixture_route_distinguishes_search_and_feed_requests() {
        assert_eq!(
            fixture_route("https://itunes.apple.com/search?media=podcast&term=rust%20audio"),
            Some(FixtureRoute::AppleSearch("rust audio".to_owned()))
        );
        assert_eq!(
            fixture_route("https://feeds.example.test/show.xml"),
            Some(FixtureRoute::Feed(
                "https://feeds.example.test/show.xml".to_owned()
            ))
        );
    }

    #[test]
    fn conditional_fixture_returns_headers_then_not_modified() {
        let directory = tempfile::tempdir().unwrap();
        let url = "https://feeds.example.test/show.xml";
        let route = fixture_route(url).unwrap();
        let path = directory.path().join(route.filename());
        std::fs::write(&path, "<rss/>").unwrap();
        std::fs::write(path.with_extension("etag"), "\"v1\"").unwrap();

        with_fixture_dir(directory.path(), || {
            let first = get(url).unwrap();
            assert_eq!(first.etag.as_deref(), Some("\"v1\""));
            assert!(matches!(
                get_conditional(url, first.etag.as_deref(), None),
                Err(PodcastError::NotModified)
            ));
        });
    }

    #[test]
    fn fixture_download_copies_audio_without_network_access() {
        let directory = tempfile::tempdir().unwrap();
        let destination_directory = tempfile::tempdir().unwrap();
        let url = "https://media.example.test/episode.mp3";
        std::fs::write(
            directory
                .path()
                .join(FixtureRoute::Download(url.to_owned()).filename()),
            b"fixture audio",
        )
        .unwrap();
        let destination = destination_directory.path().join("episode.mp3");

        with_fixture_dir(directory.path(), || {
            download(url, &destination).unwrap();
        });

        assert_eq!(std::fs::read(destination).unwrap(), b"fixture audio");
    }
}
