//! Synchronous, capability-gated online-source discovery (Block H-A).
//!
//! Mirrors the GNOME add dialogs (`SRC-6`/`SRC-9`) so both surfaces search
//! and filter the exact same way: one provider per call, no mixed result
//! list, already-subscribed sources dropped by the same stable-identity
//! rules, and an optional YouTube subscriber count that is omitted rather
//! than shown as zero when the provider hides it. All provider logic is
//! reused from `reprise-core` — this module only projects results into a
//! leak-safe response and applies the `sources:manage` capability gate,
//! exactly like `source_actions::manage_podcasts`/`manage_radio`.

use std::path::Path;

use rmcp::schemars;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::data::{self, DataError};
use crate::source_actions::{podcast_source_error, radio_source_error, required_nonempty};

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SearchSourcesParams {
    /// One of: `rss`, `youtube`, `radio`. `SRC-6`: a search is bound to
    /// exactly one provider — there is no mixed result list.
    pub provider: String,
    /// Free-text search terms.
    pub query: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiscoveryCandidateDto {
    Rss {
        title: String,
        author: Option<String>,
        episode_count: Option<u32>,
        url: String,
    },
    Youtube {
        title: String,
        url: String,
        matching_video_count: usize,
        /// `SRC-9`: present only when the channel publishes a subscriber
        /// count. Absent, null, and malformed counts are all omitted here —
        /// never rendered as zero.
        #[serde(skip_serializing_if = "Option::is_none")]
        subscriber_count: Option<u64>,
    },
    Radio {
        uuid: String,
        name: String,
        url: String,
        genre: Option<String>,
        codec: Option<String>,
        bitrate_kbps: Option<i64>,
        country_code: Option<String>,
        votes: i64,
    },
}

#[derive(Debug, Serialize)]
pub struct SearchSourcesResult {
    pub provider: &'static str,
    pub query: String,
    pub candidates: Vec<DiscoveryCandidateDto>,
    pub total: usize,
}

impl SearchSourcesResult {
    pub fn summary(&self) -> String {
        format!(
            "Found {} new {} result(s) for '{}'",
            self.total, self.provider, self.query
        )
    }
}

pub fn search_sources(
    path: &Path,
    granted_at_startup: bool,
    params: &SearchSourcesParams,
) -> Result<SearchSourcesResult, DataError> {
    let conn = data::open(path)?;
    // Search performs network (iTunes, radio-browser) and subprocess
    // (yt-dlp) work, so it is gated at least as strictly as the
    // add/edit/remove/refresh mutations: the same capability, the same
    // startup-snapshot-plus-live-value effective check.
    let allowed = crate::capability::sources_manage_effective(&conn, granted_at_startup)
        .map_err(DataError::Db)?;
    if !allowed {
        return Err(DataError::CapabilityDenied("sources:manage"));
    }
    let query = required_nonempty(Some(params.query.as_str()), "query is required")?;
    let candidates = match params.provider.as_str() {
        "rss" => search_rss(&conn, query)?,
        "youtube" => search_youtube(&conn, query)?,
        "radio" => search_radio(&conn, query)?,
        other => {
            return Err(DataError::InvalidInput(format!(
                "unknown provider '{other}'; expected rss, youtube, or radio"
            )))
        }
    };
    let total = candidates.len();
    Ok(SearchSourcesResult {
        provider: static_provider_name(&params.provider),
        query: query.to_owned(),
        candidates,
        total,
    })
}

fn static_provider_name(provider: &str) -> &'static str {
    match provider {
        "rss" => "rss",
        "youtube" => "youtube",
        _ => "radio",
    }
}

fn search_rss(conn: &Connection, query: &str) -> Result<Vec<DiscoveryCandidateDto>, DataError> {
    use reprise_core::podcasts::{discovery, itunes, PodcastKind};

    let locale = locale_from_env();
    let rows = itunes::search(query, &locale).map_err(|error| podcast_source_error(&error))?;
    let subscribed = discovery::active_source_keys(conn);
    Ok(rows
        .into_iter()
        .filter(|row| {
            !discovery::source_is_subscribed(PodcastKind::Rss, &row.feed_url, &[], &subscribed)
        })
        .filter_map(|row| {
            sanitize_url(&row.feed_url).map(|url| DiscoveryCandidateDto::Rss {
                title: row.title,
                author: row.author,
                episode_count: row.episode_count,
                url,
            })
        })
        .collect())
}

