#[cfg(any(test, feature = "test-fixtures"))]
use std::fs::OpenOptions;
#[cfg(any(test, feature = "test-fixtures"))]
use std::io::Write;
#[cfg(any(test, feature = "test-fixtures"))]
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
#[cfg(any(test, feature = "test-fixtures"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(any(test, feature = "test-fixtures"))]
use url::Url;

use super::ProviderError;
use crate::http_body::{self, BoundedReadError};

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(any(test, feature = "test-fixtures"))]
const FIXTURE_DIR_ENV: &str = "REPRISE_CONCERTS_FIXTURE_DIR";
#[cfg(any(test, feature = "test-fixtures"))]
const FIXTURE_LOG_ENV: &str = "REPRISE_CONCERTS_FIXTURE_LOG";

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

#[cfg(test)]
thread_local! {
    static TEST_FIXTURE_DIR: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
}

pub fn user_agent() -> String {
    format!(
        "Reprise/{} ( {} )",
        env!("CARGO_PKG_VERSION"),
        crate::musicbrainz::CONTACT_URL
    )
}

pub fn get(url: &str) -> Result<String, ProviderError> {
    let _ = wait_for_request_slot(&mut || false);
    #[cfg(any(test, feature = "test-fixtures"))]
    if let Some(directory) = fixture_directory() {
        return fixture_get(url, &directory);
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
    if status == 429 {
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse().ok());
        return Err(ProviderError::RateLimited { retry_after });
    }
    if !(200..300).contains(&status) {
        return Err(ProviderError::HttpStatus(status));
    }
    http_body::read_bounded_string(response.into_body().into_reader()).map_err(map_body_error)
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
#[derive(Clone, Debug, PartialEq, Eq)]
enum FixtureRequest {
    BandsintownArtist(String),
    BandsintownEvents(String),
    TicketmasterAttractions(String),
    TicketmasterEvents(String),
    Nominatim(String),
    ListenBrainzSimilar(String),
    LastfmSimilar(String),
}

#[cfg(any(test, feature = "test-fixtures"))]
impl FixtureRequest {
    fn filename(&self) -> String {
        let (prefix, value) = match self {
            Self::BandsintownArtist(value) => ("bandsintown-artist", value),
            Self::BandsintownEvents(value) => ("bandsintown-events", value),
            Self::TicketmasterAttractions(value) => ("ticketmaster-attractions", value),
            Self::TicketmasterEvents(value) => ("ticketmaster-events", value),
            Self::Nominatim(value) => ("nominatim", value),
            Self::ListenBrainzSimilar(value) => ("listenbrainz-similar", value),
            Self::LastfmSimilar(value) => ("lastfm-similar", value),
        };
        format!("{prefix}-{}.json", fixture_component(value))
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_request(value: &str) -> Option<FixtureRequest> {
    let url = Url::parse(value).ok()?;
    let query = |name: &str| {
        url.query_pairs()
            .find_map(|(key, value)| (key == name).then(|| value.into_owned()))
    };
    let segments = url.path_segments()?.collect::<Vec<_>>();
    match url.host_str()? {
        "rest.bandsintown.com" => {
            let artist = segments.get(1).map(|value| percent_decode(value))?;
            if segments.get(2).is_some_and(|segment| *segment == "events") {
                Some(FixtureRequest::BandsintownEvents(artist))
            } else {
                Some(FixtureRequest::BandsintownArtist(artist))
            }
        }
        "app.ticketmaster.com" if segments.last() == Some(&"attractions.json") => {
            Some(FixtureRequest::TicketmasterAttractions(query("keyword")?))
        }
        "app.ticketmaster.com" if segments.last() == Some(&"events.json") => {
            Some(FixtureRequest::TicketmasterEvents(query("attractionId")?))
        }
        "nominatim.openstreetmap.org" => Some(FixtureRequest::Nominatim(query("q")?)),
        "labs.api.listenbrainz.org" => {
            Some(FixtureRequest::ListenBrainzSimilar(query("artist_mbids")?))
        }
        "ws.audioscrobbler.com" => Some(FixtureRequest::LastfmSimilar(query("artist")?)),
        _ => None,
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_get(url: &str, directory: &Path) -> Result<String, ProviderError> {
    let request = fixture_request(url).ok_or(ProviderError::Transport)?;
    append_fixture_log(&request)?;
    let filename = request.filename();
    if let Some(error) = fixture_status(directory.join(format!("{filename}.status")).as_path())? {
        return Err(error);
    }
    let file =
        std::fs::File::open(directory.join(filename)).map_err(|_| ProviderError::Transport)?;
    http_body::read_bounded_string(file).map_err(map_body_error)
}

#[cfg(any(test, feature = "test-fixtures"))]
fn fixture_status(path: &Path) -> Result<Option<ProviderError>, ProviderError> {
    let value = match std::fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(_) => return Err(ProviderError::Transport),
    };
    let value = value.trim();
    if value.eq_ignore_ascii_case("timeout") {
        return Ok(Some(ProviderError::Timeout));
    }
    if value.eq_ignore_ascii_case("transport") {
        return Ok(Some(ProviderError::Transport));
    }
    let status = value.parse::<u16>().map_err(|_| ProviderError::Transport)?;
    if (200..300).contains(&status) {
        return Ok(None);
    }
    if status == 429 {
        return Ok(Some(ProviderError::RateLimited { retry_after: None }));
    }
    Ok(Some(ProviderError::HttpStatus(status)))
}

fn map_body_error(error: BoundedReadError) -> ProviderError {
    match error {
        BoundedReadError::Read => ProviderError::Body,
        BoundedReadError::TooLarge => ProviderError::BodyTooLarge,
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
fn append_fixture_log(request: &FixtureRequest) -> Result<(), ProviderError> {
    let Ok(path) = std::env::var(FIXTURE_LOG_ENV) else {
        return Ok(());
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| ProviderError::Transport)?;
    writeln!(file, "{timestamp}\t{}", request.filename()).map_err(|_| ProviderError::Transport)
}

pub(crate) fn wait_for_request_slot(cancelled: &mut dyn FnMut() -> bool) -> bool {
    const SLICE: Duration = Duration::from_millis(50);
    let mut previous = lock_unpoisoned(&LAST_REQUEST);
    let mut delay = previous.map_or(Duration::ZERO, |instant| {
        MIN_REQUEST_INTERVAL.saturating_sub(instant.elapsed())
    });
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

fn classify_transport(error: ureq::Error) -> ProviderError {
    match error {
        ureq::Error::Timeout(_) => ProviderError::Timeout,
        other if other.to_string().to_ascii_lowercase().contains("timeout") => {
            ProviderError::Timeout
        }
        _ => ProviderError::Transport,
    }
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

#[cfg(any(test, feature = "test-fixtures"))]
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_routes_cover_every_concerts_http_consumer() {
        for (url, filename) in [
            (
                "https://rest.bandsintown.com/artists/Lorna%20Shore?app_id=x",
                "bandsintown-artist-Lorna_Shore.json",
            ),
            (
                "https://rest.bandsintown.com/artists/Lorna%20Shore/events?app_id=x",
                "bandsintown-events-Lorna_Shore.json",
            ),
            (
                "https://app.ticketmaster.com/discovery/v2/attractions.json?keyword=Lorna%20Shore&apikey=x",
                "ticketmaster-attractions-Lorna_Shore.json",
            ),
            (
                "https://app.ticketmaster.com/discovery/v2/events.json?attractionId=abc&apikey=x",
                "ticketmaster-events-abc.json",
            ),
            (
                "https://nominatim.openstreetmap.org/search?q=Munich&format=json&limit=1",
                "nominatim-Munich.json",
            ),
            (
                "https://labs.api.listenbrainz.org/similar-artists/json?artist_mbids=abc",
                "listenbrainz-similar-abc.json",
            ),
            (
                "https://ws.audioscrobbler.com/2.0/?method=artist.getsimilar&artist=Lorna%20Shore",
                "lastfm-similar-Lorna_Shore.json",
            ),
        ] {
            assert_eq!(fixture_request(url).unwrap().filename(), filename);
        }
    }

    #[test]
    fn user_agent_identifies_reprise_and_the_contact_url() {
        let value = user_agent();
        assert!(value.contains(env!("CARGO_PKG_VERSION")));
        assert!(value.contains(crate::musicbrainz::CONTACT_URL));
    }

    #[test]
    fn oversized_fixture_body_is_rejected() {
        let fixtures = tempfile::tempdir().unwrap();
        std::fs::write(
            fixtures.path().join("bandsintown-artist-Oversized.json"),
            vec![b'x'; crate::http_body::MAX_JSON_RESPONSE_BYTES as usize + 1],
        )
        .unwrap();

        assert_eq!(
            fixture_get(
                "https://rest.bandsintown.com/artists/Oversized?app_id=x",
                fixtures.path()
            ),
            Err(ProviderError::BodyTooLarge)
        );
    }
}
