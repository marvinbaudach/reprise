use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;

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

/// Subscribes to cover-download batch progress and projects it onto the
/// sidebar scan card (via `ScanControls::show_cover_progress`). Terminal
/// states (complete/failed) remain visible briefly and then hide
/// automatically, matching the old headerbar banner's behaviour.
pub(super) fn install(scan_controls: &ScanControls, batch: &Rc<CoverDownloadBatch>) {
    let controls = scan_controls.clone();
    let hide_generation = Rc::new(Cell::new(0u64));

    batch.subscribe_progress(
        || true,
        {
            let controls = controls.clone();
            let hide_generation = hide_generation.clone();
            move |progress| {
                let pres = presentation(progress);
                if !pres.visible {
                    return;
                }
                // A library scan's own progress takes priority over the cover
                // batch — don't clobber the scan card while a scan is active.
                if controls.is_scanning() {
                    return;
                }

                controls.show_cover_progress(&pres.title, &pres.detail, pres.fraction);

                if pres.auto_hide {
                    let generation = hide_generation.get().wrapping_add(1);
                    hide_generation.set(generation);
                    let controls = controls.clone();
                    let hide_generation = hide_generation.clone();
                    glib::timeout_add_seconds_local_once(TERMINAL_HIDE_DELAY_SECS, move || {
                        if hide_generation.get() != generation {
                            return;
                        }
                        // Only hide if no scan started in the meantime.
                        if !controls.is_scanning() {
                            controls.finish_progress();
                        }
                    });
                }
            }
        },
    );

    let batch = batch.clone();
    scan_controls.set_on_complete(move || batch.start());
}

#[cfg(test)]
mod tests {
    use super::presentation;
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
        assert_eq!(running.title, "Checking missing album covers\u{2026}");
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
}
