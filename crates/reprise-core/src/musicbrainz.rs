//! Shared blocking MusicBrainz HTTP boundary.
//!
//! Every MusicBrainz consumer goes through this module so the process-wide
//! one-request-per-second policy cannot accidentally diverge. Callers must
//! keep this work off the UI thread.

#[cfg(any(test, feature = "test-fixtures"))]
use std::fs::OpenOptions;
#[cfg(any(test, feature = "test-fixtures"))]
use std::io::Write;
#[cfg(any(test, feature = "test-fixtures"))]
use std::path::Path;
use std::time::Duration;
#[cfg(any(test, feature = "test-fixtures"))]
use std::time::{SystemTime, UNIX_EPOCH};

use crate::http_body::{self, BoundedReadError};
use crate::net::client::{build_agent, AgentPolicy};
use crate::net::rate::{wait_for_slot, RateLimitKey};

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(any(test, feature = "test-fixtures"))]
const FIXTURE_DIR_ENV: &str = "REPRISE_MUSICBRAINZ_FIXTURE_DIR";
#[cfg(any(test, feature = "test-fixtures"))]
const FIXTURE_LOG_ENV: &str = "REPRISE_MUSICBRAINZ_FIXTURE_LOG";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FetchError {
    #[error("MusicBrainz request timed out")]
    Timeout,
    #[error("MusicBrainz transport failed")]
    Transport,
    #[error("MusicBrainz returned HTTP status {0}")]
    HttpStatus(u16),
    #[error("MusicBrainz response body could not be read")]
    Body,
    #[error("MusicBrainz response body exceeds the size limit")]
    BodyTooLarge,
}

/// MusicBrainz answers are read through ureq's status errors.
pub(crate) const fn agent_policy() -> AgentPolicy {
    AgentPolicy::strict(HTTP_TIMEOUT)
}

/// Performs a blocking, rate-limited MusicBrainz GET.
pub fn get(url: &str) -> Result<String, FetchError> {
    let _ = wait_for_slot(RateLimitKey::MusicBrainz, &mut || false);
    #[cfg(any(test, feature = "test-fixtures"))]
    if let Ok(directory) = std::env::var(FIXTURE_DIR_ENV) {
        return fixture_get(url, Path::new(&directory));
    }
    let response = build_agent(agent_policy())
        .get(url)
        .call()
        .map_err(classify_error)?;
    http_body::read_bounded_string(response.into_body().into_reader()).map_err(map_body_error)
}

#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Debug, PartialEq, Eq)]
enum FixtureRequest {
    Artist(String),
    ReleaseGroups(String),
    NewReleases(String),
    ReleaseGroupDetail(String),
}

#[cfg(any(test, feature = "test-fixtures"))]
impl FixtureRequest {
    fn filename(&self) -> String {
        match self {
            Self::Artist(artist) => format!("artist-{artist}.json"),
            Self::ReleaseGroups(mbid) => format!("release-groups-{mbid}.json"),
            Self::NewReleases(mbid) => format!("new-releases-{mbid}.json"),
            Self::ReleaseGroupDetail(mbid) => format!("release-group-detail-{mbid}.json"),
        }
    }

