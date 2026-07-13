use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::cover_download_batch::{BatchProgress, BatchState};
use super::preferences::PreferencesContext;
use super::strings;

#[derive(Clone)]
struct CoverProgressWidgets {
    row: adw::ActionRow,
    progress: gtk4::ProgressBar,
}

fn build_progress_widgets() -> CoverProgressWidgets {
    let progress = gtk4::ProgressBar::builder()
        .width_request(220)
        .valign(gtk4::Align::Center)
        .build();
    let row = adw::ActionRow::builder().visible(false).build();
    row.add_suffix(&progress);
    CoverProgressWidgets { row, progress }
}

fn apply_progress(widgets: &CoverProgressWidgets, progress: BatchProgress) {
    let title = match progress.state {
        BatchState::Idle => {
            widgets.row.set_visible(false);
            return;
        }
        BatchState::Running => strings::COVER_DOWNLOAD_CHECKING,
        BatchState::Complete => strings::COVER_DOWNLOAD_COMPLETE,
        BatchState::Stopped => strings::COVER_DOWNLOAD_STOPPED,
        BatchState::Failed => strings::COVER_DOWNLOAD_FAILED,
    };
    widgets.row.set_title(&strings::text(title));
    widgets.row.set_subtitle(&strings::cover_download_progress(
        progress.checked,
        progress.total,
        progress.downloaded,
        progress.unavailable,
    ));
    widgets.progress.set_fraction(progress.fraction());
    widgets.row.set_visible(true);
}

impl PreferencesContext {
    pub(super) fn add_cover_download_progress(&self, group: &adw::PreferencesGroup) {
        let widgets = build_progress_widgets();
        let row = widgets.row.downgrade();
        let progress = widgets.progress.downgrade();
        self.cover_batch.subscribe_progress(move |state| {
            let (Some(row), Some(progress)) = (row.upgrade(), progress.upgrade()) else {
                return false;
            };
            apply_progress(&CoverProgressWidgets { row, progress }, state);
            true
        });
        group.add(&widgets.row);
    }
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;
    use libadwaita::prelude::*;

    use super::{apply_progress, build_progress_widgets};
    use crate::ui::cover_download_batch::{BatchProgress, BatchState};

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn progress_widgets_show_running_fraction_and_counts() {
        if gtk4::init().is_err() {
            return;
        }
        let widgets = build_progress_widgets();
        apply_progress(
            &widgets,
            BatchProgress {
                state: BatchState::Running,
                checked: 2,
                total: 4,
                downloaded: 1,
                unavailable: 0,
            },
        );

        assert!(widgets.row.is_visible());
        assert_eq!(widgets.progress.fraction(), 0.5);
        let subtitle = widgets.row.subtitle().unwrap();
        assert!(subtitle.contains("2 of 4"));
        assert!(subtitle.contains('1'));
    }
}
