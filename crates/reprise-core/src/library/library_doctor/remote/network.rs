//! Network implementation hidden behind the deep remote-provider seam.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::acoustid::parse_acoustid;
use super::metadata::canonical_uuid;
use super::{
    RemoteDirectLookup, RemoteEvidenceSource, RemoteIdentity, RemoteProvider, RemoteProviderError,
    RemoteProviderResult, RemoteTrackMetadata,
};
use crate::library::library_doctor::ScanControl;

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(test)]
const MUSICBRAINZ_INTERVAL: Duration = Duration::from_secs(1);
const ACOUSTID_INTERVAL: Duration = Duration::from_millis(334);
const WAIT_SLICE: Duration = Duration::from_millis(50);
const MAX_ATTEMPTS: usize = 3;
const ACOUSTID_ENDPOINT: &str = "https://api.acoustid.org/v2/lookup";

/// Compile-time client key. A build without it keeps pure MusicBrainz useful.
pub const BUNDLED_ACOUSTID_CLIENT_KEY: Option<&str> = option_env!("REPRISE_ACOUSTID_CLIENT_KEY");

static LAST_ACOUSTID: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Default)]
pub(crate) struct NoNetworkProvider;

impl RemoteProvider for NoNetworkProvider {
    fn direct(
        &mut self,
        _: &RemoteDirectLookup,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        Ok(Vec::new())
    }

    fn search_musicbrainz(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        Ok(Vec::new())
    }

    fn acoustid(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &str,
        _: &str,
        _: u64,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        Ok(Vec::new())
    }
}

pub(crate) struct NetworkProvider {
    agent: ureq::Agent,
    dedup: HashMap<String, RemoteProviderResult>,
    musicbrainz_circuit_open: bool,
    acoustid_circuit_open: bool,
}

#[derive(Clone, Copy)]
enum NetworkSource {
    MusicBrainz,
    AcoustId,
}

impl NetworkProvider {
    pub(crate) fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(HTTP_TIMEOUT))
            .http_status_as_error(false)
            .user_agent(crate::musicbrainz::user_agent())
            .build()
            .new_agent();
        Self {
            agent,
            dedup: HashMap::new(),
            musicbrainz_circuit_open: false,
            acoustid_circuit_open: false,
        }
    }

    fn musicbrainz(
        &mut self,
        lookup_kind: &str,
        url: &str,
        parse: fn(&str) -> RemoteProviderResult,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        let key = format!("mb:{lookup_kind}:{url}");
        let agent = self.agent.clone();
        self.memoized(NetworkSource::MusicBrainz, key, || {
            request_with_retry(&Mutex::new(None), Duration::ZERO, control, |control| {
                if !crate::musicbrainz::wait_for_request_slot(&mut || {
                    control() == ScanControl::Cancel
                }) {
                    return Err(RemoteProviderError::Cancelled);
                }
                http_get(&agent, url)
            })
            .and_then(|body| parse(&body))
        })
    }

    fn acoustid_request(
        &mut self,
        fingerprint: &str,
        duration: u64,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        let Some(client) = BUNDLED_ACOUSTID_CLIENT_KEY else {
            return Err(RemoteProviderError::Unavailable);
        };
        let key = format!("ac:{fingerprint}:{duration}");
        let agent = self.agent.clone();
        let form = acoustid_form(client, fingerprint, duration);
        self.memoized(NetworkSource::AcoustId, key, || {
            request_with_retry(&LAST_ACOUSTID, ACOUSTID_INTERVAL, control, |_| {
                http_post(&agent, &form)
            })
            .and_then(|body| parse_acoustid(&body))
        })
    }

    fn memoized(
        &mut self,
        source: NetworkSource,
        key: String,
        request: impl FnOnce() -> RemoteProviderResult,
    ) -> RemoteProviderResult {
        let circuit_open = match source {
            NetworkSource::MusicBrainz => self.musicbrainz_circuit_open,
            NetworkSource::AcoustId => self.acoustid_circuit_open,
        };
        if circuit_open {
            return Err(RemoteProviderError::Unavailable);
        }
        if let Some(result) = self.dedup.get(&key) {
            return result.clone();
        }
        let result = request();
        if matches!(result, Err(RemoteProviderError::Unavailable)) {
            match source {
                NetworkSource::MusicBrainz => self.musicbrainz_circuit_open = true,
                NetworkSource::AcoustId => self.acoustid_circuit_open = true,
            }
        }
        if result.is_ok() {
            self.dedup.insert(key, result.clone());
        }
        result
    }
}

