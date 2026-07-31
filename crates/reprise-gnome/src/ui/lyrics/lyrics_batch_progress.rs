//! Scan-card presentation for the library lyrics batch.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use reprise_core::db::Db;

use super::lyrics_batch::{LyricsBatch, LyricsBatchProgress, LyricsBatchState};
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

fn presentation(progress: LyricsBatchProgress) -> ProgressPresentation {
    let title = match progress.state {
        LyricsBatchState::Idle => String::new(),
        LyricsBatchState::Running => strings::text(strings::LYRICS_BATCH_CHECKING),
        LyricsBatchState::Complete => strings::text(strings::LYRICS_BATCH_COMPLETE),
        LyricsBatchState::Failed => strings::text(strings::LYRICS_BATCH_FAILED),
    };
    ProgressPresentation {
        visible: progress.state != LyricsBatchState::Idle,
        title,
        detail: strings::lyrics_batch_progress(
            progress.checked,
            progress.total,
            progress.downloaded,
            progress.unavailable,
        ),
        fraction: progress.fraction().clamp(0.0, 1.0),
        auto_hide: matches!(
            progress.state,
            LyricsBatchState::Complete | LyricsBatchState::Failed
        ),
    }
}

pub(in crate::ui) fn build(conn: &Rc<Db>, scan_controls: &ScanControls) -> Rc<LyricsBatch> {
    let batch = LyricsBatch::new(conn);
    install(scan_controls, &batch);
    batch
}

pub(in crate::ui) fn install(scan_controls: &ScanControls, batch: &Rc<LyricsBatch>) {
    let controls = scan_controls.clone();
    let hide_generation = Rc::new(Cell::new(0u64));
    // The card's cancel gesture belongs to whatever the card is showing. A
    // real scan owns it while one is running; only otherwise does it reach the
    // batch — and it never touches the scan's own cancellation flag.
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
        move |progress| {
            let presentation = presentation(progress);
            if !presentation.visible || controls.is_scanning() {
                return;
            }
            controls.show_batch_progress(
                &presentation.title,
                &presentation.detail,
                presentation.fraction,
            );
            let generation = hide_generation.get().wrapping_add(1);
            hide_generation.set(generation);
            if presentation.auto_hide {
                let controls = controls.clone();
                let hide_generation = hide_generation.clone();
                glib::timeout_add_seconds_local_once(TERMINAL_HIDE_DELAY_SECS, move || {
                    if hide_generation.get() == generation && !controls.is_scanning() {
                        controls.finish_progress();
                    }
                });
            }
        }
    });
}

#[cfg(test)]
#[path = "lyrics_batch_progress_tests.rs"]
mod tests;
