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
/// sidebar scan card (via `ScanControls::show_batch_progress`). Terminal
/// states (complete/failed) remain visible briefly and then hide
/// automatically, matching the old headerbar banner's behaviour.
pub(in crate::ui) fn install(scan_controls: &ScanControls, batch: &Rc<CoverDownloadBatch>) {
    let controls = scan_controls.clone();
    let hide_generation = Rc::new(Cell::new(0u64));
    // Whether the card currently shows *this* batch. Only then may a state
    // with nothing to show take the card away — otherwise an idle cover batch
    // would clear a card another job had put up.
    let showing = Rc::new(Cell::new(false));

    // The card's cancel gesture belongs to whatever the card is showing. A
    // real scan owns it while one is running; only otherwise does it reach
    // this batch — and it never touches the scan's own cancellation flag.
    scan_controls.add_on_cancel_requested({
        let controls = controls.clone();
        let batch = Rc::downgrade(batch);
        move || {
            if controls.is_scanning() {
                return;
            }
            if let Some(batch) = batch.upgrade() {
                batch.cancel();
            }
        }
    });

    batch.subscribe_progress(|| true, {
        let controls = controls.clone();
        let hide_generation = hide_generation.clone();
        let showing = showing.clone();
        move |progress| {
            let pres = presentation(progress);
            // A library scan's own progress takes priority over the cover
            // batch — don't clobber the scan card while a scan is active.
            if controls.is_scanning() {
                return;
            }
            if !pres.visible {
                // Idle: the batch has nothing to show. If it was the one
                // showing, its card goes with it — a cancelled run that left
                // its card behind could not be dismissed at all.
                hide_generation.set(hide_generation.get().wrapping_add(1));
                if showing.replace(false) {
                    controls.finish_progress();
                }
                return;
            }

            controls.show_batch_progress(&pres.title, &pres.detail, pres.fraction);
            showing.set(true);

            let generation = hide_generation.get().wrapping_add(1);
            hide_generation.set(generation);
            if pres.auto_hide {
                let controls = controls.clone();
                let hide_generation = hide_generation.clone();
                let showing = showing.clone();
                glib::timeout_add_seconds_local_once(TERMINAL_HIDE_DELAY_SECS, move || {
                    if hide_generation.get() != generation {
                        return;
                    }
                    // Only hide if no scan started in the meantime.
                    if !controls.is_scanning() {
                        showing.set(false);
                        controls.finish_progress();
                    }
                });
            }
        }
    });

    let batch = batch.clone();
    scan_controls.add_on_complete(move || batch.start());
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use gtk4::prelude::*;

    use super::{install, presentation};
    use crate::ui::cover_download_batch::{BatchProgress, BatchState, CoverDownloadBatch};
    use crate::ui::scan_flow::ScanControls;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn the_cards_cancel_stops_the_cover_batch_without_sharing_the_scan_flag() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let button = gtk4::Button::new();
        let view = crate::ui::scan_progress::ScanProgressView::new();
        let controls = ScanControls::new(&button, &view);
        let conn = Rc::new(crate::test_db::open().unwrap());
        let runtime = crate::ui::cover_download_worker::setup_for_test();
        let track_list = Rc::new(crate::ui::track_list::TrackList::new(
            conn.clone(),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            runtime.clone(),
        ));
        let batch = CoverDownloadBatch::new(&conn, &runtime, &track_list, None);
        install(&controls, &batch);

        batch.set_progress_for_test(BatchProgress {
            state: BatchState::Running,
            checked: 1,
            total: 5,
            downloaded: 0,
            unavailable: 0,
        });
        assert!(
            view.widget().reveals_child(),
            "a running cover batch shows the scan card"
        );

        // While a real scan owns the card, its cancel belongs to the scan alone.
        button.set_sensitive(false);
        controls.request_cancel();
        assert_eq!(
            batch.progress_for_test().state,
            BatchState::Running,
            "cancelling a library scan must not abort the cover batch"
        );

        // With the card showing the batch, the same gesture stops the batch.
        button.set_sensitive(true);
        controls.request_cancel();
        assert_eq!(
            batch.progress_for_test().state,
            BatchState::Idle,
            "the card's cancel gesture must stop the cover batch"
        );
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            !view.widget().reveals_child()
        });
        assert!(
            !view.widget().reveals_child(),
            "a cancelled batch takes its card away instead of leaving it stuck"
        );
    }

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