impl RemoteProvider for NetworkProvider {
    fn direct(
        &mut self,
        lookup: &RemoteDirectLookup,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        let (lookup_kind, path, parse): (&str, String, fn(&str) -> RemoteProviderResult) =
            match lookup {
                RemoteDirectLookup::Recording(id) => (
                    "recording",
                    format!("recording/{id}?inc=artists+releases+release-groups&fmt=json"),
                    parse_musicbrainz as fn(&str) -> RemoteProviderResult,
                ),
                RemoteDirectLookup::Release(id) => (
                    "release",
                    format!("release/{id}?inc=recordings+artists+release-groups&fmt=json"),
                    parse_release,
                ),
                RemoteDirectLookup::ReleaseGroup(id) => (
                    "release_group",
                    format!("release-group/{id}?inc=releases+artist-credits&fmt=json"),
                    parse_release_group,
                ),
                RemoteDirectLookup::Artist(id) => {
                    ("artist", format!("artist/{id}?fmt=json"), parse_artist)
                }
                RemoteDirectLookup::ReleaseArtist(id) => (
                    "release_artist",
                    format!("artist/{id}?fmt=json"),
                    parse_release_artist,
                ),
            };
        self.musicbrainz(
            lookup_kind,
            &format!("https://musicbrainz.org/ws/2/{path}"),
            parse,
            control,
        )
    }

    fn search_musicbrainz(
        &mut self,
        metadata: &RemoteTrackMetadata,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        let mut terms = Vec::new();
        push_term(&mut terms, "recording", metadata.lookup_title());
        push_term(&mut terms, "artist", metadata.lookup_artist());
        push_term(&mut terms, "release", metadata.lookup_album());
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let query = crate::musicbrainz::urlencode(&terms.join(" AND "));
        self.musicbrainz(
            "recording_search",
            &format!("https://musicbrainz.org/ws/2/recording?query={query}&fmt=json&limit=10"),
            parse_musicbrainz,
            control,
        )
    }

    fn acoustid(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &str,
        fingerprint: &str,
        duration_seconds: u64,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> RemoteProviderResult {
        self.acoustid_request(fingerprint, duration_seconds, control)
    }
}

#[derive(Debug)]
struct HttpReply {
    status: u16,
    retry_after: Option<Duration>,
    body: String,
}

fn http_get(agent: &ureq::Agent, url: &str) -> Result<HttpReply, RemoteProviderError> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|_| RemoteProviderError::InvalidResponse)?;
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("Retry-After")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_retry_after(value, std::time::SystemTime::now()));
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|_| RemoteProviderError::InvalidResponse)?;
    Ok(HttpReply {
        status,
        retry_after,
        body,
    })
}

fn http_post(
    agent: &ureq::Agent,
    form: &[(String, String)],
) -> Result<HttpReply, RemoteProviderError> {
    let mut response = agent
        .post(ACOUSTID_ENDPOINT)
        .send_form(form.iter().cloned())
        .map_err(|_| RemoteProviderError::InvalidResponse)?;
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("Retry-After")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_retry_after(value, std::time::SystemTime::now()));
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|_| RemoteProviderError::InvalidResponse)?;
    Ok(HttpReply {
        status,
        retry_after,
        body,
    })
}

fn request_with_retry(
    limiter: &Mutex<Option<Instant>>,
    interval: Duration,
    control: &mut dyn FnMut() -> ScanControl,
    mut request: impl FnMut(&mut dyn FnMut() -> ScanControl) -> Result<HttpReply, RemoteProviderError>,
) -> Result<String, RemoteProviderError> {
    for attempt in 0..MAX_ATTEMPTS {
        rate_limit(limiter, interval, control)?;
        if control() == ScanControl::Cancel {
            return Err(RemoteProviderError::Cancelled);
        }
        match request(control) {
            Ok(reply) if (200..300).contains(&reply.status) => {
                if control() == ScanControl::Cancel {
                    return Err(RemoteProviderError::Cancelled);
                }
                return Ok(reply.body);
            }
            Ok(reply) if matches!(reply.status, 401 | 403) => {
                return Err(RemoteProviderError::Unavailable);
            }
            Ok(reply) if reply.status == 429 || reply.status >= 500 => {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(RemoteProviderError::InvalidResponse);
                }
                let delay = reply
                    .retry_after
                    .unwrap_or_else(|| Duration::from_millis(250 * (attempt as u64 + 1)));
                cancellable_sleep(delay, control)?;
            }
            Ok(_) => return Err(RemoteProviderError::InvalidResponse),
            Err(RemoteProviderError::Cancelled) => {
                return Err(RemoteProviderError::Cancelled);
            }
            Err(error) => {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(error);
                }
                cancellable_sleep(Duration::from_millis(250 * (attempt as u64 + 1)), control)?;
            }
        }
    }
    Err(RemoteProviderError::InvalidResponse)
}

