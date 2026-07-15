//! Read-only ListenBrainz statistics client. Fetches public user stats
//! (top artists, top releases, listening activity) and maps them into the
//! shared [`RemoteStats`] type.
//!
//! All calls are blocking (`ureq`) and intended to run on a background
//! thread — never on the GTK main loop.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use super::remote_stats::{RemoteStats, RemoteStatsError, StatsSource};
use super::stats_screen::{HeadlineTotals, MonthlyListens, TopAlbum, TopArtist};

const API_ROOT: &str = "https://api.listenbrainz.org";
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: u64 = 512 * 1024;
const USER_AGENT: &str = concat!("Reprise/", env!("CARGO_PKG_VERSION"));

/// Supported time ranges for ListenBrainz statistics queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LbRange {
    ThisWeek,
    ThisMonth,
    ThisYear,
    AllTime,
}

impl LbRange {
    fn as_param(self) -> &'static str {
        match self {
            Self::ThisWeek => "this_week",
            Self::ThisMonth => "this_month",
            Self::ThisYear => "this_year",
            Self::AllTime => "all_time",
        }
    }
}

/// Blocking ListenBrainz stats client. Construct once, reuse across calls.
pub struct LbStatsClient {
    base_url: String,
    agent: ureq::Agent,
}

impl Default for LbStatsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LbStatsClient {
    pub fn new() -> Self {
        Self::with_api_root(API_ROOT)
    }

    /// Constructs a client pointing at a custom API root (for testing).
    #[doc(hidden)]
    pub fn with_api_root(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            agent: ureq::builder()
                .timeout(HTTP_TIMEOUT)
                .user_agent(USER_AGENT)
                .build(),
        }
    }

    /// Fetches top artists, top albums/releases, and monthly listening
    /// activity for `username`, returning a unified [`RemoteStats`].
    pub fn fetch_stats(
        &self,
        username: &str,
        range: LbRange,
    ) -> Result<RemoteStats, RemoteStatsError> {
        let top_artists = self.fetch_top_artists(username, range, 10)?;
        let top_albums = self.fetch_top_releases(username, range, 10)?;
        let monthly = self.fetch_listening_activity(username, range)?;

        let total_plays: i64 = top_artists.iter().map(|a| a.plays).sum();

        Ok(RemoteStats {
            source: StatsSource::ListenBrainz {
                username: username.to_string(),
            },
            headline: HeadlineTotals {
                // ListenBrainz public stats do not expose total listening
                // time; approximate from the monthly timeseries when
                // available, otherwise leave at zero.
                total_ms: monthly.iter().map(|m| m.total_ms).sum(),
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
        range: LbRange,
        count: u32,
    ) -> Result<Vec<TopArtist>, RemoteStatsError> {
        let url = format!(
            "{}/1/stats/user/{}/artists?count={}&range={}",
            self.base_url,
            username,
            count,
            range.as_param(),
        );
        let body = self.get(&url, username)?;
        parse_top_artists(&body)
    }

    fn fetch_top_releases(
        &self,
        username: &str,
        range: LbRange,
        count: u32,
    ) -> Result<Vec<TopAlbum>, RemoteStatsError> {
        let url = format!(
            "{}/1/stats/user/{}/releases?count={}&range={}",
            self.base_url,
            username,
            count,
            range.as_param(),
        );
        let body = self.get(&url, username)?;
        parse_top_releases(&body)
    }

    fn fetch_listening_activity(
        &self,
        username: &str,
        range: LbRange,
    ) -> Result<Vec<MonthlyListens>, RemoteStatsError> {
        let url = format!(
            "{}/1/stats/user/{}/listening-activity?range={}",
            self.base_url,
            username,
            range.as_param(),
        );
        let body = self.get(&url, username)?;
        parse_listening_activity(&body)
    }

    fn get(&self, url: &str, username: &str) -> Result<String, RemoteStatsError> {
        let response = self.agent.get(url).call().map_err(|error| match &error {
            ureq::Error::Status(404, _) => {
                RemoteStatsError::UserNotFound(username.to_string())
            }
            ureq::Error::Status(status, _) => {
                RemoteStatsError::Network(format!("HTTP {status}"))
            }
            ureq::Error::Transport(transport) => {
                RemoteStatsError::Network(transport.to_string())
            }
        })?;
        let mut body = String::new();
        response
            .into_reader()
            .take(MAX_RESPONSE_BYTES)
            .read_to_string(&mut body)
            .map_err(|error| RemoteStatsError::Network(error.to_string()))?;
        Ok(body)
    }
}

// ---------------------------------------------------------------------------
// JSON parsing helpers
// ---------------------------------------------------------------------------

fn parse_top_artists(body: &str) -> Result<Vec<TopArtist>, RemoteStatsError> {
    let root: Value =
        serde_json::from_str(body).map_err(|e| RemoteStatsError::ParseError(e.to_string()))?;
    let artists = root
        .pointer("/payload/artists")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|entry| {
            let artist = entry.get("artist_name")?.as_str()?.to_string();
            let plays = entry.get("listen_count")?.as_i64()?;
            Some(TopArtist { artist, plays })
        })
        .collect();
    Ok(artists)
}

fn parse_top_releases(body: &str) -> Result<Vec<TopAlbum>, RemoteStatsError> {
    let root: Value =
        serde_json::from_str(body).map_err(|e| RemoteStatsError::ParseError(e.to_string()))?;
    let albums = root
        .pointer("/payload/releases")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|entry| {
            let album = entry.get("release_name")?.as_str()?.to_string();
            let album_artist = entry.get("artist_name")?.as_str()?.to_string();
            let plays = entry.get("listen_count")?.as_i64()?;
            Some(TopAlbum {
                album,
                album_artist,
                plays,
            })
        })
        .collect();
    Ok(albums)
}

