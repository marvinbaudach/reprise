//! Deezer public-API artist portrait client. Blocking; worker-thread only.
//! Own rate throttle and HTTP agent — deliberately not routed through
//! `musicbrainz::get`, which applies MusicBrainz's one-request-per-second limit.

use std::io::Read;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::musicbrainz::{self, FetchError};

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(300);
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_SEARCH_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);
static AGENT: OnceLock<ureq::Agent> = OnceLock::new();

pub(crate) struct DeezerArtist {
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
    let wanted = super::normalize(name);
    for candidate in data {
        let Some(candidate_name) = candidate.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if super::normalize(candidate_name) != wanted {
            continue;
        }
        let picture_url = ["picture_xl", "picture_big"].into_iter().find_map(|field| {
            candidate
                .get(field)
                .and_then(serde_json::Value::as_str)
                .filter(|url| !url.is_empty() && !is_placeholder_url(url))
                .map(str::to_owned)
        });
        return Some(DeezerArtist { picture_url });
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
    let mut body = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_SEARCH_RESPONSE_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|_| FetchError::Body)?;
    if body.len() as u64 > MAX_SEARCH_RESPONSE_BYTES {
        return Err(FetchError::Body);
    }
    String::from_utf8(body).map_err(|_| FetchError::Body)
}

pub(crate) fn download_image(url: &str) -> Result<Vec<u8>, FetchError> {
    if !is_deezer_image_url(url) {
        return Err(FetchError::Transport);
    }
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

fn is_deezer_image_url(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host.ends_with(".dzcdn.net"))
}

fn agent() -> &'static ureq::Agent {
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(HTTP_TIMEOUT))
            .https_only(true)
            .user_agent(musicbrainz::user_agent())
            .build()
            .new_agent()
    })
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
    let now = Instant::now();
    let next = {
        let mut guard = LAST_REQUEST
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let next = guard.map_or(now, |last| (last + MIN_REQUEST_INTERVAL).max(now));
        *guard = Some(next);
        next
    };
    let delay = next.saturating_duration_since(Instant::now());
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
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
    fn parse_falls_back_to_real_big_when_xl_is_a_placeholder() {
        let json = r#"{"data":[{
          "name":"Band",
          "picture_xl":"https://e-cdns-images.dzcdn.net/images/artist//1000x1000.jpg",
          "picture_big":"https://e-cdns-images.dzcdn.net/images/artist/real/500x500.jpg"
        }]}"#;

        let artist = parse_best_artist(json, "Band").unwrap();

        assert_eq!(
            artist.picture_url.as_deref(),
            Some("https://e-cdns-images.dzcdn.net/images/artist/real/500x500.jpg")
        );
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

    #[test]
    fn image_url_requires_https_deezer_cdn() {
        assert!(is_deezer_image_url(
            "https://e-cdns-images.dzcdn.net/images/artist/abc/1000x1000.jpg"
        ));
        assert!(!is_deezer_image_url(
            "https://example.com/images/artist/abc/1000x1000.jpg"
        ));
        assert!(!is_deezer_image_url(
            "http://e-cdns-images.dzcdn.net/images/artist/abc/1000x1000.jpg"
        ));
        assert_eq!(
            download_image("https://example.com/image.jpg"),
            Err(FetchError::Transport)
        );
        assert_eq!(
            download_image("http://e-cdns-images.dzcdn.net/image.jpg"),
            Err(FetchError::Transport)
        );
    }
}
