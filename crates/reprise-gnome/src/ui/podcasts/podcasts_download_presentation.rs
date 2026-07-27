//! Pure projection of persisted and transient episode download states.

use std::collections::BTreeMap;

use reprise_core::podcasts::download_state::{self, DownloadState};
use reprise_core::podcasts::EpisodeRow;

pub(super) fn refreshed_download_states(
    rows: &[EpisodeRow],
    previous: &BTreeMap<i64, DownloadState>,
) -> BTreeMap<i64, DownloadState> {
    rows.iter()
        .map(|row| {
            let state = match previous.get(&row.id) {
                Some(
                    state @ (DownloadState::Queued
                    | DownloadState::Downloading { .. }
                    | DownloadState::Failed { .. }),
                ) => state.clone(),
                _ => {
                    let metadata = row
                        .downloaded_path
                        .as_deref()
                        .and_then(|path| std::fs::metadata(path).ok())
                        .filter(std::fs::Metadata::is_file);
                    let bytes = row.downloaded_bytes.or_else(|| {
                        metadata
                            .as_ref()
                            .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
                    });
                    download_state::from_persisted(
                        row.downloaded_path.as_deref(),
                        bytes,
                        metadata.is_some(),
                    )
                }
            };
            (row.id, state)
        })
        .collect()
}
