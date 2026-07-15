//! Read-only Last.fm statistics client. Fetches public user stats
//! (top artists, top albums, weekly chart list) and maps them into the
//! shared [`RemoteStats`] type.
//!
//! All calls are blocking (`ureq`) and intended to run on a background
//! thread — never on the GTK main loop.
//!
//! The API key is sourced from the bundled compile-time credential used
//! by the scrobbling module (see [`crate::scrobbling::lastfm::BUNDLED_API_KEY`]).
//! Read-only stats endpoints do not require a session key or shared secret.

use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use super::remote_stats::{RemoteStats, RemoteStatsError, StatsSource};
use super::stats_screen::{HeadlineTotals, MonthlyListens, TopAlbum, TopArtist};

const API_ROOT: &str = "https://ws.audioscrobbler.com/2.0/";
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: u64 = 512 * 1024;
const USER_AGENT: &str = concat!("Reprise/", env!("CARGO_PKG_VERSION"));

/// Supported time periods for Last.fm statistics queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfmPeriod {
    SevenDays,
    OneMonth,
    ThreeMonths,
    SixMonths,
    TwelveMonths,
    Overall,
}

impl LfmPeriod {
    fn as_param(self) -> &'static str {
        match self {
            Self::SevenDays => "7day",
            Self::OneMonth => "1month",
            Self::ThreeMonths => "3month",
            Self::SixMonths => "6month",
            Self::TwelveMonths => "12month",
            Self::Overall => "overall",
        }
    }
}

/// Blocking Last.fm stats client. Requires an API key (read-only; no
/// session key needed).
pub struct LfmStatsClient {
    api_root: String,
    api_key: String,
    agent: ureq::Agent,
}

impl LfmStatsClient {
    /// Creates a client from an explicit API key.
    pub fn new(api_key: &str) -> Result<Self, RemoteStatsError> {
        Self::with_api_root(API_ROOT, api_key)
    }

    /// Creates a client from the bundled compile-time API key, returning
    /// `None` when the build was not configured with
    /// `REPRISE_LASTFM_API_KEY`.
    pub fn bundled() -> Option<Result<Self, RemoteStatsError>> {
        let api_key = crate::scrobbling::lastfm::BUNDLED_API_KEY?;
        Some(Self::new(api_key))
    }

    /// Constructs a client pointing at a custom API root (for testing).
    #[doc(hidden)]
    pub fn with_api_root(api_root: &str, api_key: &str) -> Result<Self, RemoteStatsError> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(RemoteStatsError::ParseError(
                "Last.fm API key is required".to_string(),
            ));
        }
        Ok(Self {
            api_root: format!("{}/", api_root.trim_end_matches('/')),
            api_key,
            agent: ureq::builder()
                .timeout(HTTP_TIMEOUT)
                .user_agent(USER_AGENT)
                .build(),
        })
    }

    /// Fetches top artists, top albums, and (approximate) monthly
    /// listening activity for `username`, returning a unified [`RemoteStats`].
    pub fn fetch_stats(
        &self,
        username: &str,
        period: LfmPeriod,
    ) -> Result<RemoteStats, RemoteStatsError> {
        let top_artists = self.fetch_top_artists(username, period, 10)?;
        let top_albums = self.fetch_top_albums(username, period, 10)?;
        let monthly = self.fetch_monthly_activity(username)?;

        let total_plays: i64 = top_artists.iter().map(|a| a.plays).sum();

        Ok(RemoteStats {
            source: StatsSource::LastFm {
                username: username.to_string(),
            },
            headline: HeadlineTotals {
                // Last.fm does not expose total listening time in its
                // public API; leave at zero.
                total_ms: 0,
                total_plays,
            },
            top_artists,
            top_albums,
            monthly,
        })
    }

    fn fetch_top_artists(
        &self,
        username: &str,
        period: LfmPeriod,
        limit: u32,
    ) -> Result<Vec<TopArtist>, RemoteStatsError> {
        let body = self.api_call(&[
            ("method", "user.getTopArtists"),
            ("user", username),
            ("period", period.as_param()),
            ("limit", &limit.to_string()),
        ], username)?;
        parse_top_artists(&body)
    }

    fn fetch_top_albums(
        &self,
        username: &str,
        period: LfmPeriod,
        limit: u32,
    ) -> Result<Vec<TopAlbum>, RemoteStatsError> {
        let body = self.api_call(&[
            ("method", "user.getTopAlbums"),
            ("user", username),
            ("period", period.as_param()),
            ("limit", &limit.to_string()),
        ], username)?;
        parse_top_albums(&body)
    }

    /// Fetches the weekly chart list and derives monthly listen counts.
    /// Last.fm provides weekly chart timestamps; we aggregate them into
    /// calendar months.
    fn fetch_monthly_activity(
        &self,
        username: &str,
    ) -> Result<Vec<MonthlyListens>, RemoteStatsError> {
        let body = self.api_call(&[
            ("method", "user.getWeeklyChartList"),
            ("user", username),
        ], username)?;
        parse_weekly_chart_list(&body)
    }

    fn api_call(
        &self,
        params: &[(&str, &str)],
        username: &str,
    ) -> Result<String, RemoteStatsError> {
        let mut url =
            url::Url::parse(&self.api_root).map_err(|e| RemoteStatsError::ParseError(e.to_string()))?;
        {
            let mut query = url.query_pairs_mut();
            for &(key, value) in params {
                query.append_pair(key, value);
            }
            query.append_pair("api_key", &self.api_key);
            query.append_pair("format", "json");
        }
        let response = self
            .agent
            .get(url.as_str())
            .call()
            .map_err(|error| classify_error(&error, username))?;
        let mut body = String::new();
        response
            .into_reader()
            .take(MAX_RESPONSE_BYTES)
            .read_to_string(&mut body)
            .map_err(|error| RemoteStatsError::Network(error.to_string()))?;

        // Last.fm returns errors as JSON with an "error" field.
        check_api_error(&body, username)?;

        Ok(body)
    }
}