    fn log_fields(&self) -> (&'static str, &str) {
        match self {
            Self::Artist(artist) => ("artist", artist),
            Self::ReleaseGroups(mbid) => ("release-group", mbid),
            Self::NewReleases(mbid) => ("new-releases", mbid),
            Self::ReleaseGroupDetail(mbid) => ("release-group-detail", mbid),
        }
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_request(url: &str) -> Option<FixtureRequest> {
    const ARTIST_PREFIX: &str = "query=artist%3A%22";
    if let Some(start) = url.find(ARTIST_PREFIX) {
        let value = &url[start + ARTIST_PREFIX.len()..];
        return value
            .split_once("%22")
            .map(|(artist, _)| FixtureRequest::Artist(artist.to_owned()));
    }
    const RELEASE_GROUP_DETAIL_PREFIX: &str = "https://musicbrainz.org/ws/2/release-group/";
    if let Some(value) = url.strip_prefix(RELEASE_GROUP_DETAIL_PREFIX) {
        return value
            .split_once('?')
            .map(|(mbid, _)| FixtureRequest::ReleaseGroupDetail(mbid.to_owned()));
    }
    if url.contains("/release-group?") {
        let value = url.split_once("artist=")?.1;
        let mbid = value.split_once('&').map_or(value, |(mbid, _)| mbid);
        if url.contains("type=album%7Cep%7Csingle") {
            return Some(FixtureRequest::NewReleases(mbid.to_owned()));
        }
        return Some(FixtureRequest::ReleaseGroups(mbid.to_owned()));
    }
    None
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_get(url: &str, directory: &Path) -> Result<String, FetchError> {
    let request = fixture_request(url).ok_or(FetchError::Transport)?;
    append_fixture_log(&request)?;
    let path = directory.join(request.filename());
    if let Ok(delay) = std::fs::read_to_string(path.with_extension("delay-ms")) {
        let millis = delay.trim().parse::<u64>().unwrap_or_default();
        std::thread::sleep(Duration::from_millis(millis));
    }
    let file = std::fs::File::open(path).map_err(|_| FetchError::Transport)?;
    http_body::read_bounded_string(file).map_err(map_body_error)
}

fn map_body_error(error: BoundedReadError) -> FetchError {
    match error {
        BoundedReadError::Read => FetchError::Body,
        BoundedReadError::TooLarge => FetchError::BodyTooLarge,
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
fn append_fixture_log(request: &FixtureRequest) -> Result<(), FetchError> {
    let Ok(path) = std::env::var(FIXTURE_LOG_ENV) else {
        return Ok(());
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let (kind, value) = request.log_fields();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| FetchError::Transport)?;
    writeln!(file, "{timestamp}\t{kind}\t{value}").map_err(|_| FetchError::Transport)
}

pub(crate) fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn classify_error(error: ureq::Error) -> FetchError {
    match error {
        ureq::Error::StatusCode(status) => FetchError::HttpStatus(status),
        ureq::Error::Timeout(_) => FetchError::Timeout,
        other => {
            let message = other.to_string().to_ascii_lowercase();
            if message.contains("timed out") || message.contains("timeout") {
                FetchError::Timeout
            } else {
                FetchError::Transport
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_policy_surfaces_statuses_as_errors() {
        assert_eq!(
            agent_policy(),
            AgentPolicy {
                timeout: Duration::from_secs(15),
                status_as_error: true,
                https_only: false,
                max_redirects: None,
                proxy_from_env: true,
            }
        );
    }

    #[test]
    fn oversized_fixture_body_is_rejected() {
        let fixtures = tempfile::tempdir().unwrap();
        std::fs::write(
            fixtures.path().join("artist-Oversized.json"),
            vec![b'x'; crate::http_body::MAX_JSON_RESPONSE_BYTES as usize + 1],
        )
        .unwrap();

        assert_eq!(
            fixture_get(
                "https://musicbrainz.org/ws/2/artist?query=artist%3A%22Oversized%22&fmt=json",
                fixtures.path()
            ),
            Err(FetchError::BodyTooLarge)
        );
    }

    #[test]
    fn fixture_routes_expose_only_artist_or_mbid_fields() {
        assert_eq!(
            fixture_request(
                "https://musicbrainz.org/ws/2/artist/?query=artist%3A%22Artist%20Alpha%22&fmt=json&limit=5"
            ),
            Some(FixtureRequest::Artist("Artist%20Alpha".into()))
        );
        assert_eq!(
            fixture_request(
                "https://musicbrainz.org/ws/2/release-group?artist=aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa&type=album"
            ),
            Some(FixtureRequest::ReleaseGroups(
                "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into()
            ))
        );
        assert_eq!(
            fixture_request(
                "https://musicbrainz.org/ws/2/release-group/release-id?inc=releases%2Bmedia&fmt=json"
            ),
            Some(FixtureRequest::ReleaseGroupDetail("release-id".into()))
        );
        assert_eq!(fixture_request("https://example.test/private/path"), None);
    }

    #[test]
    fn new_releases_endpoint_has_a_dedicated_fixture_route() {
        let url = "https://musicbrainz.org/ws/2/release-group?artist=aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa&type=album%7Cep%7Csingle&release-group-status=website-default&limit=100&fmt=json";
        assert_eq!(
            fixture_request(url),
            Some(FixtureRequest::NewReleases(
                "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into()
            ))
        );
        assert_eq!(
            FixtureRequest::NewReleases("artist-id".into()).filename(),
            "new-releases-artist-id.json"
        );
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory
                .path()
                .join("new-releases-aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa.json"),
            r#"{"release-groups":[]}"#,
        )
        .unwrap();
        assert_eq!(
            fixture_get(url, directory.path()).unwrap(),
            r#"{"release-groups":[]}"#
        );
    }

    #[test]
    fn new_releases_url_rels_extension_keeps_the_fixture_route() {
        // NR-11 [geplant]: `release_groups_url` now asks MusicBrainz for
        // url-rels too. The fixture matcher keys off `type=album%7Cep%7C
        // single`, which must survive the `inc=url-rels` addition or every
        // fixture-backed New Releases test would silently fall back to the
        // generic `ReleaseGroups` route.
        let url = crate::artist_news::release_groups_url("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
        assert!(url.contains("inc=url-rels"));
        assert_eq!(
            fixture_request(&url),
            Some(FixtureRequest::NewReleases(
                "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into()
            ))
        );
    }

    #[test]
    fn fixture_get_reads_the_routed_response() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("artist-Artist%20Alpha.json"),
            r#"{"artists":[]}"#,
        )
        .unwrap();
        assert_eq!(
            fixture_get(
                "https://musicbrainz.org/ws/2/artist/?query=artist%3A%22Artist%20Alpha%22&fmt=json&limit=5",
                directory.path()
            )
            .unwrap(),
            r#"{"artists":[]}"#
        );
    }
}
