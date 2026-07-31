use gtk4::prelude::*;
use reprise_core::connectivity::Connectivity;
use reprise_core::podcasts::download_state::DownloadState;

use super::podcasts_download_presentation::{
    download_failure_presentation, episode_network_presentation, localized_download_failure_reason,
    EpisodeNetworkPresentation,
};
use super::podcasts_groups::DownloadRowWidgets;
use crate::ui::strings;

#[derive(Clone, Copy)]
pub(super) struct RowNetworkState {
    pub(super) connectivity: Connectivity,
    pub(super) unavailable_now: bool,
}

pub(super) fn update_network_state(
    widgets: &DownloadRowWidgets,
    state: &DownloadState,
    connectivity: Connectivity,
    unavailable_now: bool,
) {
    let presentation = episode_network_presentation(connectivity, state, unavailable_now);
    widgets
        .root
        .set_opacity(if presentation == EpisodeNetworkPresentation::Normal {
            1.0
        } else {
            0.55
        });
    if presentation == EpisodeNetworkPresentation::Normal {
        return;
    }
    while let Some(child) = widgets.status.first_child() {
        widgets.status.remove(&child);
    }
    let label = gtk4::Label::new(Some(&strings::text(match presentation {
        EpisodeNetworkPresentation::NeedsNetwork => strings::PODCAST_NEEDS_NETWORK,
        EpisodeNetworkPresentation::UnavailableNow => strings::PODCAST_UNAVAILABLE_NOW,
        EpisodeNetworkPresentation::Normal => unreachable!(),
    })));
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    widgets.status.append(&label);
}

pub(super) fn update_download_state(widgets: &DownloadRowWidgets, state: &DownloadState) {
    while let Some(child) = widgets.status.first_child() {
        widgets.status.remove(&child);
    }
    widgets.status.append(&download_status(state));
    widgets.action.set_icon_name(match state {
        DownloadState::Downloaded { .. } => "object-select-symbolic",
        DownloadState::Failed { .. } => "view-refresh-symbolic",
        _ => "folder-download-symbolic",
    });
    widgets
        .action
        .set_tooltip_text(Some(&strings::text(match state {
            DownloadState::Downloaded { .. } => strings::PODCAST_DELETE_DOWNLOAD,
            DownloadState::Failed { .. } => strings::PODCAST_RETRY_DOWNLOAD,
            _ => strings::PODCAST_DOWNLOAD,
        })));
    widgets.action.set_sensitive(!matches!(
        state,
        DownloadState::Queued | DownloadState::Downloading { .. }
    ));
}

pub(super) fn download_status(state: &DownloadState) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
    if matches!(state, DownloadState::NotDownloaded) {
        return root.upcast();
    }
    let failure = download_failure_presentation(state);
    let localized_tooltip = failure
        .as_ref()
        .map(|failure| localized_download_failure_reason(failure.tooltip));
    let localized_detail = failure
        .as_ref()
        .and_then(|failure| failure.visible_detail)
        .map(localized_download_failure_reason);
    if let Some(failure) = localized_tooltip.as_deref() {
        root.set_tooltip_text(Some(failure));
    }
    root.set_size_request(110, -1);
    let label = gtk4::Label::new(None);
    label.set_xalign(1.0);
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    match state {
        DownloadState::NotDownloaded => unreachable!(),
        DownloadState::Queued => label.set_text(&strings::text(strings::PODCAST_DOWNLOAD_QUEUED)),
        DownloadState::Downloading {
            received_bytes,
            total_bytes,
        } => {
            label.set_text(&format!(
                "{} · {}",
                strings::text(strings::PODCAST_DOWNLOADING),
                strings::compact_file_size(*received_bytes)
            ));
            if let Some(total) = total_bytes.filter(|total| *total > 0) {
                let progress = gtk4::ProgressBar::new();
                progress.set_fraction((*received_bytes as f64 / total as f64).clamp(0.0, 1.0));
                root.append(&progress);
            } else {
                let spinner = gtk4::Spinner::new();
                spinner.start();
                spinner.set_halign(gtk4::Align::End);
                root.append(&spinner);
            }
        }
        DownloadState::Downloaded { bytes } => {
            label.set_text(&strings::compact_file_size(*bytes));
        }
        DownloadState::Missing => {
            label.set_text(&strings::text(strings::PODCAST_DOWNLOAD_MISSING));
        }
        DownloadState::Failed { .. } => {
            label.set_text(&strings::text(strings::PODCAST_DOWNLOAD_FAILED));
            if let Some(detail) = localized_detail.as_deref() {
                let reason = gtk4::Label::new(Some(detail));
                reason.set_xalign(1.0);
                reason.set_wrap(true);
                reason.set_justify(gtk4::Justification::Right);
                reason.add_css_class("caption");
                reason.add_css_class("dim-label");
                root.append(&reason);
            }
        }
    }
    root.prepend(&label);
    root.upcast()
}