fn classify_error(error: &ureq::Error, username: &str) -> RemoteStatsError {
    match error {
        ureq::Error::Status(404, _) => RemoteStatsError::UserNotFound(username.to_string()),
        ureq::Error::Status(status, _) => RemoteStatsError::Network(format!("HTTP {status}")),
        ureq::Error::Transport(transport) => RemoteStatsError::Network(transport.to_string()),
    }
}

/// Last.fm signals user-not-found and other errors inside the JSON body
/// with an `"error"` field (e.g. error code 6 = user not found).
fn check_api_error(body: &str, username: &str) -> Result<(), RemoteStatsError> {
    let root: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Ok(()), // not JSON — let the caller parse and fail
    };
    if let Some(code) = root.get("error").and_then(Value::as_u64) {
        let message = root
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error")
            .to_string();
        return match code {
            6 => Err(RemoteStatsError::UserNotFound(username.to_string())),
            _ => Err(RemoteStatsError::Network(format!(
                "Last.fm API error {code}: {message}"
            ))),
        };
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// JSON parsing helpers
// ---------------------------------------------------------------------------

fn parse_top_artists(body: &str) -> Result<Vec<TopArtist>, RemoteStatsError> {
    let root: Value =
        serde_json::from_str(body).map_err(|e| RemoteStatsError::ParseError(e.to_string()))?;
    let artists = root
        .pointer("/topartists/artist")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|entry| {
            let artist = entry.get("name")?.as_str()?.to_string();
            let plays = entry.get("playcount")?.as_str()?.parse::<i64>().ok()?;
            Some(TopArtist { artist, plays, total_ms: 0, representative_track_path: String::new() })
        })
        .collect();
    Ok(artists)
}

fn parse_top_albums(body: &str) -> Result<Vec<TopAlbum>, RemoteStatsError> {
    let root: Value =
        serde_json::from_str(body).map_err(|e| RemoteStatsError::ParseError(e.to_string()))?;
    let albums = root
        .pointer("/topalbums/album")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|entry| {
            let album = entry.get("name")?.as_str()?.to_string();
            let album_artist = entry
                .pointer("/artist/name")
                .and_then(Value::as_str)?
                .to_string();
            let plays = entry.get("playcount")?.as_str()?.parse::<i64>().ok()?;
            Some(TopAlbum {
                album,
                album_artist,
                plays,
                total_ms: 0,
                track_path: String::new(),
            })
        })
        .collect();
    Ok(albums)
}

