use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use reprise_core::library::scanner::ScanProgress;
use reprise_core::waveform::{WaveformBackend, WaveformError};

use super::{publish_latest_progress, ScanCompletion, ScanControls};
use crate::ui::scan_progress::ScanProgressView;

struct FakeWaveformBackend;

impl WaveformBackend for FakeWaveformBackend {
    fn extract_peaks(
        &self,
        _path: &std::path::Path,
        buckets: usize,
    ) -> Result<Vec<u8>, WaveformError> {
        Ok(vec![0; buckets])
    }
}

#[test]
fn cancellation_is_shared_across_clones_and_reset_between_scans() {
    let cancellation = super::ScanCancellation::default();
    let worker_view = cancellation.clone();

    cancellation.request();
    assert!(worker_view.is_requested());

    worker_view.reset();
    assert!(!cancellation.is_requested());
}

#[test]
fn scan_completion_callback_runs_without_holding_its_refcell_borrow() {
    let completion = ScanCompletion::default();
    let calls = Rc::new(Cell::new(0));
    let calls_for_callback = calls.clone();
    let reentrant_completion = completion.clone();
    completion.set(move || {
        calls_for_callback.set(calls_for_callback.get() + 1);
        reentrant_completion.set(|| {});
    });

    completion.notify();

    assert_eq!(calls.get(), 1);
}

#[test]
fn progress_channel_keeps_only_the_latest_pending_update() {
    let (sender, receiver) = async_channel::bounded(1);
    publish_latest_progress(&sender, &receiver, ScanProgress::Discovering);
    publish_latest_progress(
        &sender,
        &receiver,
        ScanProgress::Scanning {
            processed: 2,
            total: 9,
            current_path: PathBuf::from("second.flac"),
        },
    );

    let progress = receiver.try_recv().expect("latest progress event");
    let ScanProgress::Scanning {
        processed,
        total,
        current_path,
    } = progress
    else {
        panic!("expected the newest scanning event");
    };
    assert_eq!(processed, 2);
    assert_eq!(total, 9);
    assert_eq!(current_path, PathBuf::from("second.flac"));
    assert!(receiver.is_empty());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn foreground_progress_view_replays_and_tracks_the_active_scan() {
    if gtk4::init().is_err() {
        return;
    }
    let button = gtk4::Button::new();
    let main = ScanProgressView::new();
    let controls = ScanControls::new(&button, &main, Arc::new(FakeWaveformBackend));
    controls.show_progress(&ScanProgress::Scanning {
        processed: 2,
        total: 5,
        current_path: PathBuf::from("song.flac"),
    });
    let foreground = ScanProgressView::new();

    controls.attach_progress_view(&foreground);

    assert!(main.widget().reveals_child());
    assert!(foreground.widget().reveals_child());
    controls.finish_progress();
    assert!(!main.widget().reveals_child());
    assert!(!foreground.widget().reveals_child());

    drop(foreground);
    controls.show_progress(&ScanProgress::Discovering);
    assert!(main.widget().reveals_child());
    assert!(controls.foreground_progress.borrow().is_empty());
}
