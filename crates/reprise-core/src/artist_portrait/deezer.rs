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
const MISSING_IMAGE_IDENTIFIERS: &[&str] = &[
    "",
    // Deezer puts MD5 of the empty string into its image URL when no artist
    // image exists. Keep the sentinel explicit; computing it adds no value.
    "d41d8cd98f00b204e9800998ecf8427e",
];

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);
static AGENT: OnceLock<ureq::Agent> = OnceLock::new();

pub(crate) struct DeezerArtist {
    pub picture_url: Option<String>,
}

pub(crate) fn search_url(name: &str) -> String {
    format!(
        "https://api.deezer.com/search/artist?q={}&limit=10",
        musicbrainz::urlencode(name.trim())
    )
}

/// Returns the most popular pictured exact-name match, preserving response
/// order when picture availability and fan count tie.
pub(crate) fn parse_best_artist(json: &str, name: &str) -> Option<DeezerArtist> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let data = value.get("data")?.as_array()?;
    let wanted = super::normalize(name);
    data.iter()
        .filter_map(|candidate| {
            let candidate_name = candidate.get("name")?.as_str()?;
            (super::normalize(candidate_name) == wanted).then(|| Candidate {
                picture_url: candidate_picture_url(candidate),
                fan_count: candidate
                    .get("nb_fan")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            })
        })
        .fold(None, |best, candidate| match best {
            Some(current) if !candidate.outranks(&current) => Some(current),
            _ => Some(candidate),
        })
        .map(|candidate| DeezerArtist {
            picture_url: candidate.picture_url,
        })
}

struct Candidate {
    picture_url: Option<String>,
    fan_count: u64,
}

impl Candidate {
    fn outranks(&self, other: &Self) -> bool {
        (self.picture_url.is_some(), self.fan_count)
            > (other.picture_url.is_some(), other.fan_count)
    }
}

fn candidate_picture_url(candidate: &serde_json::Value) -> Option<String> {
    let url = ["picture_xl", "picture_big"]
        .into_iter()
        .find_map(|field| candidate.get(field)?.as_str().filter(|url| !url.is_empty()))?;
    (!is_placeholder_url(url)).then(|| url.to_owned())
}

fn is_placeholder_url(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    let Some(mut segments) = url.path_segments() else {
        return false;
    };
    while let Some(segment) = segments.next() {
        if segment == "images" && segments.next() == Some("artist") {
            return segments
                .next()
                .is_some_and(|identifier| MISSING_IMAGE_IDENTIFIERS.contains(&identifier));
        }
    }
    false
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
    use crate::artist_portrait::test_fixtures::ALL_PLACEHOLDERS_RESPONSE;

    const HIT: &str = r#"{"data":[
      {"id":1,"link":"https://www.deezer.com/artist/1","name":"Blessthefall","nb_album":12,"nb_fan":3456,"picture":"https://cdn-images.dzcdn.net/images/artist/abc123/500x500-000000-80-0-0.jpg","picture_big":"https://cdn-images.dzcdn.net/images/artist/abc123/500x500-000000-80-0-0.jpg","picture_medium":"https://cdn-images.dzcdn.net/images/artist/abc123/250x250-000000-80-0-0.jpg","picture_small":"https://cdn-images.dzcdn.net/images/artist/abc123/56x56-000000-80-0-0.jpg","picture_xl":"https://cdn-images.dzcdn.net/images/artist/abc123/1000x1000-000000-80-0-0.jpg","radio":true,"tracklist":"https://api.deezer.com/artist/1/top?limit=50","type":"artist"}
    ],"total":1}"#;

    const WRONG_NAME: &str = r#"{"data":[
      {"id":3,"name":"Blessthefall (Tribute)","nb_album":1,"nb_fan":999999,"picture_xl":"https://cdn-images.dzcdn.net/images/artist/def/1000x1000-000000-80-0-0.jpg","picture_big":"https://cdn-images.dzcdn.net/images/artist/def/500x500-000000-80-0-0.jpg","type":"artist"}
    ],"total":1}"#;

    #[test]
    fn search_url_encodes_query() {
        let url = search_url("Bring Me The Horizon");
        assert!(url.starts_with("https://api.deezer.com/search/artist?q="));
        assert!(url.contains("limit=10"));
        assert!(!url.contains(' '));
    }

    #[test]
    fn parse_accepts_exact_normalized_match_with_picture() {
        let artist = parse_best_artist(HIT, "  blessthefall ").unwrap();
        let picture = url::Url::parse(artist.picture_url.as_deref().unwrap()).unwrap();
        assert!(picture
            .host_str()
            .is_some_and(|host| host.ends_with(".dzcdn.net")));
        assert!(picture.path().contains("/images/artist/abc123/"));
    }

    #[test]
    fn parse_treats_deezer_placeholder_as_no_picture() {
        let artist = parse_best_artist(ALL_PLACEHOLDERS_RESPONSE, "Band").unwrap();
        assert!(artist.picture_url.is_none());
    }

    #[test]
    fn parse_falls_back_to_real_big_when_xl_is_missing() {
        let json = r#"{"data":[{
          "name":"Band",
          "nb_album":1,
          "nb_fan":1,
          "picture_big":"https://cdn-images.dzcdn.net/images/artist/real/500x500.jpg",
          "type":"artist"
        }]}"#;

        let artist = parse_best_artist(json, "Band").unwrap();
        let picture = url::Url::parse(artist.picture_url.as_deref().unwrap()).unwrap();
        assert!(picture
            .host_str()
            .is_some_and(|host| host.ends_with(".dzcdn.net")));
        assert!(picture.path().contains("/images/artist/real/"));
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
    fn placeholder_detection_reads_the_artist_identifier_segment() {
        assert!(is_placeholder_url(
            "https://cdn-images.dzcdn.net/images/artist//1000x1000-000000-80-0-0.jpg"
        ));
        assert!(is_placeholder_url(
            "https://cdn-images.dzcdn.net/images/artist/d41d8cd98f00b204e9800998ecf8427e/1000x1000-000000-80-0-0.jpg"
        ));
        assert!(!is_placeholder_url(
            "https://cdn-images.dzcdn.net/images/artist/415714b600000000000000000000afe4/1000x1000-000000-80-0-0.jpg"
        ));
    }

    #[test]
    fn image_url_requires_https_deezer_cdn() {
        assert!(is_deezer_image_url(
            "https://cdn-images.dzcdn.net/images/artist/abc/1000x1000.jpg"
        ));
        assert!(!is_deezer_image_url(
            "https://example.com/images/artist/abc/1000x1000.jpg"
        ));
        assert!(!is_deezer_image_url(
            "http://cdn-images.dzcdn.net/images/artist/abc/1000x1000.jpg"
        ));
        assert_eq!(
            download_image("https://example.com/image.jpg"),
            Err(FetchError::Transport)
        );
        assert_eq!(
            download_image("http://cdn-images.dzcdn.net/image.jpg"),
            Err(FetchError::Transport)
        );
    }
}
