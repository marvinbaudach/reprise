use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use super::cover_download_batch::{BatchProgress, BatchState, CoverDownloadBatch};
use super::scan_flow::ScanControls;
use super::strings;

const TERMINAL_HIDE_DELAY_SECS: u32 = 3;

#[derive(Debug, PartialEq)]
struct ProgressPresentation {
    visible: bool,
    title: String,
    detail: String,
    fraction: f64,
    auto_hide: bool,
}

fn presentation(progress: BatchProgress) -> ProgressPresentation {
    let title = match progress.state {
        BatchState::Idle => "".to_string(),
        BatchState::Running => strings::text(strings::COVER_DOWNLOAD_CHECKING),
        BatchState::Complete => strings::text(strings::COVER_DOWNLOAD_COMPLETE),
        BatchState::Failed => strings::text(strings::COVER_DOWNLOAD_FAILED),
    };
    ProgressPresentation {
        visible: progress.state != BatchState::Idle,
        title,
        detail: strings::cover_download_progress(
            progress.checked,
            progress.total,
            progress.downloaded,
            progress.unavailable,
        ),
        fraction: progress.fraction().clamp(0.0, 1.0),
        auto_hide: matches!(progress.state, BatchState::Complete | BatchState::Failed),
    }
}

/// Compact main-window projection of the shared cover batch state. Terminal
/// results remain visible briefly and then hide automatically.
#[derive(Clone)]
struct MainCoverProgressView {
    revealer: gtk4::Revealer,
    title: gtk4::Label,
    detail: gtk4::Label,
    progress: gtk4::ProgressBar,
    hide_generation: Rc<Cell<u64>>,
    phase: Rc<Cell<BatchState>>,
}

impl MainCoverProgressView {
    fn new() -> Self {
        let title = gtk4::Label::builder()
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .xalign(0.0)
            .build();
        let detail = gtk4::Label::builder()
            .halign(gtk4::Align::End)
            .hexpand(true)
            .xalign(1.0)
            .build();
        detail.add_css_class("dim-label");
        detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);

        let labels = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        labels.append(&title);
        labels.append(&detail);
        let progress = gtk4::ProgressBar::builder().hexpand(true).build();
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(6);
        content.set_margin_bottom(6);
        content.append(&labels);
        content.append(&progress);

        Self {
            revealer: gtk4::Revealer::builder()
                .transition_type(gtk4::RevealerTransitionType::SlideDown)
                .child(&content)
                .build(),
            title,
            detail,
            progress,
            hide_generation: Rc::new(Cell::new(0)),
            phase: Rc::new(Cell::new(BatchState::Idle)),
        }
    }

    fn widget(&self) -> &gtk4::Revealer {
        &self.revealer
    }

    fn apply(&self, progress: BatchProgress) {
        let state = presentation(progress);
        let generation = self.hide_generation.get().wrapping_add(1);
        self.hide_generation.set(generation);
        let previous = self.phase.replace(progress.state);

        if !state.visible {
            self.revealer.set_reveal_child(false);
            return;
        }

        self.title.set_label(&state.title);
        self.detail.set_label(&state.detail);
        self.progress.set_fraction(state.fraction);
        self.revealer.set_reveal_child(true);
        if progress.state == BatchState::Running && previous == BatchState::Running {
            tracing::debug!(
                checked = progress.checked,
                total = progress.total,
                "main cover progress: running"
            );
        } else {
            tracing::info!(
                state = ?progress.state,
                checked = progress.checked,
                total = progress.total,
                "main cover progress updated"
            );
        }

        if state.auto_hide {
            let revealer = self.revealer.downgrade();
            let hide_generation = self.hide_generation.clone();
            let phase = self.phase.clone();
            glib::timeout_add_seconds_local_once(TERMINAL_HIDE_DELAY_SECS, move || {
                if hide_generation.get() != generation {
                    return;
                }
                if let Some(revealer) = revealer.upgrade() {
                    revealer.set_reveal_child(false);
                }
                phase.set(BatchState::Idle);
            });
        }
    }
}

pub(super) fn install(
    toolbar_view: &adw::ToolbarView,
    batch: &Rc<CoverDownloadBatch>,
    scan_controls: &ScanControls,
) {
    let view = MainCoverProgressView::new();
    toolbar_view.add_top_bar(view.widget());
    batch.subscribe_progress(|| true, move |progress| view.apply(progress));

    let batch = batch.clone();
    scan_controls.set_on_complete(move || batch.start());
}

#[cfg(test)]
mod tests {
    use super::{presentation, MainCoverProgressView};
    use crate::ui::cover_download_batch::{BatchProgress, BatchState};

    #[test]
    fn idle_is_hidden_and_running_shows_determinate_counts() {
        let idle = presentation(BatchProgress {
            state: BatchState::Idle,
            checked: 0,
            total: 0,
            downloaded: 0,
            unavailable: 0,
        });
        assert!(!idle.visible);
        assert!(!idle.auto_hide);

        let running = presentation(BatchProgress {
            state: BatchState::Running,
            checked: 2,
            total: 4,
            downloaded: 1,
            unavailable: 0,
        });
        assert!(running.visible);
        assert_eq!(running.title, "Checking missing album covers…");
        assert!(running.detail.contains("2 of 4"));
        assert!(running.detail.contains("1 downloaded"));
        assert_eq!(running.fraction, 0.5);
        assert!(!running.auto_hide);
    }

    #[test]
    fn terminal_states_stay_visible_briefly_and_clamp_fraction() {
        for (state, expected_title) in [
            (BatchState::Complete, "Cover check complete"),
            (BatchState::Failed, "Could not check album covers"),
        ] {
            let state = presentation(BatchProgress {
                state,
                checked: 7,
                total: 4,
                downloaded: 2,
                unavailable: 1,
            });
            assert!(state.visible);
            assert_eq!(state.title, expected_title);
            assert_eq!(state.fraction, 1.0);
            assert!(state.auto_hide);
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn widgets_show_running_counts_and_terminal_result() {
        if gtk4::init().is_err() {
            return;
        }
        let view = MainCoverProgressView::new();
        view.apply(BatchProgress {
            state: BatchState::Running,
            checked: 2,
            total: 4,
            downloaded: 1,
            unavailable: 0,
        });
        assert!(view.revealer.reveals_child());
        assert_eq!(view.progress.fraction(), 0.5);
        assert!(view.detail.label().contains("2 of 4"));

        view.apply(BatchProgress {
            state: BatchState::Complete,
            checked: 4,
            total: 4,
            downloaded: 1,
            unavailable: 1,
        });
        assert!(view.revealer.reveals_child());
        assert_eq!(view.title.label(), "Cover check complete");
    }
}
