//! Signed Last.fm desktop authentication and scrobbling transport.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{Read, Take};
use std::time::Duration;

use md5::{Digest, Md5};
use serde_json::Value;

use super::{Listen, MetadataError, ScrobblerTransport, TrackMetadata, TransportError};

const API_ROOT: &str = "https://ws.audioscrobbler.com/2.0/";
const AUTH_ROOT: &str = "https://www.last.fm/api/auth/";
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
const MAX_SCROBBLES: usize = 50;
const USER_AGENT: &str = concat!("Reprise/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, PartialEq, Eq)]
pub struct LastFmSession {
    pub user_name: String,
    pub session_key: String,
}

impl fmt::Debug for LastFmSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LastFmSession")
            .field("user_name", &self.user_name)
            .field("session_key", &"<redacted>")
            .finish()
    }
}

pub struct LastFmClient {
    api_key: String,
    shared_secret: String,
    api_root: String,
    auth_root: String,
    agent: ureq::Agent,
}

impl LastFmClient {
    pub fn new(api_key: &str, shared_secret: &str) -> Result<Self, MetadataError> {
        Self::with_roots(API_ROOT, AUTH_ROOT, api_key, shared_secret)
    }

    /// Constructs a client for loopback-only integration tests and smokes.
    /// Production callers should use [`Self::new`].
    #[doc(hidden)]
    pub fn with_roots(
        api_root: &str,
        auth_root: &str,
        api_key: &str,
        shared_secret: &str,
    ) -> Result<Self, MetadataError> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(MetadataError::MissingApiKey);
        }
        let shared_secret = shared_secret.trim().to_string();
        if shared_secret.is_empty() {
            return Err(MetadataError::MissingSharedSecret);
        }
        Ok(Self {
            api_key,
            shared_secret,
            api_root: api_root.trim_end_matches('/').to_string(),
            auth_root: format!("{}/", auth_root.trim_end_matches('/')),
            agent: ureq::builder()
                .timeout(HTTP_TIMEOUT)
                .user_agent(USER_AGENT)
                .build(),
        })
    }

    pub fn request_token(&self) -> Result<String, TransportError> {
        let params = BTreeMap::from([("method".to_string(), "auth.getToken".to_string())]);
        let body = self.call(params, None)?;
        parse_request_token(&body)
    }

    pub fn authorization_url(&self, token: &str) -> Result<String, MetadataError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(MetadataError::MissingRequestToken);
        }
        let mut url = url::Url::parse(&self.auth_root).map_err(|_| MetadataError::Serialization)?;
        url.query_pairs_mut()
            .append_pair("api_key", &self.api_key)
            .append_pair("token", token);
        Ok(url.into())
    }

    pub fn exchange_token(&self, token: &str) -> Result<LastFmSession, TransportError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(MetadataError::MissingRequestToken.into());
        }
        let params = BTreeMap::from([
            ("method".to_string(), "auth.getSession".to_string()),
            ("token".to_string(), token.to_string()),
        ]);
        let body = self.call(params, None)?;
        parse_session(&body)
    }

    fn call(
        &self,
        mut params: BTreeMap<String, String>,
        session_key: Option<&str>,
    ) -> Result<String, TransportError> {
        params.insert("api_key".to_string(), self.api_key.clone());
        if let Some(session_key) = session_key {
            params.insert("sk".to_string(), session_key.to_string());
        }
        let signature = method_signature(&params, &self.shared_secret);
        params.insert("api_sig".to_string(), signature);
        params.insert("format".to_string(), "json".to_string());
        let form = params
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let response = self
            .agent
            .post(&self.api_root)
            .send_form(&form)
            .map_err(|error| classify_http_error(&error))?;
        let mut body = String::new();
        let reader = response.into_reader();
        let mut reader: Take<_> = reader.take(MAX_RESPONSE_BYTES + 1);
        reader
            .read_to_string(&mut body)
            .map_err(|_| TransportError::InvalidResponse)?;
        if body.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(TransportError::InvalidResponse);
        }
        if let Some(code) = api_error_code(&body)? {
            return Err(classify_api_error(code));
        }
        Ok(body)
    }
}

impl ScrobblerTransport for LastFmClient {
    fn validate_token(&self, token: &str) -> Result<String, TransportError> {
        let params = BTreeMap::from([("method".to_string(), "user.getInfo".to_string())]);
        let body = self.call(params, Some(token))?;
        let value: Value =
            serde_json::from_str(&body).map_err(|_| TransportError::InvalidResponse)?;
        non_blank(value.pointer("/user/name").and_then(Value::as_str))
    }

    fn playing_now(&self, token: &str, track: &TrackMetadata) -> Result<(), TransportError> {
        let params = now_playing_params(track)?;
        let body = self.call(params, Some(token))?;
        parse_write_response(&body)
    }

    fn submit(&self, token: &str, listens: &[Listen]) -> Result<(), TransportError> {
        let params = scrobble_params(listens)?;
        let body = self.call(params, Some(token))?;
        parse_write_response(&body)
    }
}

