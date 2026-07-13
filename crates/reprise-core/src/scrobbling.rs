//! Platform-neutral scrobbling contracts and the ListenBrainz HTTP backend.
//!
//! This module contains no credential storage and never initiates work on its
//! own. A frontend must explicitly enable the integration, retrieve a token
//! from its platform credential store, and call the transport from an
//! off-main worker. Tokens are used only to construct the Authorization header
//! and are deliberately absent from every error value.

use std::time::Duration;
use std::{io::Read, io::Take};

use rusqlite::{params, Connection};
use serde::Serialize;

const LISTENBRAINZ_API_ROOT: &str = "https://api.listenbrainz.org";
const FOUR_MINUTES_MS: i64 = 4 * 60 * 1_000;
const MAX_LISTENS_PER_REQUEST: usize = 1_000;
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
const USER_AGENT: &str = concat!("Reprise/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackMetadata {
    pub artist_name: String,
    pub track_name: String,
    pub release_name: Option<String>,
    pub duration_ms: i64,
}

impl TrackMetadata {
    pub fn validate(&self) -> Result<(), MetadataError> {
        if self.artist_name.trim().is_empty() {
            return Err(MetadataError::MissingArtist);
        }
        if self.track_name.trim().is_empty() {
            return Err(MetadataError::MissingTrack);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listen {
    /// Local durable-queue id. Never serialized to ListenBrainz.
    pub id: Option<i64>,
    /// Unix timestamp at which this playback session started.
    pub listened_at: i64,
    pub track: TrackMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MetadataError {
    #[error("artist is required")]
    MissingArtist,
    #[error("track title is required")]
    MissingTrack,
    #[error("at least one listen is required")]
    EmptyPayload,
    #[error("a submission may contain at most 1000 listens")]
    TooManyListens,
    #[error("listen metadata could not be serialized")]
    Serialization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error("ListenBrainz rejected the token")]
    Unauthorized,
    #[error("ListenBrainz request failed temporarily with HTTP {0}")]
    Retryable(u16),
    #[error("ListenBrainz rejected the request with HTTP {0}")]
    Rejected(u16),
    #[error("ListenBrainz is unreachable")]
    Network,
    #[error("ListenBrainz returned an invalid response")]
    InvalidResponse,
    #[error("invalid listen metadata: {0}")]
    InvalidMetadata(#[from] MetadataError),
}

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid listen metadata: {0}")]
    InvalidMetadata(#[from] MetadataError),
    #[error("database returned an invalid pending-listen count")]
    InvalidCount,
}

pub trait ScrobblerTransport: Send + 'static {
    fn validate_token(&self, token: &str) -> Result<String, TransportError>;
    fn playing_now(&self, token: &str, track: &TrackMetadata) -> Result<(), TransportError>;
    fn submit(&self, token: &str, listens: &[Listen]) -> Result<(), TransportError>;
}

/// ListenBrainz's documented threshold: half the track or four minutes,
/// whichever comes first.
pub fn should_scrobble(position_ms: i64, duration_ms: i64) -> bool {
    if duration_ms <= 0 {
        return false;
    }
    let half_duration = duration_ms / 2 + duration_ms % 2;
    let threshold = half_duration.min(FOUR_MINUTES_MS);
    position_ms >= threshold
}

/// Adds one completed playback session to the durable FIFO. The caller may
/// safely request a worker flush only after this transaction has returned.
pub fn enqueue(conn: &Connection, listen: &Listen) -> Result<i64, QueueError> {
    listen.track.validate()?;
    let release_name = listen
        .track
        .release_name
        .as_deref()
        .map(str::trim)
        .filter(|release| !release.is_empty());
    conn.execute(
        "INSERT INTO listenbrainz_queue \
         (listened_at, artist_name, track_name, release_name, duration_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            listen.listened_at,
            listen.track.artist_name.trim(),
            listen.track.track_name.trim(),
            release_name,
            listen.track.duration_ms.max(0),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Returns at most one ListenBrainz API batch in stable FIFO order.
pub fn pending(conn: &Connection, limit: usize) -> Result<Vec<Listen>, QueueError> {
    let limit = limit.min(MAX_LISTENS_PER_REQUEST);
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut statement = conn.prepare(
        "SELECT id, listened_at, artist_name, track_name, release_name, duration_ms \
         FROM listenbrainz_queue ORDER BY id ASC LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok(Listen {
            id: Some(row.get(0)?),
            listened_at: row.get(1)?,
            track: TrackMetadata {
                artist_name: row.get(2)?,
                track_name: row.get(3)?,
                release_name: row.get(4)?,
                duration_ms: row.get(5)?,
            },
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(QueueError::from)
}

/// Atomically removes only rows included in a confirmed API submission.
pub fn acknowledge(conn: &Connection, ids: &[i64]) -> Result<(), QueueError> {
    if ids.is_empty() {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    for id in ids {
        transaction.execute("DELETE FROM listenbrainz_queue WHERE id = ?1", params![id])?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn clear_pending(conn: &Connection) -> Result<usize, QueueError> {
    conn.execute("DELETE FROM listenbrainz_queue", [])
        .map_err(QueueError::from)
}

pub fn pending_count(conn: &Connection) -> Result<usize, QueueError> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM listenbrainz_queue", [], |row| {
        row.get(0)
    })?;
    usize::try_from(count).map_err(|_| QueueError::InvalidCount)
}

#[derive(Serialize)]
struct Submission<'a> {
    listen_type: &'static str,
    payload: Vec<ListenPayload<'a>>,
}

#[derive(Serialize)]
struct ListenPayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    listened_at: Option<i64>,
    track_metadata: ApiTrackMetadata<'a>,
}

#[derive(Serialize)]
struct ApiTrackMetadata<'a> {
    artist_name: &'a str,
    track_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_name: Option<&'a str>,
    additional_info: AdditionalInfo,
}

#[derive(Serialize)]
struct AdditionalInfo {
    duration_ms: i64,
    media_player: &'static str,
    submission_client: &'static str,
    submission_client_version: &'static str,
}

fn api_track(track: &TrackMetadata) -> ApiTrackMetadata<'_> {
    ApiTrackMetadata {
        artist_name: track.artist_name.trim(),
        track_name: track.track_name.trim(),
        release_name: track
            .release_name
            .as_deref()
            .map(str::trim)
            .filter(|release| !release.is_empty()),
        additional_info: AdditionalInfo {
            duration_ms: track.duration_ms.max(0),
            media_player: "Reprise",
            submission_client: "Reprise",
            submission_client_version: env!("CARGO_PKG_VERSION"),
        },
    }
}

fn build_playing_now_payload(track: &TrackMetadata) -> Result<serde_json::Value, MetadataError> {
    track.validate()?;
    serde_json::to_value(Submission {
        listen_type: "playing_now",
        payload: vec![ListenPayload {
            listened_at: None,
            track_metadata: api_track(track),
        }],
    })
    .map_err(|_| MetadataError::Serialization)
}

fn build_listen_payload(listens: &[Listen]) -> Result<serde_json::Value, MetadataError> {
    if listens.is_empty() {
        return Err(MetadataError::EmptyPayload);
    }
    if listens.len() > MAX_LISTENS_PER_REQUEST {
        return Err(MetadataError::TooManyListens);
    }
    for listen in listens {
        listen.track.validate()?;
    }
    let payload = listens
        .iter()
        .map(|listen| ListenPayload {
            listened_at: Some(listen.listened_at),
            track_metadata: api_track(&listen.track),
        })
        .collect();
    let listen_type = if listens.len() == 1 {
        "single"
    } else {
        "import"
    };
    serde_json::to_value(Submission {
        listen_type,
        payload,
    })
    .map_err(|_| MetadataError::Serialization)
}

pub struct ListenBrainzClient {
    base_url: String,
    agent: ureq::Agent,
}

impl Default for ListenBrainzClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ListenBrainzClient {
    pub fn new() -> Self {
        Self::with_base_url(LISTENBRAINZ_API_ROOT)
    }

    fn with_base_url(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            agent: ureq::builder()
                .timeout(HTTP_TIMEOUT)
                .user_agent(USER_AGENT)
                .build(),
        }
    }

    fn authorization(token: &str) -> String {
        format!("Token {token}")
    }

    fn validation_url(&self) -> String {
        format!("{}/1/validate-token", self.base_url)
    }

    fn submission_url(&self) -> String {
        format!("{}/1/submit-listens", self.base_url)
    }

    fn classify_status(status: u16) -> TransportError {
        match status {
            401 => TransportError::Unauthorized,
            408 | 429 | 500..=599 => TransportError::Retryable(status),
            _ => TransportError::Rejected(status),
        }
    }

    fn classify(error: &ureq::Error) -> TransportError {
        match error {
            ureq::Error::Status(status, _) => Self::classify_status(*status),
            ureq::Error::Transport(_) => TransportError::Network,
        }
    }

    fn post(&self, token: &str, body: &serde_json::Value) -> Result<(), TransportError> {
        let body = serde_json::to_string(body).map_err(|_| TransportError::InvalidResponse)?;
        self.agent
            .post(&self.submission_url())
            .set("Authorization", &Self::authorization(token))
            .set("Content-Type", "application/json")
            .send_string(&body)
            .map_err(|error| Self::classify(&error))?;
        Ok(())
    }
}

impl ScrobblerTransport for ListenBrainzClient {
    fn validate_token(&self, token: &str) -> Result<String, TransportError> {
        let response = self
            .agent
            .get(&self.validation_url())
            .set("Authorization", &Self::authorization(token))
            .call()
            .map_err(|error| Self::classify(&error))?;
        let mut body = String::new();
        let reader = response.into_reader();
        let mut reader: Take<_> = reader.take(MAX_RESPONSE_BYTES + 1);
        reader
            .read_to_string(&mut body)
            .map_err(|_| TransportError::InvalidResponse)?;
        if body.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(TransportError::InvalidResponse);
        }
        parse_validation_response(&body)
    }