fn rate_limit(
    limiter: &Mutex<Option<Instant>>,
    interval: Duration,
    control: &mut dyn FnMut() -> ScanControl,
) -> Result<(), RemoteProviderError> {
    let delay = {
        let mut previous = lock_unpoisoned(limiter);
        let now = Instant::now();
        let delay = request_delay(*previous, now, interval);
        *previous = Some(now + delay);
        delay
    };
    cancellable_sleep(delay, control)
}

fn request_delay(previous: Option<Instant>, now: Instant, interval: Duration) -> Duration {
    previous.map_or(Duration::ZERO, |value| {
        if value > now {
            value.duration_since(now).saturating_add(interval)
        } else {
            interval.saturating_sub(now.duration_since(value))
        }
    })
}

fn parse_retry_after(value: &str, now: std::time::SystemTime) -> Option<Duration> {
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let deadline = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let now: chrono::DateTime<chrono::Utc> = now.into();
    deadline
        .with_timezone(&chrono::Utc)
        .signed_duration_since(now)
        .to_std()
        .ok()
        .or(Some(Duration::ZERO))
}

fn cancellable_sleep(
    mut remaining: Duration,
    control: &mut dyn FnMut() -> ScanControl,
) -> Result<(), RemoteProviderError> {
    while !remaining.is_zero() {
        if control() == ScanControl::Cancel {
            return Err(RemoteProviderError::Cancelled);
        }
        let slice = remaining.min(WAIT_SLICE);
        std::thread::sleep(slice);
        remaining = remaining.saturating_sub(slice);
    }
    Ok(())
}

fn acoustid_form(client: &str, fingerprint: &str, duration: u64) -> Vec<(String, String)> {
    vec![
        ("client".into(), client.into()),
        ("format".into(), "json".into()),
        ("meta".into(), "recordings+releasegroups+releases".into()),
        ("fingerprint".into(), fingerprint.into()),
        ("duration".into(), duration.to_string()),
    ]
}