pub(crate) fn method_signature(params: &BTreeMap<String, String>, shared_secret: &str) -> String {
    let mut material = String::new();
    for (name, value) in params {
        if matches!(name.as_str(), "api_sig" | "callback" | "format") {
            continue;
        }
        material.push_str(name);
        material.push_str(value);
    }
    material.push_str(shared_secret);
    format!("{:x}", Md5::digest(material.as_bytes()))
}

pub(crate) fn now_playing_params(
    track: &TrackMetadata,
) -> Result<BTreeMap<String, String>, MetadataError> {
    track.validate()?;
    let mut params = BTreeMap::from([
        ("artist".to_string(), track.artist_name.trim().to_string()),
        ("method".to_string(), "track.updateNowPlaying".to_string()),
        ("track".to_string(), track.track_name.trim().to_string()),
    ]);
    add_optional_track_params(&mut params, track, "");
    Ok(params)
}

pub(crate) fn scrobble_params(
    listens: &[Listen],
) -> Result<BTreeMap<String, String>, MetadataError> {
    if listens.is_empty() {
        return Err(MetadataError::EmptyPayload);
    }
    if listens.len() > MAX_SCROBBLES {
        return Err(MetadataError::TooManyLastFmScrobbles);
    }
    let mut params = BTreeMap::from([("method".to_string(), "track.scrobble".to_string())]);
    for (index, listen) in listens.iter().enumerate() {
        listen.track.validate()?;
        let suffix = format!("[{index}]");
        params.insert(
            format!("artist{suffix}"),
            listen.track.artist_name.trim().to_string(),
        );
        params.insert(
            format!("track{suffix}"),
            listen.track.track_name.trim().to_string(),
        );
        params.insert(format!("timestamp{suffix}"), listen.listened_at.to_string());
        add_optional_track_params(&mut params, &listen.track, &suffix);
    }
    Ok(params)
}

fn add_optional_track_params(
    params: &mut BTreeMap<String, String>,
    track: &TrackMetadata,
    suffix: &str,
) {
    if let Some(album) = track
        .release_name
        .as_deref()
        .map(str::trim)
        .filter(|album| !album.is_empty())
    {
        params.insert(format!("album{suffix}"), album.to_string());
    }
    if track.duration_ms > 0 {
        params.insert(
            format!("duration{suffix}"),
            (track.duration_ms / 1_000).to_string(),
        );
    }
}

pub(crate) fn parse_request_token(body: &str) -> Result<String, TransportError> {
    let value: Value = serde_json::from_str(body).map_err(|_| TransportError::InvalidResponse)?;
    non_blank(value.get("token").and_then(Value::as_str))
}

pub(crate) fn parse_session(body: &str) -> Result<LastFmSession, TransportError> {
    let value: Value = serde_json::from_str(body).map_err(|_| TransportError::InvalidResponse)?;
    Ok(LastFmSession {
        user_name: non_blank(value.pointer("/session/name").and_then(Value::as_str))?,
        session_key: non_blank(value.pointer("/session/key").and_then(Value::as_str))?,
    })
}

pub(crate) fn parse_write_response(body: &str) -> Result<(), TransportError> {
    let value: Value = serde_json::from_str(body).map_err(|_| TransportError::InvalidResponse)?;
    if value.get("scrobbles").is_some() || value.get("nowplaying").is_some() {
        Ok(())
    } else {
        Err(TransportError::InvalidResponse)
    }
}

fn non_blank(value: Option<&str>) -> Result<String, TransportError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(TransportError::InvalidResponse)
}

fn api_error_code(body: &str) -> Result<Option<u16>, TransportError> {
    let value: Value = serde_json::from_str(body).map_err(|_| TransportError::InvalidResponse)?;
    let Some(error) = value.get("error") else {
        return Ok(None);
    };
    error
        .as_u64()
        .and_then(|code| u16::try_from(code).ok())
        .map(Some)
        .ok_or(TransportError::InvalidResponse)
}

pub(crate) fn classify_api_error(code: u16) -> TransportError {
    match code {
        4 | 9 => TransportError::Unauthorized,
        8 | 11 | 16 | 29 => TransportError::Retryable(code),
        _ => TransportError::Rejected(code),
    }
}

fn classify_http_error(error: &ureq::Error) -> TransportError {
    match error {
        ureq::Error::Status(status, _) => match status {
            408 | 429 | 500..=599 => TransportError::Retryable(*status),
            401 | 403 => TransportError::Unauthorized,
            _ => TransportError::Rejected(*status),
        },
        ureq::Error::Transport(_) => TransportError::Network,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_debug_redacts_the_session_key() {
        let session = LastFmSession {
            user_name: "marvin".to_string(),
            session_key: "session-key-must-not-leak".to_string(),
        };
        let debug = format!("{session:?}");
        assert!(debug.contains("marvin"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("session-key-must-not-leak"));
    }

    #[test]
    fn write_response_rejects_unrecognized_success_json() {
        assert_eq!(
            parse_write_response(r#"{"status":"ok"}"#),
            Err(TransportError::InvalidResponse)
        );
    }
}
