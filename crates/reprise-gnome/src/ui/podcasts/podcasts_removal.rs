//! Pure aggregation and decision helpers for reversible podcast removal.

use std::collections::BTreeMap;
use std::path::PathBuf;

use reprise_core::podcasts::download_state::DownloadState;

#[derive(Default)]
pub(super) struct KeptDownloads {
    shows: BTreeMap<i64, Vec<PathBuf>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DownloadCommitAction {
    Keep,
    Trash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DownloadToggleAction {
    Download,
    Trash,
}

pub(super) fn download_commit_action(delete_requested: bool) -> DownloadCommitAction {
    if delete_requested {
        DownloadCommitAction::Trash
    } else {
        DownloadCommitAction::Keep
    }
}

pub(super) fn download_toggle_action(
    downloaded_path: Option<&str>,
    file_exists: bool,
) -> DownloadToggleAction {
    if downloaded_path.is_some() && file_exists {
        DownloadToggleAction::Trash
    } else {
        DownloadToggleAction::Download
    }
}

pub(super) fn download_request_allowed(state: Option<&DownloadState>) -> bool {
    !matches!(
        state,
        Some(DownloadState::Queued | DownloadState::Downloading { .. })
    )
}

impl KeptDownloads {
    pub(super) fn add(&mut self, subscription_id: i64, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        self.shows
            .entry(subscription_id)
            .or_default()
            .extend(paths.into_iter().map(PathBuf::from));
    }

    pub(super) fn take(&mut self) -> (usize, Vec<PathBuf>) {
        let shows = self.shows.len();
        let paths = std::mem::take(&mut self.shows)
            .into_values()
            .flatten()
            .collect();
        (shows, paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsubscribe_aggregation_skips_empty_download_sets_and_coalesces_shows() {
        let mut aggregate = KeptDownloads::default();
        aggregate.add(1, Vec::new());
        aggregate.add(2, vec!["a.mp3".into(), "b.mp3".into()]);
        aggregate.add(3, vec!["c.mp3".into()]);
        let (shows, paths) = aggregate.take();
        assert_eq!(shows, 2);
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn src_4_unsubscribe_commit_toast_trashes_never_hard_deletes() {
        assert_eq!(download_commit_action(false), DownloadCommitAction::Keep);
        assert_eq!(download_commit_action(true), DownloadCommitAction::Trash);
    }

    #[test]
    fn pod_7_missing_downloads_are_retried_instead_of_trashing_a_missing_file() {
        assert_eq!(
            download_toggle_action(Some("/missing/episode.mp3"), false),
            DownloadToggleAction::Download
        );
        assert_eq!(
            download_toggle_action(Some("/local/episode.mp3"), true),
            DownloadToggleAction::Trash
        );
        assert_eq!(
            download_toggle_action(None, false),
            DownloadToggleAction::Download
        );
    }

    #[test]
    fn pod_7_queued_or_running_downloads_cannot_be_requested_twice() {
        assert!(!download_request_allowed(Some(&DownloadState::Queued)));
        assert!(!download_request_allowed(Some(
            &DownloadState::Downloading {
                received_bytes: 10,
                total_bytes: None,
            }
        )));
        assert!(download_request_allowed(Some(
            &DownloadState::NotDownloaded
        )));
    }
}