fn push_term(terms: &mut Vec<String>, field: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        terms.push(format!(r#"{field}:"{}""#, value.replace('"', "")));
    }
}

fn parse_musicbrainz(body: &str) -> RemoteProviderResult {
    let root: Value =
        serde_json::from_str(body).map_err(|_| RemoteProviderError::InvalidResponse)?;
    let entries = root
        .get("recordings")
        .and_then(Value::as_array)
        .map_or_else(|| std::slice::from_ref(&root), Vec::as_slice);
    Ok(entries
        .iter()
        .flat_map(|value| parse_recording_identities(value, RemoteEvidenceSource::MusicBrainz, 100))
        .collect())
}

fn parse_release(body: &str) -> RemoteProviderResult {
    let root: Value =
        serde_json::from_str(body).map_err(|_| RemoteProviderError::InvalidResponse)?;
    let release_mbid = canonical_uuid(root.get("id").and_then(Value::as_str))
        .ok_or(RemoteProviderError::InvalidResponse)?;
    let release_group = root.get("release-group");
    Ok(vec![RemoteIdentity {
        source: RemoteEvidenceSource::MusicBrainz,
        confidence: 100,
        recording_mbid: None,
        release_mbid: Some(release_mbid),
        release_group_mbid: release_group
            .and_then(|value| canonical_uuid(value.get("id")?.as_str())),
        artist_mbid: None,
        release_artist_mbid: credit(&root)
            .and_then(|item| canonical_uuid(item.get("artist")?.get("id")?.as_str())),
        title: None,
        artist: None,
        album: text(&root, "title"),
        album_artist: credit(&root).and_then(|item| text(item, "name")),
        release_year: root.get("date").and_then(Value::as_str).and_then(year),
        original_release_year: release_group
            .and_then(|group| group.get("first-release-date"))
            .and_then(Value::as_str)
            .and_then(year),
        duration_ms: None,
    }])
}

fn parse_release_group(body: &str) -> RemoteProviderResult {
    let root: Value =
        serde_json::from_str(body).map_err(|_| RemoteProviderError::InvalidResponse)?;
    let group_mbid = canonical_uuid(root.get("id").and_then(Value::as_str))
        .ok_or(RemoteProviderError::InvalidResponse)?;
    Ok(vec![RemoteIdentity {
        source: RemoteEvidenceSource::MusicBrainz,
        confidence: 100,
        recording_mbid: None,
        release_mbid: None,
        release_group_mbid: Some(group_mbid),
        artist_mbid: None,
        release_artist_mbid: credit(&root)
            .and_then(|item| canonical_uuid(item.get("artist")?.get("id")?.as_str())),
        title: None,
        artist: None,
        album: text(&root, "title"),
        album_artist: credit(&root).and_then(|item| text(item, "name")),
        release_year: None,
        original_release_year: root
            .get("first-release-date")
            .and_then(Value::as_str)
            .and_then(year),
        duration_ms: None,
    }])
}

fn parse_artist(body: &str) -> RemoteProviderResult {
    parse_artist_identity(body, false)
}

fn parse_release_artist(body: &str) -> RemoteProviderResult {
    parse_artist_identity(body, true)
}

fn parse_artist_identity(body: &str, release_artist: bool) -> RemoteProviderResult {
    let root: Value =
        serde_json::from_str(body).map_err(|_| RemoteProviderError::InvalidResponse)?;
    let artist_mbid = canonical_uuid(root.get("id").and_then(Value::as_str))
        .ok_or(RemoteProviderError::InvalidResponse)?;
    Ok(vec![RemoteIdentity {
        source: RemoteEvidenceSource::MusicBrainz,
        confidence: 100,
        recording_mbid: None,
        release_mbid: None,
        release_group_mbid: None,
        artist_mbid: (!release_artist).then(|| artist_mbid.clone()),
        release_artist_mbid: release_artist.then_some(artist_mbid),
        title: None,
        artist: text(&root, "name"),
        album: None,
        album_artist: None,
        release_year: None,
        original_release_year: None,
        duration_ms: None,
    }])
}

fn credit(value: &Value) -> Option<&Value> {
    value.get("artist-credit")?.as_array()?.first()
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}

fn parse_recording_identities(
    value: &Value,
    source: RemoteEvidenceSource,
    fallback: u8,
) -> Vec<RemoteIdentity> {
    let releases = value.get("releases").and_then(Value::as_array);
    match releases {
        Some(releases) if !releases.is_empty() => releases
            .iter()
            .filter_map(|release| {
                parse_identity_with_release(value, Some(release), source, fallback)
            })
            .collect(),
        _ => parse_identity_with_release(value, None, source, fallback)
            .into_iter()
            .collect(),
    }
}

fn parse_identity_with_release(
    value: &Value,
    release: Option<&Value>,
    source: RemoteEvidenceSource,
    fallback: u8,
) -> Option<RemoteIdentity> {
    let recording_mbid = canonical_uuid(Some(value.get("id")?.as_str()?))?;
    let confidence = value
        .get("score")
        .and_then(Value::as_f64)
        .map_or(fallback, |score| percentage(Some(score / 100.0)));
    let artist = value
        .get("artist-credit")
        .and_then(Value::as_array)
        .and_then(|v| v.first());
    Some(RemoteIdentity {
        source,
        confidence,
        recording_mbid: Some(recording_mbid),
        release_mbid: release.and_then(|item| canonical_uuid(item.get("id")?.as_str())),
        release_group_mbid: release
            .and_then(|item| canonical_uuid(item.get("release-group")?.get("id")?.as_str())),
        artist_mbid: artist
            .and_then(|item| canonical_uuid(item.get("artist")?.get("id")?.as_str())),
        release_artist_mbid: release
            .and_then(credit)
            .and_then(|item| canonical_uuid(item.get("artist")?.get("id")?.as_str())),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_owned),
        artist: artist
            .and_then(|item| item.get("name"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        album: release
            .and_then(|item| item.get("title"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        album_artist: release
            .and_then(|item| item.get("artist-credit"))
            .and_then(Value::as_array)
            .and_then(|v| v.first())
            .and_then(|item| item.get("name"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        release_year: release.and_then(|item| year(item.get("date")?.as_str()?)),
        original_release_year: release.and_then(|item| {
            year(
                item.get("release-group")?
                    .get("first-release-date")?
                    .as_str()?,
            )
        }),
        duration_ms: value.get("length").and_then(Value::as_u64),
    })
}

fn year(value: &str) -> Option<u32> {
    value.get(..4)?.parse().ok()
}

fn percentage(value: Option<f64>) -> u8 {
    value
        .map(|score| (score.clamp(0.0, 1.0) * 100.0).round() as u8)
        .unwrap_or_default()
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
