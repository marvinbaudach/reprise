//! Shared podcast episode batch target planning and dispatch.

use std::collections::BTreeMap;

use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::podcasts::download_state::DownloadState;

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

pub(super) fn dispatch_each(host: &impl IsA<gtk4::Widget>, action: &str, episode_ids: &[i64]) {
    for episode_id in episode_ids {
        let _ = host.activate_action(
            &format!("podcasts.{action}"),
            Some(&episode_id.to_variant()),
        );
    }
}

pub(super) fn dispatch_downloads(
    host: &impl IsA<gtk4::Widget>,
    selected_ids: &[i64],
    states: &BTreeMap<i64, DownloadState>,
    action: &str,
) {
    dispatch_each(host, action, &downloadable_ids(selected_ids, states));
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
}
