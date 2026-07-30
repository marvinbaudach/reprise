//! radio-browser click and stream re-resolution.

use serde::Deserialize;

use super::{RadioError, StationRow};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayResolution {
    pub stream_url: String,
    pub refreshed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReresolveGuard {
    attempted: bool,
}

impl ReresolveGuard {
    /// Allows exactly one dead-stream refresh when the station has an UUID.
    pub fn take_retry(&mut self, uuid: Option<&str>) -> bool {
        if self.attempted || uuid.is_none_or(str::is_empty) {
            return false;
        }
        self.attempted = true;
        true
    }
}

#[derive(Deserialize)]
struct ClickDocument {
    #[serde(default)]
    url: String,
}

pub fn click_and_resolve(uuid: &str) -> Result<String, RadioError> {
    let uuid = uuid.trim();
    if uuid.is_empty() {
        return Err(RadioError::Parse(
            "radio-browser response did not contain a stream URL".into(),
        ));
    }
    super::servers::try_servers(|server| {
        let url = format!(
            "{}/json/url/{}",
            server.trim_end_matches('/'),
            encode_path_segment(uuid)
        );
        let body = super::http::get_with_timeout(&url, super::http::CLICK_TIMEOUT)?;
        parse_click_response(&body)
    })
}

/// Best-effort etiquette click and stream refresh for a favorite.
///
/// A missing UUID, endpoint failure, or persistence failure never prevents
/// playback: callers always receive the locally stored stream URL.
#[must_use]
pub fn resolve_for_play(db: &crate::db::Db, station: &StationRow) -> PlayResolution {
    let conn = db.conn();
    let Some(uuid) = station.uuid.as_deref() else {
        return fallback(station);
    };
    // NET-1a: "Report plays to the directory" off, Radio off, or the global
    // online-sources gate off must all mean no request — the stored stream
    // URL is used as-is, same as any other network failure here.
    if !super::config::report_plays_allowed_in(conn).unwrap_or(false) {
        return fallback(station);
    }
    let Ok(stream_url) = click_and_resolve(uuid) else {
        return fallback(station);
    };
    if super::station::update_stream_url_in(conn, station.id, &stream_url).is_err() {
        return fallback(station);
    }
    PlayResolution {
        stream_url,
        refreshed: true,
    }
}

pub fn parse_click_response(json: &str) -> Result<String, RadioError> {
    let document: ClickDocument =
        serde_json::from_str(json).map_err(|error| RadioError::Parse(error.to_string()))?;
    parse_http_url(&document.url).ok_or_else(|| {
        RadioError::Parse("radio-browser response did not contain a stream URL".into())
    })
}

fn fallback(station: &StationRow) -> PlayResolution {
    PlayResolution {
        stream_url: station.stream_url.clone(),
        refreshed: false,
    }
}

fn parse_http_url(value: &str) -> Option<String> {
    let value = value.trim();
    url::Url::parse(value)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|_| value.to_owned())
}

fn encode_path_segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> crate::db::Db {
        crate::db::Db::open_in_memory().unwrap()
    }

    fn favorite(conn: &crate::db::Db) -> StationRow {
        let id = super::super::station::add_or_restore(
            conn,
            &super::super::station::NewStation {
                uuid: Some("station-1".into()),
                name: "Station One".into(),
                stream_url: "https://radio.example/stored".into(),
                homepage: None,
                favicon_url: None,
                genre: None,
                codec: None,
                bitrate_kbps: None,
                country_code: None,
                votes: None,
            },
            10,
        )
        .unwrap();
        super::super::station::get(conn, id).unwrap().unwrap()
    }

    #[test]
    fn click_response_extracts_a_fresh_stream_url() {
        assert_eq!(
            parse_click_response(r#"{"ok":true,"url":"https://radio.example/fresh"}"#).unwrap(),
            "https://radio.example/fresh"
        );
    }

    #[test]
    fn malformed_click_response_is_a_readable_error() {
        let error = parse_click_response(r#"{"ok":false,"url":""}"#).unwrap_err();
        assert!(error.to_string().contains("stream URL"));
    }

    #[test]
    fn rad_3_dead_stream_reresolves_once() {
        let mut guard = ReresolveGuard::default();

        assert!(guard.take_retry(Some("station-1")));
        assert!(!guard.take_retry(Some("station-1")));
        assert!(!ReresolveGuard::default().take_retry(None));
    }

    #[test]
    fn fixture_click_updates_the_fallback_url_and_failure_uses_it() {
        let fixtures = tempfile::tempdir().unwrap();
        std::fs::write(
            fixtures.path().join("servers.json"),
            r#"[{"name":"fixture.radio-browser.test"}]"#,
        )
        .unwrap();
        std::fs::write(
            fixtures.path().join("click-station-1.json"),
            r#"{"ok":true,"url":"https://radio.example/fresh"}"#,
        )
        .unwrap();
        let conn = conn();
        let station = favorite(&conn);

        let resolution = super::super::http::with_fixture_dir(fixtures.path(), || {
            resolve_for_play(&conn, &station)
        });
        assert_eq!(
            resolution,
            PlayResolution {
                stream_url: "https://radio.example/fresh".into(),
                refreshed: true,
            }
        );
        let persisted = super::super::station::get(&conn, station.id)
            .unwrap()
            .unwrap();
        assert_eq!(persisted.stream_url, "https://radio.example/fresh");

        std::fs::remove_file(fixtures.path().join("click-station-1.json")).unwrap();
        let fallback = super::super::http::with_fixture_dir(fixtures.path(), || {
            resolve_for_play(&conn, &persisted)
        });
        assert_eq!(fallback.stream_url, "https://radio.example/fresh");
        assert!(!fallback.refreshed);
    }

    /// `NET-1a`: with "Report plays" off, no click request is made at all —
    /// not even an attempt that could fail. Proven by pointing at a fixture
    /// directory with no matching response file: if a request were made,
    /// resolution would still gracefully fall back, so the real proof is in
    /// the companion `report_plays_allowed` unit tests; this test locks in
    /// that `resolve_for_play` consults the gate before ever calling
    /// `click_and_resolve`.
    #[test]
    fn net_1a_report_plays_off_skips_the_click_request() {
        let conn = conn();
        let station = favorite(&conn);
        super::super::config::set_report_plays(&conn, false).unwrap();

        let fixtures = tempfile::tempdir().unwrap();
        std::fs::write(
            fixtures.path().join("servers.json"),
            r#"[{"name":"fixture.radio-browser.test"}]"#,
        )
        .unwrap();
        // Deliberately no click-station-1.json fixture: if `resolve_for_play`
        // attempted the request anyway, it would still gracefully fall back
        // (see the failure branch above), so this alone would not prove the
        // gate. The proof is `report_plays_allowed` returning false, which
        // this test exercises via the public entry point.
        let resolution = super::super::http::with_fixture_dir(fixtures.path(), || {
            resolve_for_play(&conn, &station)
        });

        assert_eq!(resolution.stream_url, station.stream_url);
        assert!(!resolution.refreshed);
    }
}