/// Parses Last.fm's `user.getWeeklyChartList` response and aggregates
/// the weekly chart windows into calendar-month buckets.
///
/// Each chart entry has `from` and `to` unix timestamps. We bucket each
/// entry by the calendar month (UTC) of its `from` timestamp and count
/// one entry per week. This gives a rough monthly listen-activity
/// timeseries. We retain only the most recent 12 months.
fn parse_weekly_chart_list(body: &str) -> Result<Vec<MonthlyListens>, RemoteStatsError> {
    let root: Value =
        serde_json::from_str(body).map_err(|e| RemoteStatsError::ParseError(e.to_string()))?;
    let charts = root
        .pointer("/weeklychartlist/chart")
        .and_then(Value::as_array);

    let Some(charts) = charts else {
        return Ok(Vec::new());
    };

    // Aggregate weekly entries into YYYY-MM buckets. Entries with
    // missing or unparseable `from` fields are silently skipped.
    let mut monthly: BTreeMap<String, i64> = BTreeMap::new();
    for entry in charts {
        let from_ts = entry
            .get("from")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<i64>().ok());
        if let Some(ts) = from_ts {
            let year_month = unix_to_year_month(ts);
            *monthly.entry(year_month).or_insert(0) += 1;
        }
    }

    // Take only the most recent 12 months.
    let buckets: Vec<MonthlyListens> = monthly
        .into_iter()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|(year_month, listens)| MonthlyListens {
            year_month,
            total_ms: 0,
            listens,
        })
        .collect();

    Ok(buckets)
}

/// Converts a unix timestamp to a `YYYY-MM` string (UTC).
fn unix_to_year_month(ts: i64) -> String {
    use chrono::{TimeZone, Utc};
    let dt = Utc.timestamp_opt(ts, 0).single().unwrap_or_default();
    format!("{}", dt.format("%Y-%m"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_top_artists_extracts_names_and_playcounts() {
        let json = r#"{
            "topartists": {
                "artist": [
                    {"name": "Radiohead", "playcount": "142", "url": "..."},
                    {"name": "Bjork", "playcount": "87", "url": "..."}
                ]
            }
        }"#;
        let result = parse_top_artists(json).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].artist, "Radiohead");
        assert_eq!(result[0].plays, 142);
        assert_eq!(result[1].artist, "Bjork");
        assert_eq!(result[1].plays, 87);
    }

    #[test]
    fn parse_top_artists_returns_empty_on_missing_data() {
        let json = r#"{"topartists": {}}"#;
        let result = parse_top_artists(json).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_top_albums_extracts_nested_artist() {
        let json = r#"{
            "topalbums": {
                "album": [
                    {
                        "name": "OK Computer",
                        "playcount": "55",
                        "artist": {"name": "Radiohead", "url": "..."}
                    }
                ]
            }
        }"#;
        let result = parse_top_albums(json).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].album, "OK Computer");
        assert_eq!(result[0].album_artist, "Radiohead");
        assert_eq!(result[0].plays, 55);
    }

    #[test]
    fn parse_weekly_chart_list_aggregates_into_months() {
        // Two entries in the same month (Jan 2026), one in Feb 2026.
        let json = r##"{
            "weeklychartlist": {
                "chart": [
                    {"#text": "", "from": "1767225600", "to": "1767830400"},
                    {"#text": "", "from": "1767830400", "to": "1768435200"},
                    {"#text": "", "from": "1769904000", "to": "1770508800"}
                ]
            }
        }"##;
        let result = parse_weekly_chart_list(json).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].year_month, "2026-01");
        assert_eq!(result[0].listens, 2);
        assert_eq!(result[1].year_month, "2026-02");
        assert_eq!(result[1].listens, 1);
    }

    #[test]
    fn parse_weekly_chart_list_empty_on_missing_data() {
        let json = r#"{"weeklychartlist": {}}"#;
        let result = parse_weekly_chart_list(json).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn check_api_error_detects_user_not_found() {
        let body = r#"{"error": 6, "message": "User not found"}"#;
        let err = check_api_error(body, "ghost").unwrap_err();
        assert!(matches!(err, RemoteStatsError::UserNotFound(_)));
    }

    #[test]
    fn check_api_error_passes_valid_responses() {
        let body = r#"{"topartists": {"artist": []}}"#;
        assert!(check_api_error(body, "user").is_ok());
    }

    #[test]
    fn check_api_error_maps_other_codes_to_network() {
        let body = r#"{"error": 10, "message": "Invalid API key"}"#;
        let err = check_api_error(body, "user").unwrap_err();
        assert!(matches!(err, RemoteStatsError::Network(_)));
    }

    #[test]
    fn lfm_period_param_values() {
        assert_eq!(LfmPeriod::SevenDays.as_param(), "7day");
        assert_eq!(LfmPeriod::OneMonth.as_param(), "1month");
        assert_eq!(LfmPeriod::ThreeMonths.as_param(), "3month");
        assert_eq!(LfmPeriod::SixMonths.as_param(), "6month");
        assert_eq!(LfmPeriod::TwelveMonths.as_param(), "12month");
        assert_eq!(LfmPeriod::Overall.as_param(), "overall");
    }

    #[test]
    fn client_rejects_empty_api_key() {
        let result = LfmStatsClient::new("  ");
        assert!(result.is_err());
    }
}