    fn playing_now(&self, token: &str, track: &TrackMetadata) -> Result<(), TransportError> {
        let body = build_playing_now_payload(track)?;
        self.post(token, &body)
    }

    fn submit(&self, token: &str, listens: &[Listen]) -> Result<(), TransportError> {
        let body = build_listen_payload(listens)?;
        self.post(token, &body)
    }
}

fn parse_validation_response(body: &str) -> Result<String, TransportError> {
    #[derive(serde::Deserialize)]
    struct ValidationResponse {
        valid: bool,
        user_name: Option<String>,
    }

    let response: ValidationResponse =
        serde_json::from_str(body).map_err(|_| TransportError::InvalidResponse)?;
    if !response.valid {
        return Err(TransportError::Unauthorized);
    }
    response
        .user_name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .ok_or(TransportError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> TrackMetadata {
        TrackMetadata {
            artist_name: "Massive Attack".to_string(),
            track_name: "Teardrop".to_string(),
            release_name: Some("Mezzanine".to_string()),
            duration_ms: 331_000,
        }
    }

    fn listen() -> Listen {
        Listen {
            id: None,
            listened_at: 1_700_000_000,
            track: track(),
        }
    }

    #[test]
    fn short_track_scrobbles_at_exactly_half_but_not_before() {
        assert!(!should_scrobble(89_999, 180_000));
        assert!(should_scrobble(90_000, 180_000));
    }

    #[test]
    fn long_track_scrobbles_at_four_minutes() {
        assert!(!should_scrobble(239_999, 600_000));
        assert!(should_scrobble(240_000, 600_000));
    }

    #[test]
    fn non_positive_duration_never_scrobbles() {
        assert!(!should_scrobble(1_000, 0));
        assert!(!should_scrobble(1_000, -1));
        assert!(!should_scrobble(0, 1));
        assert!(should_scrobble(1, 1));
    }

    #[test]
    fn metadata_requires_non_blank_artist_and_track() {
        let mut metadata = track();
        assert!(metadata.validate().is_ok());
        metadata.artist_name = "  ".to_string();
        assert_eq!(metadata.validate(), Err(MetadataError::MissingArtist));
        metadata.artist_name = "Artist".to_string();
        metadata.track_name.clear();
        assert_eq!(metadata.validate(), Err(MetadataError::MissingTrack));
    }

    #[test]
    fn playing_now_payload_omits_listened_at() {
        let payload = build_playing_now_payload(&track()).unwrap();
        assert_eq!(payload["listen_type"], "playing_now");
        assert_eq!(payload["payload"].as_array().unwrap().len(), 1);
        assert!(payload["payload"][0].get("listened_at").is_none());
        assert_eq!(
            payload["payload"][0]["track_metadata"]["release_name"],
            "Mezzanine"
        );
    }

    #[test]
    fn single_payload_contains_playback_start_time() {
        let payload = build_listen_payload(&[listen()]).unwrap();
        assert_eq!(payload["listen_type"], "single");
        assert_eq!(payload["payload"][0]["listened_at"], 1_700_000_000);
        assert_eq!(
            payload["payload"][0]["track_metadata"]["track_name"],
            "Teardrop"
        );
    }

    #[test]
    fn multiple_listens_use_import_payload_type() {
        let payload = build_listen_payload(&[listen(), listen()]).unwrap();
        assert_eq!(payload["listen_type"], "import");
        assert_eq!(payload["payload"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn validation_contract_uses_authorization_header_and_returns_user() {
        let client = ListenBrainzClient::with_base_url("http://example.test");
        assert_eq!(
            parse_validation_response(
                r#"{"code":200,"message":"Token valid.","valid":true,"user_name":" marvin "}"#,
            )
            .unwrap(),
            "marvin"
        );
        assert_eq!(
            client.validation_url(),
            "http://example.test/1/validate-token"
        );
        assert_eq!(
            ListenBrainzClient::authorization("super-secret"),
            "Token super-secret"
        );
    }

    #[test]
    fn playing_now_request_targets_submit_endpoint_with_json() {
        let client = ListenBrainzClient::with_base_url("http://example.test/");
        let body = serde_json::to_string(&build_playing_now_payload(&track()).unwrap()).unwrap();
        assert_eq!(
            client.submission_url(),
            "http://example.test/1/submit-listens"
        );
        assert!(body.contains("\"listen_type\":\"playing_now\""));
        assert!(!body.contains("listened_at"));
    }

    #[test]
    fn unauthorized_status_is_classified_without_echoing_token() {
        let error = ListenBrainzClient::classify_status(401);
        assert_eq!(error, TransportError::Unauthorized);
        assert!(!error.to_string().contains("must-not-leak"));
        assert_eq!(
            parse_validation_response(r#"{"valid":false}"#),
            Err(TransportError::Unauthorized)
        );
    }

    #[test]
    fn server_failure_is_retryable_and_bad_payload_is_rejected() {
        assert_eq!(
            ListenBrainzClient::classify_status(503),
            TransportError::Retryable(503)
        );
        assert_eq!(
            ListenBrainzClient::classify_status(400),
            TransportError::Rejected(400)
        );
    }

    fn migrated_conn() -> rusqlite::Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn enqueue_rejects_missing_required_metadata() {
        let conn = migrated_conn();
        let mut invalid = listen();
        invalid.track.artist_name.clear();
        assert!(matches!(
            enqueue(&conn, &invalid),
            Err(QueueError::InvalidMetadata(MetadataError::MissingArtist))
        ));
        assert_eq!(pending_count(&conn).unwrap(), 0);
    }

    #[test]
    fn pending_returns_fifo_order_with_local_ids() {
        let conn = migrated_conn();
        let mut first = listen();
        first.listened_at = 10;
        first.track.track_name = "First".to_string();
        let mut second = listen();
        second.listened_at = 20;
        second.track.track_name = "Second".to_string();
        let first_id = enqueue(&conn, &first).unwrap();
        let second_id = enqueue(&conn, &second).unwrap();

        let queued = pending(&conn, 100).unwrap();
        assert_eq!(
            queued.iter().map(|listen| listen.id).collect::<Vec<_>>(),
            vec![Some(first_id), Some(second_id)]
        );
        assert_eq!(queued[0].track.track_name, "First");
        assert_eq!(queued[1].track.track_name, "Second");
    }

    #[test]
    fn pending_clamps_batch_to_listenbrainz_maximum() {
        let conn = migrated_conn();
        for timestamp in 0..1_005 {
            let mut item = listen();
            item.listened_at = timestamp;
            enqueue(&conn, &item).unwrap();
        }
        assert_eq!(pending(&conn, usize::MAX).unwrap().len(), 1_000);
        assert!(pending(&conn, 0).unwrap().is_empty());
    }

    #[test]
    fn acknowledge_deletes_only_confirmed_ids() {
        let conn = migrated_conn();
        let first = enqueue(&conn, &listen()).unwrap();
        let second = enqueue(&conn, &listen()).unwrap();
        let third = enqueue(&conn, &listen()).unwrap();

        acknowledge(&conn, &[first, third]).unwrap();

        let queued = pending(&conn, 100).unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].id, Some(second));
    }

    #[test]
    fn queue_survives_database_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("library.db");
        {
            let conn = crate::db::open(Some(&path)).unwrap();
            crate::db::migrate(&conn).unwrap();
            enqueue(&conn, &listen()).unwrap();
        }
        let conn = crate::db::open(Some(&path)).unwrap();
        crate::db::migrate(&conn).unwrap();
        assert_eq!(pending(&conn, 100).unwrap(), vec![listen_with_id(1)]);
    }

    #[test]
    fn clear_pending_returns_deleted_count() {
        let conn = migrated_conn();
        enqueue(&conn, &listen()).unwrap();
        enqueue(&conn, &listen()).unwrap();
        assert_eq!(clear_pending(&conn).unwrap(), 2);
        assert_eq!(clear_pending(&conn).unwrap(), 0);
        assert_eq!(pending_count(&conn).unwrap(), 0);
    }

    fn listen_with_id(id: i64) -> Listen {
        let mut item = listen();
        item.id = Some(id);
        item
    }
}
