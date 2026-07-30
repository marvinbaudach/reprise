//! Synchronous, capability-gated podcast and radio source mutations.

use std::path::Path;

use rmcp::schemars;
use serde::{Deserialize, Serialize};

use crate::data::{self, DataError};

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ManagePodcastsParams {
    /// One of: `add`, `edit`, `remove`, `refresh`.
    pub action: String,
    /// Subscription id for `edit` and `remove`.
    #[serde(default)]
    pub subscription_id: Option<i64>,
    /// RSS feed or YouTube channel/playlist URL for `add`.
    #[serde(default)]
    pub url: Option<String>,
    /// Optional display-title replacement for `edit`.
    #[serde(default)]
    pub title: Option<String>,
    /// Optional per-subscription auto-download setting for `add` or `edit`.
    #[serde(default)]
    pub auto_download: Option<bool>,
    /// For `add`, import currently listed episodes (default true).
    #[serde(default = "default_true")]
    pub import_existing: bool,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ManageRadioParams {
    /// One of: `add`, `edit`, `remove`.
    pub action: String,
    /// Favorite id for `edit` and `remove`.
    #[serde(default)]
    pub station_id: Option<i64>,
    /// HTTP(S) stream URL for `add`, or a replacement for `edit`.
    #[serde(default)]
    pub url: Option<String>,
    /// Station name for `add`, or a replacement for `edit`.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub bitrate_kbps: Option<i64>,
    #[serde(default)]
    pub country_code: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub votes: Option<i64>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct ManageSourceResult {
    pub action: &'static str,
    pub id: i64,
    pub kind: &'static str,
    pub title: String,
    pub episodes_affected: usize,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ManagePodcastsResult {
    Source(ManageSourceResult),
    Refresh(PodcastRefreshResult),
}

#[derive(Debug, Serialize)]
pub struct PodcastRefreshResult {
    pub action: &'static str,
    pub attempted: usize,
    pub refreshed: usize,
    pub not_modified: usize,
    pub failed: usize,
    pub episodes_inserted: usize,
    pub episodes_updated: usize,
    pub downloads_completed: usize,
    pub downloads_failed: usize,
}

impl ManagePodcastsResult {
    pub fn summary(&self) -> String {
        match self {
            Self::Source(result) => format!(
                "{} podcast source '{}' (id {})",
                result.action, result.title, result.id
            ),
            Self::Refresh(result) => format!(
                "Refreshed {} of {} podcast source(s); {} failed",
                result.refreshed, result.attempted, result.failed
            ),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ManageRadioResult {
    pub action: &'static str,
    pub id: i64,
    pub name: String,
    pub genre: Option<String>,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub country_code: Option<String>,
}

pub fn manage_podcasts(
    path: &Path,
    granted_at_startup: bool,
    params: &ManagePodcastsParams,
) -> Result<ManagePodcastsResult, DataError> {
    let db = data::open(path)?;
    let allowed = crate::capability::sources_manage_effective(&db, granted_at_startup)
        .map_err(DataError::Db)?;
    if !allowed {
        return Err(DataError::CapabilityDenied("sources:manage"));
    }
    match params.action.as_str() {
        "add" => add_podcast(&db, params).map(ManagePodcastsResult::Source),
        "edit" => edit_podcast(&db, params).map(ManagePodcastsResult::Source),
        "remove" => remove_podcast(&db, params).map(ManagePodcastsResult::Source),
        "refresh" => refresh_podcasts(path, &db).map(ManagePodcastsResult::Refresh),
        other => Err(DataError::InvalidInput(format!(
            "unknown podcast action '{other}'"
        ))),
    }
}

pub fn manage_radio(
    path: &Path,
    granted_at_startup: bool,
    params: &ManageRadioParams,
) -> Result<ManageRadioResult, DataError> {
    let db = data::open(path)?;
    require_manage(&db, granted_at_startup)?;
    match params.action.as_str() {
        "add" => add_radio(&db, params),
        "edit" => edit_radio(&db, params),
        "remove" => remove_radio(&db, params),
        other => Err(DataError::InvalidInput(format!(
            "unknown radio action '{other}'"
        ))),
    }
}

fn add_radio(
    db: &reprise_core::db::Db,
    params: &ManageRadioParams,
) -> Result<ManageRadioResult, DataError> {
    let url = required_http_url(params.url.as_deref(), "url is required for add")?;
    let stream_url = resolve_radio_url(url)?;
    let supplied_name = normalized(params.name.as_deref());
    let probe = if supplied_name.is_none() {
        Some(
            reprise_core::radio::icy::probe(&stream_url)
                .map_err(|error| radio_source_error(&error))?,
        )
    } else {
        None
    };
    let name = supplied_name
        .or_else(|| probe.as_ref().and_then(|value| value.name.clone()))
        .ok_or_else(|| {
            DataError::InvalidInput(
                "radio stream returned no station name; provide name explicitly".to_owned(),
            )
        })?;
    let station = reprise_core::radio::station::NewStation {
        uuid: normalized(params.uuid.as_deref()),
        name,
        stream_url,
        homepage: None,
        favicon_url: None,
        genre: normalized(params.genre.as_deref())
            .or_else(|| probe.as_ref().and_then(|value| value.genre.clone())),
        codec: normalized(params.codec.as_deref()).or_else(|| {
            probe
                .as_ref()
                .and_then(|value| value.content_type.as_deref())
                .and_then(codec_from_content_type)
        }),
        bitrate_kbps: params
            .bitrate_kbps
            .filter(|value| *value > 0)
            .or_else(|| probe.as_ref().and_then(|value| value.bitrate_kbps)),
        country_code: normalized_country(params.country_code.as_deref()),
        votes: params.votes.map(|value| value.max(0)),
    };
    let id = reprise_core::radio::station::add_or_restore(db, &station, now_secs())
        .map_err(DataError::Db)?;
    Ok(radio_result("add", id, station))
}

fn edit_radio(
    db: &reprise_core::db::Db,
    params: &ManageRadioParams,
) -> Result<ManageRadioResult, DataError> {
    let id = required_id(params.station_id, "station_id is required for edit")?;
    if params.name.is_none()
        && params.url.is_none()
        && params.genre.is_none()
        && params.codec.is_none()
        && params.bitrate_kbps.is_none()
        && params.country_code.is_none()
        && params.votes.is_none()
    {
        return Err(DataError::InvalidInput(
            "edit requires at least one changed field".to_owned(),
        ));
    }
    let current = active_station(db, id)?;
    let name = match params.name.as_deref() {
        Some(value) => required_nonempty(Some(value), "name must not be empty")?.to_owned(),
        None => current.name,
    };
    let stream_url = match params.url.as_deref() {
        Some(value) => {
            let url = required_http_url(Some(value), "url must be HTTP or HTTPS")?;
            resolve_radio_url(url)?
        }
        None => current.stream_url,
    };
    let station = reprise_core::radio::station::NewStation {
        uuid: current.uuid,
        name,
        stream_url,
        homepage: current.homepage,
        favicon_url: current.favicon_url,
        genre: params
            .genre
            .as_deref()
            .map(|value| normalized(Some(value)))
            .unwrap_or(current.genre),
        codec: params
            .codec
            .as_deref()
            .map(|value| normalized(Some(value)))
            .unwrap_or(current.codec),
        bitrate_kbps: params
            .bitrate_kbps
            .map_or(current.bitrate_kbps, |value| (value > 0).then_some(value)),
        country_code: params
            .country_code
            .as_deref()
            .map(|value| normalized_country(Some(value)))
            .unwrap_or(current.country_code),
        votes: params
            .votes
            .map_or(current.votes, |value| Some(value.max(0))),
    };
    let updated = reprise_core::radio::station::update(db, id, &station).map_err(DataError::Db)?;
    if !updated {
        return Err(DataError::InvalidInput(
            "radio station does not exist".to_owned(),
        ));
    }
    Ok(radio_result("edit", id, station))
}

fn remove_radio(
    db: &reprise_core::db::Db,
    params: &ManageRadioParams,
) -> Result<ManageRadioResult, DataError> {
    let id = required_id(params.station_id, "station_id is required for remove")?;
    let current = active_station(db, id)?;
    reprise_core::radio::station::tombstone(db, id, now_secs()).map_err(DataError::Db)?;
    reprise_core::radio::station::commit_remove(db, id).map_err(DataError::Db)?;
    Ok(radio_result(
        "remove",
        id,
        reprise_core::radio::station::NewStation {
            uuid: current.uuid,
            name: current.name,
            stream_url: current.stream_url,
            homepage: current.homepage,
            favicon_url: current.favicon_url,
            genre: current.genre,
            codec: current.codec,
            bitrate_kbps: current.bitrate_kbps,
            country_code: current.country_code,
            votes: current.votes,
        },
    ))
}

fn refresh_podcasts(
    db_path: &Path,
    db: &reprise_core::db::Db,
) -> Result<PodcastRefreshResult, DataError> {
    let config = reprise_core::podcasts::config::load(db).map_err(DataError::Db)?;
    let ytdlp = reprise_core::podcasts::ytdlp::YtDlp::discover(config.ytdlp_path.as_deref());
    let download_root = db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("podcasts");
    let summary = reprise_core::podcasts::pipeline::refresh_to_root(
        db,
        &reprise_core::podcasts::pipeline::HttpFeedFetcher,
        &ytdlp,
        now_secs(),
        true,
        &download_root,
    )
    .map_err(|error| {
        tracing::error!(%error, "podcast refresh failed");
        DataError::InvalidInput("podcast refresh failed".to_owned())
    })?;
    Ok(PodcastRefreshResult {
        action: "refresh",
        attempted: summary.attempted,
        refreshed: summary.refreshed,
        not_modified: summary.not_modified,
        failed: summary.failed,
        episodes_inserted: summary.episodes_inserted,
        episodes_updated: summary.episodes_updated,
        downloads_completed: summary.downloads_completed,
        downloads_failed: summary.downloads_failed,
    })
}

fn edit_podcast(
    db: &reprise_core::db::Db,
    params: &ManagePodcastsParams,
) -> Result<ManageSourceResult, DataError> {
    let id = required_id(
        params.subscription_id,
        "subscription_id is required for edit",
    )?;
    if params.title.is_none() && params.auto_download.is_none() {
        return Err(DataError::InvalidInput(
            "edit requires title and/or auto_download".to_owned(),
        ));
    }
    let title = params
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty());
    if params.title.is_some() && title.is_none() {
        return Err(DataError::InvalidInput(
            "title must not be empty".to_owned(),
        ));
    }
    let current = active_subscription(db, id)?;
    let updated = reprise_core::podcasts::store::update_subscription_details(
        db,
        id,
        title,
        params.auto_download,
    )
    .map_err(DataError::Db)?;
    if !updated {
        return Err(DataError::InvalidInput(
            "podcast subscription does not exist".to_owned(),
        ));
    }
    Ok(ManageSourceResult {
        action: "edit",
        id,
        kind: podcast_kind(current.kind),
        title: title.unwrap_or(&current.title).to_owned(),
        episodes_affected: 0,
    })
}

fn remove_podcast(
    db: &reprise_core::db::Db,
    params: &ManagePodcastsParams,
) -> Result<ManageSourceResult, DataError> {
    let id = required_id(
        params.subscription_id,
        "subscription_id is required for remove",
    )?;
    let current = active_subscription(db, id)?;
    let episode_count = reprise_core::podcasts::query::episodes_for_subscription(db, id)
        .map_err(DataError::Db)?
        .len();
    reprise_core::podcasts::store::tombstone_subscription(db, id, now_secs())
        .map_err(DataError::Db)?;
    reprise_core::podcasts::store::commit_remove_subscription(db, id).map_err(DataError::Db)?;
    Ok(ManageSourceResult {
        action: "remove",
        id,
        kind: podcast_kind(current.kind),
        title: current.title,
        episodes_affected: episode_count,
    })
}

fn add_podcast(
    db: &reprise_core::db::Db,
    params: &ManagePodcastsParams,
) -> Result<ManageSourceResult, DataError> {
    use reprise_core::podcasts;

    let url = required_nonempty(params.url.as_deref(), "url is required for add")?;
    let kind = match podcasts::url_detect::detect(url) {
        podcasts::url_detect::InputKind::ProbableFeedUrl => podcasts::PodcastKind::Rss,
        podcasts::url_detect::InputKind::YoutubeUrl => podcasts::PodcastKind::Youtube,
        podcasts::url_detect::InputKind::Search => {
            return Err(DataError::InvalidInput(
                "url must be an HTTP RSS feed or YouTube channel/playlist URL".to_owned(),
            ));
        }
    };
    let config = podcasts::config::load(db).map_err(DataError::Db)?;
    let (feed, response) = match kind {
        podcasts::PodcastKind::Rss => {
            let response =
                podcasts::http::get(url).map_err(|error| podcast_source_error(&error))?;
            let feed = podcasts::feed::parse_feed(&response.body, config.import_count)
                .map_err(|error| podcast_source_error(&error))?;
            (feed, Some(response))
        }
        podcasts::PodcastKind::Youtube
            if reprise_core::online_sources::network_allowed(
                db,
                &reprise_core::modules::YOUTUBE_MODULE,
            )
            .unwrap_or(false) =>
        {
            let listing = podcasts::ytdlp::YtDlp::discover(config.ytdlp_path.as_deref())
                .list(url)
                .map(podcasts::youtube::project_playlist)
                .map_err(|error| podcast_source_error(&error))?;
            (
                podcasts::pipeline::project_youtube_feed(listing, config.import_count),
                None,
            )
        }
        podcasts::PodcastKind::Youtube => {
            return Err(DataError::InvalidInput(
                "YouTube sources are disabled in Reprise preferences".to_owned(),
            ));
        }
    };
    let baseline = (!params.import_existing).then(|| {
        feed.episodes
            .iter()
            .map(|episode| episode.guid.clone())
            .collect::<Vec<_>>()
    });
    let auto_download = params.auto_download.unwrap_or(config.auto_download_default);
    let now = now_secs();
    let id = podcasts::store::add_or_restore_with_baseline(
        db,
        &podcasts::store::NewSubscription {
            kind,
            feed_url: url.to_owned(),
            title: feed.title.clone(),
            author: feed.author.clone(),
            image_url: feed.image_url.clone(),
            auto_download,
        },
        now,
        baseline.as_deref(),
    )
    .map_err(DataError::Db)?;
    let mut episodes_affected = 0;
    if params.import_existing {
        for episode in &feed.episodes {
            if podcasts::store::upsert_episode(db, id, episode, now)
                .map_err(DataError::Db)?
                .is_some()
            {
                episodes_affected += 1;
            }
        }
    }
    podcasts::store::update_fetch_success(
        db,
        id,
        now,
        podcasts::store::FetchSuccess {
            etag: response.as_ref().and_then(|value| value.etag.as_deref()),
            last_modified: response
                .as_ref()
                .and_then(|value| value.last_modified.as_deref()),
            title: Some(&feed.title),
            author: feed.author.as_deref(),
            image_url: feed.image_url.as_deref(),
        },
    )
    .map_err(DataError::Db)?;

    Ok(ManageSourceResult {
        action: "add",
        id,
        kind: match kind {
            podcasts::PodcastKind::Rss => "rss",
            podcasts::PodcastKind::Youtube => "youtube",
        },
        title: feed.title,
        episodes_affected,
    })
}

pub(crate) fn required_nonempty<'a>(
    value: Option<&'a str>,
    message: &str,
) -> Result<&'a str, DataError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DataError::InvalidInput(message.to_owned()))
}

fn required_http_url<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str, DataError> {
    let value = required_nonempty(value, message)?;
    url::Url::parse(value)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|_| value)
        .ok_or_else(|| DataError::InvalidInput("url must be HTTP or HTTPS".to_owned()))
}

fn resolve_radio_url(url: &str) -> Result<String, DataError> {
    let parsed = url::Url::parse(url)
        .map_err(|_| DataError::InvalidInput("url must be HTTP or HTTPS".to_owned()))?;
    let path = parsed.path().to_ascii_lowercase();
    let kind = if path.ends_with(".pls") {
        Some(reprise_core::radio::playlist::PlaylistKind::Pls)
    } else if path.ends_with(".m3u") || path.ends_with(".m3u8") {
        Some(reprise_core::radio::playlist::PlaylistKind::M3u)
    } else {
        None
    };
    let Some(kind) = kind else {
        return Ok(url.to_owned());
    };
    let body = reprise_core::radio::http::get(url).map_err(|error| radio_source_error(&error))?;
    if reprise_core::radio::playlist::is_hls_manifest(&body) {
        return Ok(url.to_owned());
    }
    reprise_core::radio::playlist::resolve_playlist(&body, kind).ok_or_else(|| {
        DataError::InvalidInput(
            "radio playlist did not contain a playable HTTP(S) stream".to_owned(),
        )
    })
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn normalized_country(value: Option<&str>) -> Option<String> {
    normalized(value).map(|value| value.to_ascii_uppercase())
}

fn required_id(value: Option<i64>, message: &str) -> Result<i64, DataError> {
    value
        .filter(|id| *id > 0)
        .ok_or_else(|| DataError::InvalidInput(message.to_owned()))
}

fn active_subscription(
    db: &reprise_core::db::Db,
    id: i64,
) -> Result<reprise_core::podcasts::SubscriptionRow, DataError> {
    reprise_core::podcasts::store::subscription(db, id)
        .map_err(DataError::Db)?
        .filter(|row| row.removed_at.is_none())
        .ok_or_else(|| DataError::InvalidInput("podcast subscription does not exist".to_owned()))
}

fn active_station(
    db: &reprise_core::db::Db,
    id: i64,
) -> Result<reprise_core::radio::StationRow, DataError> {
    reprise_core::radio::station::get(db, id)
        .map_err(DataError::Db)?
        .ok_or_else(|| DataError::InvalidInput("radio station does not exist".to_owned()))
}

fn podcast_kind(kind: reprise_core::podcasts::PodcastKind) -> &'static str {
    match kind {
        reprise_core::podcasts::PodcastKind::Rss => "rss",
        reprise_core::podcasts::PodcastKind::Youtube => "youtube",
    }
}

fn radio_result(
    action: &'static str,
    id: i64,
    station: reprise_core::radio::station::NewStation,
) -> ManageRadioResult {
    ManageRadioResult {
        action,
        id,
        name: station.name,
        genre: station.genre,
        codec: station.codec,
        bitrate_kbps: station.bitrate_kbps,
        country_code: station.country_code,
    }
}

fn require_manage(db: &reprise_core::db::Db, granted_at_startup: bool) -> Result<(), DataError> {
    let allowed = crate::capability::sources_manage_effective(db, granted_at_startup)
        .map_err(DataError::Db)?;
    if allowed {
        Ok(())
    } else {
        Err(DataError::CapabilityDenied("sources:manage"))
    }
}

/// `POD-13`: delegates to `PodcastError::classify` — the single classifier
/// also used by `pipeline::download_episode` for a failed download — rather
/// than keeping its own copy of the same match that could drift from it.
pub(crate) fn podcast_source_error(error: &reprise_core::podcasts::PodcastError) -> DataError {
    DataError::InvalidInput(error.classify().to_owned())
}

pub(crate) fn radio_source_error(error: &reprise_core::radio::RadioError) -> DataError {
    use reprise_core::radio::RadioError;
    let message = match error {
        RadioError::Timeout => "radio stream timed out",
        RadioError::Transport(_) => "radio stream could not be reached",
        RadioError::HttpStatus(_) => "radio stream returned an HTTP error",
        RadioError::Body(_) | RadioError::Parse(_) => "radio stream returned invalid data",
        RadioError::Unavailable(_) => "radio stream is unavailable",
    };
    DataError::InvalidInput(message.to_owned())
}

fn codec_from_content_type(content_type: &str) -> Option<String> {
    match content_type.to_ascii_lowercase().as_str() {
        "audio/mpeg" | "audio/mp3" => Some("MP3".to_owned()),
        "audio/aac" | "audio/aacp" => Some("AAC".to_owned()),
        "audio/ogg" | "application/ogg" => Some("OGG".to_owned()),
        _ => None,
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}
