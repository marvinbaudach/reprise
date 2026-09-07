//! radio-browser HTTP boundary.

#[cfg(any(test, feature = "test-fixtures"))]
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(any(test, feature = "test-fixtures"))]
use url::Url;

use super::{RadioError, RadioFailureDetail};
use crate::http_body::{self, BoundedReadError};
use crate::source_error::{parse_retry_after, SOURCE_REQUEST_TIMEOUT};
#[cfg(test)]
use crate::sources_http::user_agent;
use crate::sources_http::{build_agent, lock_unpoisoned};

pub const HTTP_TIMEOUT: Duration = SOURCE_REQUEST_TIMEOUT;
pub const CLICK_TIMEOUT: Duration = Duration::from_secs(5);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(any(test, feature = "test-fixtures"))]
const FIXTURE_DIR_ENV: &str = "REPRISE_RADIO_FIXTURE_DIR";

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

pub fn get(url: &str) -> Result<String, RadioError> {
    get_with_timeout(url, HTTP_TIMEOUT)
}

pub fn get_with_timeout(url: &str, timeout: Duration) -> Result<String, RadioError> {
    #[cfg(any(test, feature = "test-fixtures"))]
    if let Some(directory) = fixture_directory() {
        return fixture_get(url, &directory);
    }
    wait_for_request_slot();
    let response = build_agent(timeout)
        .get(url)
        .call()
        .map_err(classify_transport)?;
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("Retry-After")
        .and_then(|value| value.to_str().ok());
    if let Some(error) = source_status_error(status, retry_after) {
        return Err(error);
    }
    http_body::read_bounded_string(response.into_body().into_reader()).map_err(map_body_error)
}

pub fn icy_headers(url: &str) -> Result<Vec<(String, String)>, RadioError> {
    #[cfg(any(test, feature = "test-fixtures"))]
    if let Some(directory) = fixture_directory() {
        return fixture_icy_headers(url, &directory);
    }
    wait_for_request_slot();
    let response = build_agent(HTTP_TIMEOUT)
        .get(url)
        .header("Icy-MetaData", "1")
        .call()
        .map_err(classify_transport)?;
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("Retry-After")
        .and_then(|value| value.to_str().ok());
    if let Some(error) = source_status_error(status, retry_after) {
        return Err(error);
    }
    Ok(response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_owned(), value.to_owned()))
        })
        .collect())
}

