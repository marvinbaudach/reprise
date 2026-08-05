//! Scan-card presentation for the library analysis batch.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;

use super::scan_flow::ScanControls;
use super::spectrogram_batch::{SpectrogramBatch, SpectrogramBatchProgress, SpectrogramBatchState};
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

fn presentation(progress: SpectrogramBatchProgress) -> ProgressPresentation {
    let title = match progress.state {
        SpectrogramBatchState::Idle => String::new(),
        SpectrogramBatchState::Running => strings::text(strings::SPECTROGRAM_BATCH_ANALYZING),
        SpectrogramBatchState::Complete => strings::text(strings::SPECTROGRAM_BATCH_COMPLETE),
        SpectrogramBatchState::Stopped => strings::text(strings::SPECTROGRAM_BATCH_STOPPED),
        SpectrogramBatchState::Failed => strings::text(strings::SPECTROGRAM_BATCH_FAILED),
    };
    // A finished run reports what it achieved, a running one where it is.
    let detail = if progress.state == SpectrogramBatchState::Running {
        strings::spectrogram_batch_progress(progress.analyzed, progress.total)
    } else {
        strings::spectrogram_batch_summary(progress.analyzed, progress.failed)
    };
    ProgressPresentation {
        visible: progress.is_worth_showing(),
        title,
        detail,
        fraction: progress.fraction(),
        auto_hide: progress.is_terminal(),
    }
}

pub(in crate::ui) fn install(scan_controls: &ScanControls, batch: &Rc<SpectrogramBatch>) {
    let controls = scan_controls.clone();
    let hide_generation = Rc::new(Cell::new(0u64));
    // The card's cancel gesture reaches this batch only while this batch is
    // what the card is showing; a real scan owns it whenever one runs.
    scan_controls.add_on_cancel_requested({
        let controls = controls.clone();
        let batch = Rc::downgrade(batch);
        move || {
            if controls.is_scanning() {
                return;
            }
            if let Some(batch) = batch.upgrade() {
                if batch.is_running() {
                    batch.cancel();
                }
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
#[path = "spectrogram_batch_progress_tests.rs"]
mod tests;
