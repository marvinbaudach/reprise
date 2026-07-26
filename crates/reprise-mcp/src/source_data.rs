//! Path- and credential-safe cached podcast and radio projections.
//!
//! This adapter reaches source state only through `reprise-core` facades. It
//! intentionally omits feed, media, artwork, homepage, and stream URLs: those
//! may contain private tokens even though they are not filesystem paths.

use std::path::Path;

use reprise_core::podcasts::{EpisodeRow, PodcastKind, SubscriptionRow};
use reprise_core::radio::StationRow;
use serde::Serialize;

use crate::data::{self, DataError};

const MAX_SOURCE_ITEMS: usize = 200;

#[derive(Debug, Serialize)]
pub struct PodcastsResource {
    pub subscriptions: Vec<PodcastSubscriptionDto>,
    pub subscription_total: usize,
    pub episodes: Vec<PodcastEpisodeDto>,
    pub episode_total: usize,
}

#[derive(Debug, Serialize)]
pub struct PodcastSubscriptionDto {
    pub id: i64,
    pub kind: &'static str,
    pub title: String,
    pub author: Option<String>,
    pub last_fetch_at: Option<i64>,
    pub last_outcome: Option<String>,
    pub auto_download: bool,
    pub added_at: i64,
}

#[derive(Debug, Serialize)]
pub struct PodcastEpisodeDto {
    pub id: i64,
    pub subscription_id: i64,
    pub title: String,
    pub show: String,
    pub kind: &'static str,
    pub published_at: Option<i64>,
    pub duration_secs: Option<i64>,
    pub downloaded: bool,
    pub played: bool,
    pub position_ms: i64,
    pub first_seen_at: i64,
}

#[derive(Debug, Serialize)]
pub struct RadioResource {
    pub stations: Vec<RadioStationDto>,
    pub station_total: usize,
}

#[derive(Debug, Serialize)]
pub struct RadioStationDto {
    pub id: i64,
    pub uuid: Option<String>,
    pub name: String,
    pub genre: Option<String>,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub country_code: Option<String>,
    pub votes: Option<i64>,
    pub added_at: i64,
}

pub fn podcasts(path: &Path) -> Result<PodcastsResource, DataError> {
    let conn = data::open(path)?;
    data::require_read(&conn)?;
    let subscriptions =
        reprise_core::podcasts::store::active_subscriptions(&conn).map_err(DataError::Db)?;
    let episodes = reprise_core::podcasts::query::list_episodes(&conn).map_err(DataError::Db)?;
    let subscription_total = subscriptions.len();
    let episode_total = episodes.len();

    Ok(PodcastsResource {
        subscriptions: subscriptions
            .iter()
            .take(MAX_SOURCE_ITEMS)
            .map(PodcastSubscriptionDto::from)
            .collect(),
        subscription_total,
        episodes: episodes
            .iter()
            .take(MAX_SOURCE_ITEMS)
            .map(PodcastEpisodeDto::from)
            .collect(),
        episode_total,
    })
}

pub fn radio(path: &Path) -> Result<RadioResource, DataError> {
    let conn = data::open(path)?;
    data::require_read(&conn)?;
    let stations = reprise_core::radio::station::list(&conn).map_err(DataError::Db)?;
    let station_total = stations.len();
    Ok(RadioResource {
        stations: stations
            .iter()
            .take(MAX_SOURCE_ITEMS)
            .map(RadioStationDto::from)
            .collect(),
        station_total,
    })
}

impl From<&SubscriptionRow> for PodcastSubscriptionDto {
    fn from(row: &SubscriptionRow) -> Self {
        Self {
            id: row.id,
            kind: podcast_kind(row.kind),
            title: row.title.clone(),
            author: row.author.clone(),
            last_fetch_at: row.last_fetch_at,
            last_outcome: row.last_outcome.clone(),
            auto_download: row.auto_download,
            added_at: row.added_at,
        }
    }
}

impl From<&EpisodeRow> for PodcastEpisodeDto {
    fn from(row: &EpisodeRow) -> Self {
        Self {
            id: row.id,
            subscription_id: row.subscription_id,
            title: row.title.clone(),
            show: row.show.clone(),
            kind: podcast_kind(row.kind),
            published_at: row.published_at,
            duration_secs: row.duration_secs,
            downloaded: row.downloaded_path.is_some(),
            played: row.played_at.is_some(),
            position_ms: row.position_ms,
            first_seen_at: row.first_seen_at,
        }
    }
}

impl From<&StationRow> for RadioStationDto {
    fn from(row: &StationRow) -> Self {
        Self {
            id: row.id,
            uuid: row.uuid.clone(),
            name: row.name.clone(),
            genre: row.genre.clone(),
            codec: row.codec.clone(),
            bitrate_kbps: row.bitrate_kbps,
            country_code: row.country_code.clone(),
            votes: row.votes,
            added_at: row.added_at,
        }
    }
}

fn podcast_kind(kind: PodcastKind) -> &'static str {
    match kind {
        PodcastKind::Rss => "rss",
        PodcastKind::Youtube => "youtube",
    }
}
