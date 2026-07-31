//! Shared podcast episode batch target planning and dispatch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reprise_core::podcasts::download_state::DownloadState;

/// `#[must_use]`: a dropped `BatchResult` is a batch whose partial failures
/// were never reported to anyone. That exact bug shipped once here — the undo
/// path discarded its result — so the compiler carries the rule now.
#[must_use]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct BatchResult {
    pub(super) requested: usize,
    pub(super) succeeded_ids: Vec<i64>,
    pub(super) failed: usize,
}

impl BatchResult {
    pub(super) fn succeeded(&self) -> usize {
        self.succeeded_ids.len()
    }
}

pub(super) fn run_batch(
    episode_ids: &[i64],
    mut operation: impl FnMut(i64) -> bool,
) -> BatchResult {
    let succeeded_ids = episode_ids
        .iter()
        .copied()
        .filter(|episode_id| operation(*episode_id))
        .collect::<Vec<_>>();
    BatchResult {
        requested: episode_ids.len(),
        failed: episode_ids.len().saturating_sub(succeeded_ids.len()),
        succeeded_ids,
    }
}

pub(super) fn trash_downloads(
    downloads: &[(i64, PathBuf)],
    mut trash: impl FnMut(&Path) -> bool,
) -> BatchResult {
    let succeeded_ids = downloads
        .iter()
        .filter_map(|(episode_id, path)| trash(path).then_some(*episode_id))
        .collect::<Vec<_>>();
    BatchResult {
        requested: downloads.len(),
        failed: downloads.len().saturating_sub(succeeded_ids.len()),
        succeeded_ids,
    }
}

pub(super) fn undo_batch(episode_ids: &[i64], undo: impl FnMut(i64) -> bool) -> BatchResult {
    run_batch(episode_ids, undo)
}

pub(super) fn downloadable_ids(
    selected_ids: &[i64],
    states: &BTreeMap<i64, DownloadState>,
) -> Vec<i64> {
    selected_ids
        .iter()
        .copied()
        .filter(|episode_id| {
            !matches!(
                states.get(episode_id),
                Some(
                    DownloadState::Queued
                        | DownloadState::Downloading { .. }
                        | DownloadState::Downloaded { .. }
                )
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn batch_download_skips_queued_active_and_downloaded_episodes() {
        let states = BTreeMap::from([
            (2, DownloadState::Queued),
            (
                3,
                DownloadState::Downloading {
                    received_bytes: 10,
                    total_bytes: Some(100),
                },
            ),
            (4, DownloadState::Downloaded { bytes: 100 }),
            (
                5,
                DownloadState::Failed {
                    message: "provider unavailable".into(),
                },
            ),
        ]);

        assert_eq!(downloadable_ids(&[1, 2, 3, 4, 5], &states), [1, 5]);
    }

    #[test]
    fn partial_batch_failure_reports_the_true_counts_and_successful_targets() {
        let result = run_batch(&[1, 2, 3, 4, 5, 6, 7], |episode_id| episode_id <= 4);

        assert_eq!(result.requested, 7);
        assert_eq!(result.succeeded_ids, [1, 2, 3, 4]);
        assert_eq!(result.succeeded(), 4);
        assert_eq!(result.failed, 3);
    }

    #[test]
    fn bulk_delete_routes_every_download_to_trash() {
        let downloads = [
            (1, PathBuf::from("/downloads/one.mp3")),
            (2, PathBuf::from("/downloads/two.opus")),
        ];
        let mut trashed = Vec::new();

        let result = trash_downloads(&downloads, |path| {
            trashed.push(path.to_path_buf());
            true
        });

        assert_eq!(
            trashed,
            ["/downloads/one.mp3", "/downloads/two.opus"].map(PathBuf::from)
        );
        assert_eq!(result.succeeded_ids, [1, 2]);
        assert_eq!(result.failed, 0);
    }

    #[test]
    fn one_undo_reverts_the_whole_successful_batch() {
        let mut restored = Vec::new();

        let result = undo_batch(&[11, 12, 21], |episode_id| {
            restored.push(episode_id);
            true
        });

        assert_eq!(restored, [11, 12, 21]);
        assert_eq!(result.succeeded_ids, [11, 12, 21]);
        assert_eq!(result.failed, 0);
    }
}
