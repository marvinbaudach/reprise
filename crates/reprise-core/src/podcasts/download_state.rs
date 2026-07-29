//! Pure presentation state for episode downloads.

use crate::connectivity::LocalAvailability;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadState {
    NotDownloaded,
    Queued,
    Downloading {
        received_bytes: u64,
        total_bytes: Option<u64>,
    },
    Downloaded {
        bytes: u64,
    },
    Missing,
    Failed {
        message: String,
    },
}

#[must_use]
pub fn downloading(
    previous: &DownloadState,
    received_bytes: u64,
    total_bytes: Option<u64>,
) -> DownloadState {
    let (previous_received, previous_total) = match previous {
        DownloadState::Downloading {
            received_bytes,
            total_bytes,
        } => (*received_bytes, *total_bytes),
        _ => (0, None),
    };
    DownloadState::Downloading {
        received_bytes: received_bytes.max(previous_received),
        total_bytes: total_bytes.or(previous_total),
    }
}

/// The size of the downloaded file that is on disk right now, or `None`.
///
/// `None` covers "no path recorded", "the path is gone" and "the path is not a
/// file" alike, because all three mean the same thing to every caller: there
/// is nothing to play. Pairing that with [`from_persisted`]'s `file_exists`
/// argument is the whole reason this lives here — the frontend used to reach
/// for `std::fs::metadata` itself to answer a question about a podcast.
#[must_use]
pub fn on_disk_bytes(downloaded_path: Option<&str>) -> Option<i64> {
    let metadata = std::fs::metadata(downloaded_path?).ok()?;
    metadata
        .is_file()
        .then(|| metadata.len().min(i64::MAX as u64) as i64)
}

#[must_use]
pub fn from_persisted(
    downloaded_path: Option<&str>,
    downloaded_bytes: Option<i64>,
    file_exists: bool,
) -> DownloadState {
    match (downloaded_path, file_exists) {
        (None, _) => DownloadState::NotDownloaded,
        (Some(_), false) => DownloadState::Missing,
        (Some(_), true) => DownloadState::Downloaded {
            bytes: downloaded_bytes.unwrap_or_default().max(0) as u64,
        },
    }
}

impl DownloadState {
    /// `NET-3a`'s bridge from this module's richer download lifecycle into
    /// the offline projection's simpler local-availability signal. Only a
    /// file that is actually present locally lets a row or an action skip
    /// the network entirely — every other state (not downloaded, queued,
    /// downloading, a path recorded but the file gone, or a failed
    /// attempt) still needs the network to become playable/transferable.
    #[must_use]
    pub const fn local_availability(&self) -> LocalAvailability {
        match self {
            DownloadState::Downloaded { .. } => LocalAvailability::Available,
            DownloadState::NotDownloaded
            | DownloadState::Queued
            | DownloadState::Downloading { .. }
            | DownloadState::Missing
            | DownloadState::Failed { .. } => LocalAvailability::Missing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_7_download_progress_is_monotone_and_allows_an_unknown_total() {
        let queued = DownloadState::Queued;
        let first = downloading(&queued, 120, None);
        assert_eq!(
            first,
            DownloadState::Downloading {
                received_bytes: 120,
                total_bytes: None,
            }
        );

        let regressed = downloading(&first, 80, Some(1_000));
        assert_eq!(
            regressed,
            DownloadState::Downloading {
                received_bytes: 120,
                total_bytes: Some(1_000),
            }
        );

        let missing_total = downloading(&regressed, 400, None);
        assert_eq!(
            missing_total,
            DownloadState::Downloading {
                received_bytes: 400,
                total_bytes: Some(1_000),
            }
        );
    }

    #[test]
    fn pod_7_persisted_downloads_distinguish_local_missing_and_not_downloaded() {
        assert_eq!(
            from_persisted(None, None, false),
            DownloadState::NotDownloaded
        );
        assert_eq!(
            from_persisted(Some("/podcasts/show/episode.mp3"), Some(42), true),
            DownloadState::Downloaded { bytes: 42 }
        );
        assert_eq!(
            from_persisted(Some("/podcasts/show/episode.mp3"), Some(42), false),
            DownloadState::Missing
        );
    }

    #[test]
    fn net_3a_local_availability_is_available_only_for_a_downloaded_file() {
        assert_eq!(
            DownloadState::Downloaded { bytes: 42 }.local_availability(),
            LocalAvailability::Available
        );
        for missing in [
            DownloadState::NotDownloaded,
            DownloadState::Queued,
            DownloadState::Downloading {
                received_bytes: 10,
                total_bytes: None,
            },
            DownloadState::Missing,
            DownloadState::Failed {
                message: "offline".into(),
            },
        ] {
            assert_eq!(
                missing.local_availability(),
                LocalAvailability::Missing,
                "{missing:?} still needs the network"
            );
        }
    }
}