fn source_status_error(status: u16, retry_after: Option<&str>) -> Option<RadioError> {
    match status {
        200..=299 => None,
        404 | 410 => Some(RadioError::Unavailable(RadioFailureDetail::SourceGone(
            status,
        ))),
        429 => Some(RadioError::Unavailable(RadioFailureDetail::RateLimited {
            retry_after: parse_retry_after(retry_after),
        })),
        _ => Some(RadioError::HttpStatus(status)),
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
/// Resolves a scoped radio fixture directory before the environment fallback.
/// The fallback keeps feature-enabled fixture consumers independent of tests.
fn fixture_directory() -> Option<PathBuf> {
    crate::sources_http::fixture_directory(FIXTURE_DIR_ENV)
}

#[cfg(test)]
/// Installs a radio fixture directory and invalidates cached server discovery.
/// The shared scope restores a nested fixture directory even if the operation
/// unwinds.
pub(crate) fn with_fixture_dir<T>(directory: &Path, operation: impl FnOnce() -> T) -> T {
    fn reset_source_state() {
        // The shared scope calls this before installation and after restoration.
        super::servers::reset_cache_for_tests();
    }

    crate::sources_http::with_fixture_dir(directory, reset_source_state, operation)
}

#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum FixtureRequest {
    Servers,
    Search(String),
    Click(String),
    ByUrl(String),
    Stream(String),
}

#[cfg(any(test, feature = "test-fixtures"))]
impl FixtureRequest {
    fn filename(&self) -> String {
        match self {
            Self::Servers => "servers.json".into(),
            Self::Search(term) => format!("search-{}.json", fixture_component(term)),
            Self::Click(uuid) => format!("click-{}.json", fixture_component(uuid)),
            Self::ByUrl(url) => format!("byurl-{}.json", fixture_component(url)),
            Self::Stream(url) => format!("stream-{}.body", fixture_component(url)),
        }
    }

    fn headers_filename(&self) -> Option<String> {
        match self {
            Self::Stream(url) => Some(format!("stream-{}.headers.json", fixture_component(url))),
            _ => None,
        }
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_request(value: &str) -> Option<FixtureRequest> {
    let url = Url::parse(value).ok()?;
    let segments = url.path_segments()?.collect::<Vec<_>>();
    if url.host_str() == Some("all.api.radio-browser.info")
        && segments.as_slice() == ["json", "servers"]
    {
        return Some(FixtureRequest::Servers);
    }
    if segments.get(..3) == Some(&["json", "stations", "search"]) {
        // `RAD-5`'s chip searches (`radio::search::search_by`) key the same
        // `/stations/search` route by `tag`/`countrycode` instead of `name`
        // — free-text search still wins when present, but a criteria-only
        // request (including the deliberately unfiltered "Top voted") still
        // needs a stable fixture key.
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        let by_key = |key: &str| {
            pairs
                .iter()
                .find(|(pair_key, _)| pair_key == key)
                .map(|(_, value)| value.clone())
        };
        if let Some(name) = by_key("name") {
            return Some(FixtureRequest::Search(name));
        }
        let mut parts = Vec::new();
        if let Some(tag) = by_key("tag") {
            parts.push(format!("tag-{tag}"));
        }
        if let Some(country_code) = by_key("countrycode") {
            parts.push(format!("country-{country_code}"));
        }
        let key = if parts.is_empty() {
            "broad".to_owned()
        } else {
            parts.join("-")
        };
        return Some(FixtureRequest::Search(key));
    }
    if segments.get(..2) == Some(&["json", "url"]) {
        return segments
            .get(2)
            .map(|uuid| FixtureRequest::Click((*uuid).into()));
    }
    if segments.get(..3) == Some(&["json", "stations", "byurl"]) {
        return url
            .query_pairs()
            .find_map(|(key, value)| (key == "url").then(|| value.into_owned()))
            .map(FixtureRequest::ByUrl);
    }
    matches!(url.scheme(), "http" | "https").then(|| FixtureRequest::Stream(value.to_owned()))
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_get(url: &str, directory: &Path) -> Result<String, RadioError> {
    let request = fixture_request(url)
        .ok_or_else(|| RadioError::Transport("unsupported fixture route".into()))?;
    let file = std::fs::File::open(directory.join(request.filename()))
        .map_err(|error| RadioError::Transport(error.to_string()))?;
    http_body::read_bounded_string(file).map_err(map_body_error)
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_icy_headers(url: &str, directory: &Path) -> Result<Vec<(String, String)>, RadioError> {
    let filename = fixture_request(url)
        .and_then(|request| request.headers_filename())
        .ok_or_else(|| RadioError::Transport("unsupported fixture route".into()))?;
    let file = std::fs::File::open(directory.join(filename))
        .map_err(|error| RadioError::Transport(error.to_string()))?;
    let body = http_body::read_bounded_string(file).map_err(map_body_error)?;
    serde_json::from_str(&body).map_err(|error| RadioError::Transport(error.to_string()))
}

fn wait_for_request_slot() {
    let mut previous = lock_unpoisoned(&LAST_REQUEST);
    if let Some(last) = *previous {
        std::thread::sleep(MIN_REQUEST_INTERVAL.saturating_sub(last.elapsed()));
    }
    *previous = Some(Instant::now());
}

fn classify_transport(error: ureq::Error) -> RadioError {
    match error {
        ureq::Error::Timeout(_) => RadioError::Timeout,
        other if other.to_string().to_ascii_lowercase().contains("timeout") => RadioError::Timeout,
        other => RadioError::Transport(other.to_string()),
    }
}

fn map_body_error(error: BoundedReadError) -> RadioError {
    RadioError::Body(match error {
        BoundedReadError::Read => "response could not be decoded".into(),
        BoundedReadError::TooLarge => "response exceeded the size limit".into(),
    })
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_error::SourceErrorKind;

    #[test]
    fn fixture_routes_cover_discovery_search_and_click() {
        let expected = [
            (
                "https://all.api.radio-browser.info/json/servers",
                "servers.json",
            ),
            (
                "https://de1.api.radio-browser.info/json/stations/search?name=deep+house",
                "search-deep_house.json",
            ),
            (
                "https://de1.api.radio-browser.info/json/url/abc-123",
                "click-abc-123.json",
            ),
            (
                "https://de1.api.radio-browser.info/json/stations/byurl?url=https%3A%2F%2Fradio.example%2Flive",
                "byurl-https___radio.example_live.json",
            ),
        ];
        for (url, filename) in expected {
            assert_eq!(fixture_request(url).unwrap().filename(), filename);
        }
    }

    #[test]
    fn user_agent_identifies_reprise_and_contact() {
        let value = user_agent();
        assert!(value.starts_with("Reprise/"));
        assert!(value.contains(crate::musicbrainz::CONTACT_URL));
    }

    #[test]
    fn source_statuses_distinguish_gone_rate_limited_and_transient_failures() {
        for status in [404, 410] {
            let error = source_status_error(status, None).unwrap();
            assert!(matches!(
                error,
                RadioError::Unavailable(RadioFailureDetail::SourceGone(value)) if value == status
            ));
            assert_eq!(SourceErrorKind::from(&error), SourceErrorKind::SourceGone);
        }
        let error = source_status_error(429, Some("360")).unwrap();
        assert!(matches!(
            SourceErrorKind::from(&error),
            SourceErrorKind::RateLimited {
                retry_after: Some(value)
            } if value == Duration::from_secs(360)
        ));
        assert!(matches!(
            source_status_error(500, None),
            Some(RadioError::HttpStatus(500))
        ));
        assert!(source_status_error(204, None).is_none());
    }

    #[test]
    fn feed_and_search_requests_use_the_shared_ten_second_budget() {
        assert_eq!(HTTP_TIMEOUT, Duration::from_secs(10));
        assert_eq!(HTTP_TIMEOUT, crate::source_error::SOURCE_REQUEST_TIMEOUT);
    }
}
