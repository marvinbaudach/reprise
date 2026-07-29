//! Path- and credential-safe cached podcast and radio projections.
//!
//! This adapter reaches source state only through `reprise-core` facades. It
//! intentionally omits feed, media, artwork, homepage, and stream URLs: those
//! may contain private tokens even though they are not filesystem paths.

use std::path::Path;

use reprise_core::connectivity::LocalAvailability;
use reprise_core::podcasts::download_state::{self, DownloadState};
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
    /// `POD-7`/`POD-11`: the episode's download state and, once actually
    /// present on disk, its real file size — never a filesystem path.
    pub download_state: DownloadStateDto,
    /// Block H (`NET-3`/`NET-3a`): whether this episode is already playable
    /// and transferable to a device without touching the network at all.
    /// `available` means every local action (play, phone sync) runs now,
    /// online or not; `missing` means it needs the network at some point —
    /// Reprise has no real connectivity signal today (see
    /// `reprise_core::connectivity`'s module docs), so this is the one
    /// honest, verifiable half of `NET-3` an agent can act on before
    /// starting a download or a sync that might only get queued.
    pub local_availability: &'static str,
}

#[derive(Debug, Serialize)]
pub struct DownloadStateDto {
    /// One of: `not_downloaded`, `downloaded`, `missing`. Only what is
    /// derivable from persisted state plus a real filesystem check — the
    /// transient `queued`/`downloading`/`failed` states only exist in the
    /// running app's in-memory session and are not represented here.
    pub state: &'static str,
    /// Present only for `downloaded`; the real size of the file on disk,
    /// never invented for an episode that is not there.
    pub bytes: Option<u64>,
}

pub(crate) fn download_state_dto(row: &EpisodeRow) -> DownloadStateDto {
    let (state_name, bytes) = match download_state_for(row) {
        DownloadState::Downloaded { bytes } => ("downloaded", Some(bytes)),
        DownloadState::Missing => ("missing", None),
        _ => ("not_downloaded", None),
    };
    DownloadStateDto {
        state: state_name,
        bytes,
    }
}

/// Shared with `channel_detail` (Block H-D): the real, filesystem-checked
/// `DownloadState` for one episode, reused rather than re-derived so the
/// base podcasts resource and the channel-detail tool never disagree about
/// whether a given episode is actually downloaded.
pub(crate) fn download_state_for(row: &EpisodeRow) -> DownloadState {
    let file_exists = row
        .downloaded_path
        .as_deref()
        .and_then(|path| std::fs::metadata(path).ok())
        .is_some_and(|metadata| metadata.is_file());
    download_state::from_persisted(
        row.downloaded_path.as_deref(),
        row.downloaded_bytes,
        file_exists,
    )
}

pub(crate) fn local_availability_name(row: &EpisodeRow) -> &'static str {
    match download_state_for(row).local_availability() {
        LocalAvailability::Available => "available",
        LocalAvailability::Missing => "missing",
    }
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
            download_state: download_state_dto(row),
            local_availability: local_availability_name(row),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn episode(downloaded_path: Option<&str>, downloaded_bytes: Option<i64>) -> EpisodeRow {
        EpisodeRow {
            id: 1,
            subscription_id: 1,
            guid: "guid-1".into(),
            title: "Episode".into(),
            show: "Show".into(),
            show_image_url: None,
            kind: PodcastKind::Rss,
            audio_url: "https://example.test/e.mp3".into(),
            page_url: None,
            published_at: Some(1),
            duration_secs: Some(600),
            downloaded_path: downloaded_path.map(str::to_owned),
            downloaded_bytes,
            played_at: None,
            position_ms: 0,
            first_seen_at: 1,
        }
    }

    /// Block H (`NET-3`): a real file on disk and a missing one must
    /// produce genuinely different `download_state`/`local_availability`
    /// output — not just a label that happens to say something.
    #[test]
    fn net_3_local_availability_differs_between_a_real_file_and_a_missing_one() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"audio-bytes-12").unwrap();
        let present = episode(Some(file.path().to_str().unwrap()), Some(14));
        let absent = episode(Some("/nonexistent/gone.mp3"), Some(14));
        let never = episode(None, None);

        let present_dto = PodcastEpisodeDto::from(&present);
        let absent_dto = PodcastEpisodeDto::from(&absent);
        let never_dto = PodcastEpisodeDto::from(&never);

        assert_eq!(present_dto.download_state.state, "downloaded");
        assert_eq!(present_dto.download_state.bytes, Some(14));
        assert_eq!(present_dto.local_availability, "available");

        assert_eq!(
            absent_dto.download_state.state, "missing",
            "a persisted path whose file is actually gone must not read as downloaded"
        );
        assert_eq!(absent_dto.download_state.bytes, None);
        assert_eq!(absent_dto.local_availability, "missing");

        assert_eq!(never_dto.download_state.state, "not_downloaded");
        assert_eq!(never_dto.local_availability, "missing");

        assert_ne!(
            present_dto.local_availability, absent_dto.local_availability,
            "the two states must actually read differently"
        );
    }

    #[test]
    fn downloaded_flag_stays_a_pure_db_check_independent_of_the_new_fields() {
        // `downloaded` predates `download_state`/`local_availability` and is
        // pinned by an existing resource test — it must keep meaning "a path
        // is on record", not "the file still exists on disk".
        let absent = episode(Some("/nonexistent/gone.mp3"), Some(14));
        let dto = PodcastEpisodeDto::from(&absent);
        assert!(dto.downloaded);
        assert_eq!(dto.download_state.state, "missing");
    }
}
