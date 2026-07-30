//! Shared podcast episode batch target planning and dispatch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
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

pub(super) fn dispatch_selected(host: &impl IsA<gtk4::Widget>, action: &str, episode_ids: &[i64]) {
    let _ = host.activate_action(
        &format!("podcasts.{action}"),
        Some(&episode_ids.to_variant()),
    );
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk4::gio;

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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_12_shared_batch_dispatch_reaches_library_and_channel_hosts() {
        gtk4::init().unwrap();
        fn host(seen: &Rc<RefCell<Vec<Vec<i64>>>>) -> gtk4::Box {
            let host = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            let group = gio::SimpleActionGroup::new();
            let action =
                gio::SimpleAction::new("remove-selected", Some(&Vec::<i64>::static_variant_type()));
            let seen = seen.clone();
            action.connect_activate(move |_, target| {
                seen.borrow_mut().push(
                    target
                        .and_then(gtk4::glib::Variant::get::<Vec<i64>>)
                        .expect("batch target"),
                );
            });
            group.add_action(&action);
            host.insert_action_group("podcasts", Some(&group));
            host
        }
        let seen = Rc::new(RefCell::new(Vec::new()));
        let library = host(&seen);
        let channel = host(&seen);

        dispatch_selected(&library, "remove-selected", &[1, 2]);
        dispatch_selected(&channel, "remove-selected", &[7, 8]);

        assert_eq!(*seen.borrow(), [vec![1, 2], vec![7, 8]]);
    }
}
