//! Deezer public-API artist portrait client. Blocking; worker-thread only.
//! Own rate throttle and HTTP agent — deliberately not routed through
//! `musicbrainz::get`, which applies MusicBrainz's one-request-per-second limit.

use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::musicbrainz::{self, FetchError};

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(300);
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

pub(crate) struct DeezerArtist {
    pub name: String,
    pub picture_url: Option<String>,
}

pub(crate) fn search_url(name: &str) -> String {
    format!(
        "https://api.deezer.com/search/artist?q={}&limit=5",
        musicbrainz::urlencode(name.trim())
    )
}

/// Returns the first exact-name match, omitting Deezer's placeholder image.
pub(crate) fn parse_best_artist(json: &str, name: &str) -> Option<DeezerArtist> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let data = value.get("data")?.as_array()?;
    let wanted = normalize(name);
    for candidate in data {
        let Some(candidate_name) = candidate.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if normalize(candidate_name) != wanted {
            continue;
        }
        let picture_url = candidate
            .get("picture_xl")
            .or_else(|| candidate.get("picture_big"))
            .and_then(serde_json::Value::as_str)
            .filter(|url| !url.is_empty() && !is_placeholder_url(url))
            .map(str::to_owned);
        return Some(DeezerArtist {
            name: candidate_name.to_owned(),
            picture_url,
        });
    }
    None
}

fn is_placeholder_url(url: &str) -> bool {
    url.contains("/artist//")
}

/// Fetches a prebuilt Deezer search URL as text.
pub(crate) fn search(url: &str) -> Result<String, FetchError> {
    respect_rate_limit();
    let response = agent().get(url).call().map_err(map_ureq_error)?;
    response
        .into_body()
        .read_to_string()
        .map_err(|_| FetchError::Body)
}

pub(crate) fn download_image(url: &str) -> Result<Vec<u8>, FetchError> {
    respect_rate_limit();
    let response = agent().get(url).call().map_err(map_ureq_error)?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FetchError::Body)?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(FetchError::Body);
    }
    Ok(bytes)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(musicbrainz::user_agent())
        .build()
        .new_agent()
}

fn map_ureq_error(error: ureq::Error) -> FetchError {
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

fn respect_rate_limit() {
    let mut guard = LAST_REQUEST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(last) = *guard {
        let elapsed = last.elapsed();
        if elapsed < MIN_REQUEST_INTERVAL {
            std::thread::sleep(MIN_REQUEST_INTERVAL - elapsed);
        }
    }
    *guard = Some(Instant::now());
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HIT: &str = r#"{"data":[
      {"id":1,"name":"Blessthefall","picture_xl":"https://e-cdns-images.dzcdn.net/images/artist/abc123/1000x1000-000000-80-0-0.jpg"}
    ],"total":1}"#;

    const PLACEHOLDER: &str = r#"{"data":[
      {"id":2,"name":"Before I Turn","picture_xl":"https://e-cdns-images.dzcdn.net/images/artist//1000x1000-000000-80-0-0.jpg"}
    ],"total":1}"#;

    const WRONG_NAME: &str = r#"{"data":[
      {"id":3,"name":"Blessthefall (Tribute)","picture_xl":"https://e-cdns-images.dzcdn.net/images/artist/def/1000x1000-000000-80-0-0.jpg"}
    ],"total":1}"#;

    #[test]
    fn search_url_encodes_query() {
        let url = search_url("Bring Me The Horizon");
        assert!(url.starts_with("https://api.deezer.com/search/artist?q="));
        assert!(url.contains("limit=5"));
        assert!(!url.contains(' '));
    }

    #[test]
    fn parse_accepts_exact_normalized_match_with_picture() {
        let artist = parse_best_artist(HIT, "  blessthefall ").unwrap();
        assert_eq!(artist.name, "Blessthefall");
        assert_eq!(
            artist.picture_url.as_deref(),
            Some(
                "https://e-cdns-images.dzcdn.net/images/artist/abc123/1000x1000-000000-80-0-0.jpg"
            )
        );
    }

    #[test]
    fn parse_treats_deezer_placeholder_as_no_picture() {
        let artist = parse_best_artist(PLACEHOLDER, "Before I Turn").unwrap();
        assert!(artist.picture_url.is_none());
    }

    #[test]
    fn parse_rejects_non_exact_name() {
        assert!(parse_best_artist(WRONG_NAME, "Blessthefall").is_none());
    }

    #[test]
    fn parse_handles_empty_and_garbage() {
        assert!(parse_best_artist(r#"{"data":[]}"#, "X").is_none());
        assert!(parse_best_artist("not json", "X").is_none());
    }

    #[test]
    fn is_placeholder_detects_empty_md5_segment() {
        assert!(is_placeholder_url(
            "https://e-cdns-images.dzcdn.net/images/artist//1000x1000-000000-80-0-0.jpg"
        ));
        assert!(!is_placeholder_url(
            "https://e-cdns-images.dzcdn.net/images/artist/abc/1000x1000-000000-80-0-0.jpg"
        ));
    }
}
