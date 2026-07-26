//! Shared blocking MusicBrainz HTTP boundary.
//!
//! Every MusicBrainz consumer goes through this module so the process-wide
//! one-request-per-second policy cannot accidentally diverge. Callers must
//! keep this work off the UI thread.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::http_body::{self, BoundedReadError};

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(1);
const FIXTURE_DIR_ENV: &str = "REPRISE_MUSICBRAINZ_FIXTURE_DIR";
const FIXTURE_LOG_ENV: &str = "REPRISE_MUSICBRAINZ_FIXTURE_LOG";

pub const CONTACT_URL: &str = "https://github.com/marvinbaudach";

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

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

pub fn user_agent() -> String {
    format!("Reprise/{} ( {CONTACT_URL} )", env!("CARGO_PKG_VERSION"))
}

/// Performs a blocking, rate-limited MusicBrainz GET.
pub fn get(url: &str) -> Result<String, FetchError> {
    respect_rate_limit();
    if let Ok(directory) = std::env::var(FIXTURE_DIR_ENV) {
        return fixture_get(url, Path::new(&directory));
    }
    let response = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(user_agent())
        .build()
        .new_agent()
        .get(url)
        .call()
        .map_err(classify_error)?;
    http_body::read_bounded_string(response.into_body().into_reader()).map_err(map_body_error)
}

#[derive(Debug, PartialEq, Eq)]
enum FixtureRequest {
    Artist(String),
    ReleaseGroups(String),
    NewReleases(String),
}

impl FixtureRequest {
    fn filename(&self) -> String {
        match self {
            Self::Artist(artist) => format!("artist-{artist}.json"),
            Self::ReleaseGroups(mbid) => format!("release-groups-{mbid}.json"),
            Self::NewReleases(mbid) => format!("new-releases-{mbid}.json"),
        }
    }

    fn log_fields(&self) -> (&'static str, &str) {
        match self {
            Self::Artist(artist) => ("artist", artist),
            Self::ReleaseGroups(mbid) => ("release-group", mbid),
            Self::NewReleases(mbid) => ("new-releases", mbid),
        }
    }
}

fn fixture_request(url: &str) -> Option<FixtureRequest> {
    const ARTIST_PREFIX: &str = "query=artist%3A%22";
    if let Some(start) = url.find(ARTIST_PREFIX) {
        let value = &url[start + ARTIST_PREFIX.len()..];
        return value
            .split_once("%22")
            .map(|(artist, _)| FixtureRequest::Artist(artist.to_owned()));
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

fn request_delay(previous: Option<Instant>, now: Instant) -> Duration {
    let Some(previous) = previous else {
        return Duration::ZERO;
    };
    MIN_REQUEST_INTERVAL.saturating_sub(now.saturating_duration_since(previous))
}

fn respect_rate_limit() {
    let _ = wait_for_request_slot(&mut || false);
}

/// Shares MusicBrainz's process-wide request slot with cancellable jobs.
/// Returns `false` when cancellation happens before the slot is acquired.
pub(crate) fn wait_for_request_slot(cancelled: &mut dyn FnMut() -> bool) -> bool {
    const SLICE: Duration = Duration::from_millis(50);
    let mut previous = lock_unpoisoned(&LAST_REQUEST);
    let mut delay = request_delay(*previous, Instant::now());
    while !delay.is_zero() {
        if cancelled() {
            return false;
        }
        let slice = delay.min(SLICE);
        std::thread::sleep(slice);
        delay = delay.saturating_sub(slice);
    }
    if cancelled() {
        return false;
    }
    *previous = Some(Instant::now());
    true
}

#[cfg(test)]
fn respect_rate_limit_with<N, S>(limiter: &Mutex<Option<Instant>>, now: &mut N, sleep: &mut S)
where
    N: FnMut() -> Instant,
    S: FnMut(Duration),
{
    let mut previous = lock_unpoisoned(limiter);
    let delay = request_delay(*previous, now());
    if !delay.is_zero() {
        sleep(delay);
    }
    *previous = Some(now());
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[test]
    fn user_agent_identifies_version_and_maintainer() {
        let value = user_agent();
        assert!(value.contains(env!("CARGO_PKG_VERSION")));
        assert!(value.contains("https://github.com/marvinbaudach"));
    }

    #[test]
    fn request_delay_enforces_one_second_interval() {
        let now = Instant::now();
        assert_eq!(request_delay(None, now), Duration::ZERO);
        assert_eq!(
            request_delay(Some(now - Duration::from_millis(250)), now),
            Duration::from_millis(750)
        );
        assert_eq!(
            request_delay(Some(now - Duration::from_secs(2)), now),
            Duration::ZERO
        );
    }

    #[test]
    fn nr_1a_fetch_respects_rate_limit() {
        let base = Instant::now();
        let elapsed = Cell::new(Duration::ZERO);
        let slept = Cell::new(Duration::ZERO);
        let limiter = Mutex::new(None);
        let mut now = || base + elapsed.get();
        let mut sleep = |duration| {
            slept.set(slept.get() + duration);
            elapsed.set(elapsed.get() + duration);
        };

        respect_rate_limit_with(&limiter, &mut now, &mut sleep);
        elapsed.set(Duration::from_millis(250));
        respect_rate_limit_with(&limiter, &mut now, &mut sleep);

        assert_eq!(slept.get(), Duration::from_millis(750));
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
    fn poisoned_limiter_mutex_is_recovered() {
        let mutex = Mutex::new(7_u8);
        let _ = std::panic::catch_unwind(|| {
            let _guard = mutex.lock().unwrap();
            panic!("poison test mutex");
        });
        assert_eq!(*lock_unpoisoned(&mutex), 7);
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