fn parse_listening_activity(body: &str) -> Result<Vec<MonthlyListens>, RemoteStatsError> {
    let root: Value =
        serde_json::from_str(body).map_err(|e| RemoteStatsError::ParseError(e.to_string()))?;
    let entries = root
        .pointer("/payload/listening_activity")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|entry| {
            // ListenBrainz listening-activity entries contain
            // `from_ts` / `to_ts` (unix) and `listen_count`. We derive
            // `YYYY-MM` from `from_ts` and map `listen_count` to `listens`.
            // Total listening milliseconds are not provided; we leave them
            // at zero.
            let from_ts = entry.get("from_ts")?.as_i64()?;
            let listens = entry.get("listen_count")?.as_i64()?;
            let year_month = unix_to_year_month(from_ts);
            Some(MonthlyListens {
                year_month,
                total_ms: 0,
                listens,
            })
        })
        .collect();
    Ok(entries)
}

/// Converts a unix timestamp to a `YYYY-MM` string (UTC). Uses basic
/// arithmetic to avoid pulling in a full datetime crate for this one
/// conversion.
fn unix_to_year_month(ts: i64) -> String {
    // chrono is already in deps, so use it for correctness.
    use chrono::{TimeZone, Utc};
    let dt = Utc.timestamp_opt(ts, 0).single().unwrap_or_default();
    format!("{}", dt.format("%Y-%m"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_top_artists_extracts_names_and_counts() {
        let json = r#"{
            "payload": {
                "artists": [
                    {"artist_name": "Radiohead", "listen_count": 42},
                    {"artist_name": "Portishead", "listen_count": 18}
                ]
            }
        }"#;
        let result = parse_top_artists(json).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].artist, "Radiohead");
        assert_eq!(result[0].plays, 42);
        assert_eq!(result[1].artist, "Portishead");
        assert_eq!(result[1].plays, 18);
    }

    #[test]
    fn parse_top_artists_returns_empty_on_missing_payload() {
        let json = r#"{"payload": {}}"#;
        let result = parse_top_artists(json).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_top_releases_extracts_albums() {
        let json = r#"{
            "payload": {
                "releases": [
                    {
                        "release_name": "OK Computer",
                        "artist_name": "Radiohead",
                        "listen_count": 30
                    }
                ]
            }
        }"#;
        let result = parse_top_releases(json).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].album, "OK Computer");
        assert_eq!(result[0].album_artist, "Radiohead");
        assert_eq!(result[0].plays, 30);
    }

    #[test]
    fn parse_listening_activity_maps_timestamps_to_year_month() {
        // 2026-01-01 00:00:00 UTC = 1767225600
        let json = r#"{
            "payload": {
                "listening_activity": [
                    {"from_ts": 1767225600, "to_ts": 1769904000, "listen_count": 55}
                ]
            }
        }"#;
        let result = parse_listening_activity(json).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].year_month, "2026-01");
        assert_eq!(result[0].listens, 55);
        assert_eq!(result[0].total_ms, 0);
    }

    #[test]
    fn parse_top_artists_rejects_malformed_json() {
        let result = parse_top_artists("not json");
        assert!(result.is_err());
    }

    #[test]
    fn unix_to_year_month_converts_correctly() {
        // 2025-08-01 00:00:00 UTC
        assert_eq!(unix_to_year_month(1_754_006_400), "2025-08");
        // 2026-07-01 00:00:00 UTC
        assert_eq!(unix_to_year_month(1_782_864_000), "2026-07");
    }

    #[test]
    fn lb_range_param_values() {
        assert_eq!(LbRange::ThisWeek.as_param(), "this_week");
        assert_eq!(LbRange::ThisMonth.as_param(), "this_month");
        assert_eq!(LbRange::ThisYear.as_param(), "this_year");
        assert_eq!(LbRange::AllTime.as_param(), "all_time");
    }
}