fn search_youtube(conn: &Connection, query: &str) -> Result<Vec<DiscoveryCandidateDto>, DataError> {
    use reprise_core::podcasts::{self, discovery, PodcastKind};

    let config = podcasts::config::load(conn).map_err(DataError::Db)?;
    if !config.youtube_enabled {
        return Err(DataError::InvalidInput(
            "YouTube sources are disabled in Reprise preferences".to_owned(),
        ));
    }
    let rows = podcasts::ytdlp::YtDlp::discover(config.ytdlp_path.as_deref())
        .search_channels(query)
        .map_err(|error| podcast_source_error(&error))?;
    let subscribed = discovery::active_source_keys(conn);
    Ok(rows
        .into_iter()
        .filter(|row| {
            !discovery::source_is_subscribed(
                PodcastKind::Youtube,
                &row.url,
                &row.matching_video_ids,
                &subscribed,
            )
        })
        .filter_map(|row| {
            sanitize_url(&row.url).map(|url| DiscoveryCandidateDto::Youtube {
                title: row.title,
                url,
                matching_video_count: row.matching_video_count,
                subscriber_count: row.follower_count,
            })
        })
        .collect())
}

fn search_radio(conn: &Connection, query: &str) -> Result<Vec<DiscoveryCandidateDto>, DataError> {
    use reprise_core::radio;

    let order = radio::config::load(conn).unwrap_or_default().search_order;
    let candidates =
        radio::search::search(query, order).map_err(|error| radio_source_error(&error))?;
    let favorites: Vec<(String, String)> = radio::station::list(conn)
        .map_err(DataError::Db)?
        .into_iter()
        .map(|row| (row.uuid.unwrap_or_default(), row.stream_url))
        .collect();
    let visible = radio::search::filter_new_stations(candidates, &favorites);
    Ok(visible
        .into_iter()
        .filter_map(|candidate| {
            sanitize_url(&candidate.url_resolved).map(|url| DiscoveryCandidateDto::Radio {
                uuid: candidate.uuid,
                name: candidate.name,
                url,
                genre: candidate.genre,
                codec: candidate.codec,
                bitrate_kbps: candidate.bitrate_kbps,
                country_code: candidate.country_code,
                votes: candidate.votes,
            })
        })
        .collect())
}

/// Leak guard for a candidate's identifying URL (requirement: never a signed
/// URL or embedded credential in a tool response). Drops the fragment and any
/// embedded userinfo (a strong signal of an embedded credential) and rejects
/// anything that is not a plain HTTP(S) URL outright.
///
/// The query string is deliberately kept: unlike a stored, already-active
/// subscription's feed URL (which the `reprise://podcasts` resource omits
/// because it may have accumulated a private per-subscriber token over time),
/// this is a fresh public search-provider result, and for RSS feeds in
/// particular the query string can be a semantically required part of the
/// feed's identity — stripping it would silently point `music_manage_podcasts`
/// `add` at the wrong feed, or one that fails to resolve at all.
fn sanitize_url(value: &str) -> Option<String> {
    let mut url = url::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    url.set_fragment(None);
    Some(url.into())
}

fn locale_from_env() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "C".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_9_youtube_candidates_omit_the_subscriber_count_field_when_absent() {
        let with_count = DiscoveryCandidateDto::Youtube {
            title: "Visible".into(),
            url: "https://www.youtube.com/channel/UC-visible".into(),
            matching_video_count: 3,
            subscriber_count: Some(62_400),
        };
        let without_count = DiscoveryCandidateDto::Youtube {
            title: "Hidden".into(),
            url: "https://www.youtube.com/channel/UC-hidden".into(),
            matching_video_count: 1,
            subscriber_count: None,
        };

        let with_json = serde_json::to_value(&with_count).unwrap();
        let without_json = serde_json::to_value(&without_count).unwrap();

        assert_eq!(with_json["subscriber_count"], 62_400);
        assert!(
            without_json.get("subscriber_count").is_none(),
            "an absent subscriber count must be omitted entirely, never serialized as null or zero: {without_json}"
        );
    }

    #[test]
    fn sanitize_url_strips_credentials_and_fragments_but_keeps_meaningful_query_strings() {
        assert_eq!(
            sanitize_url("https://feeds.test/show?token=abc#section").as_deref(),
            Some("https://feeds.test/show?token=abc")
        );
        assert_eq!(sanitize_url("https://user:pass@feeds.test/show"), None);
        assert_eq!(sanitize_url("file:///etc/passwd"), None);
        assert_eq!(sanitize_url("not a url"), None);
    }

    #[test]
    fn search_sources_rejects_an_unknown_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reprise.db");
        {
            let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
            reprise_core::library::settings::set_bool(
                &conn,
                crate::capability::CAP_SOURCES_MANAGE,
                true,
            )
            .unwrap();
        }
        let params = SearchSourcesParams {
            provider: "spotify".into(),
            query: "test".into(),
        };
        let error = search_sources(&path, true, &params).unwrap_err();
        assert!(
            matches!(error, DataError::InvalidInput(message) if message.contains("unknown provider"))
        );
    }

    #[test]
    fn search_sources_denies_when_the_capability_is_not_granted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reprise.db");
        reprise_core::db::open_migrated(Some(&path)).unwrap();
        let params = SearchSourcesParams {
            provider: "rss".into(),
            query: "test".into(),
        };
        let error = search_sources(&path, true, &params).unwrap_err();
        assert!(matches!(
            error,
            DataError::CapabilityDenied("sources:manage")
        ));
    }
}
