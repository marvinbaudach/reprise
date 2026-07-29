//! radio-browser HTTP boundary.

#[cfg(any(test, feature = "test-fixtures"))]
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[cfg(any(test, feature = "test-fixtures"))]
use url::Url;

use super::RadioError;
use crate::http_body::{self, BoundedReadError};

pub const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
pub const CLICK_TIMEOUT: Duration = Duration::from_secs(5);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(any(test, feature = "test-fixtures"))]
const FIXTURE_DIR_ENV: &str = "REPRISE_RADIO_FIXTURE_DIR";

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

#[cfg(test)]
thread_local! {
    static TEST_FIXTURE_DIR: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
}

#[must_use]
pub fn user_agent() -> String {
    format!(
        "Reprise/{} ( {} )",
        env!("CARGO_PKG_VERSION"),
        crate::musicbrainz::CONTACT_URL
    )
}

pub fn get(url: &str) -> Result<String, RadioError> {
    get_with_timeout(url, HTTP_TIMEOUT)
}

pub fn get_with_timeout(url: &str, timeout: Duration) -> Result<String, RadioError> {
    #[cfg(any(test, feature = "test-fixtures"))]
    if let Some(directory) = fixture_directory() {
        return fixture_get(url, &directory);
    }
    wait_for_request_slot();
    let response = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(user_agent())
        .http_status_as_error(false)
        .build()
        .new_agent()
        .get(url)
        .call()
        .map_err(classify_transport)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(RadioError::HttpStatus(status));
    }
    http_body::read_bounded_string(response.into_body().into_reader()).map_err(map_body_error)
}

pub fn icy_headers(url: &str) -> Result<Vec<(String, String)>, RadioError> {
    wait_for_request_slot();
    let response = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(user_agent())
        .http_status_as_error(false)
        .build()
        .new_agent()
        .get(url)
        .header("Icy-MetaData", "1")
        .call()
        .map_err(classify_transport)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(RadioError::HttpStatus(status));
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
            super::servers::reset_cache_for_tests();
        }
    }
    super::servers::reset_cache_for_tests();
    let previous = TEST_FIXTURE_DIR.with(|slot| slot.borrow_mut().replace(directory.to_path_buf()));
    let _reset = Reset(previous);
    operation()
}

#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum FixtureRequest {
    Servers,
    Search(String),
    Click(String),
    ByUrl(String),
}

#[cfg(any(test, feature = "test-fixtures"))]
impl FixtureRequest {
    fn filename(&self) -> String {
        match self {
            Self::Servers => "servers.json".into(),
            Self::Search(term) => format!("search-{}.json", fixture_component(term)),
            Self::Click(uuid) => format!("click-{}.json", fixture_component(uuid)),
            Self::ByUrl(url) => format!("byurl-{}.json", fixture_component(url)),
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
    None
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_get(url: &str, directory: &Path) -> Result<String, RadioError> {
    let request = fixture_request(url)
        .ok_or_else(|| RadioError::Transport("unsupported fixture route".into()))?;
    let file = std::fs::File::open(directory.join(request.filename()))
        .map_err(|error| RadioError::Transport(error.to_string()))?;
    http_body::read_bounded_string(file).map_err(map_body_error)
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

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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
}
