//! Pure presentation state for episode downloads.

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
}
