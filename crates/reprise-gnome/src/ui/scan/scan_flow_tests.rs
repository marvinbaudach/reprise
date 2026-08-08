use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use reprise_core::library::scanner::ScanProgress;

use super::{publish_latest_progress, ScanCompletion, ScanControls};
use crate::ui::scan_chrome::ScanChromeView;
use crate::ui::scan_progress::ScanProgressView;

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
    completion.add(move || {
        calls_for_callback.set(calls_for_callback.get() + 1);
        reentrant_completion.add(|| {});
    });

    completion.notify();

    assert_eq!(calls.get(), 1);
}

#[test]
fn scan_completion_notifies_cover_and_rendering_follow_ups() {
    let completion = ScanCompletion::default();
    let cover_starts = Rc::new(Cell::new(0));
    let rendering_starts = Rc::new(Cell::new(0));
    completion.add({
        let cover_starts = cover_starts.clone();
        move || cover_starts.set(cover_starts.get() + 1)
    });
    completion.add({
        let rendering_starts = rendering_starts.clone();
        move || rendering_starts.set(rendering_starts.get() + 1)
    });

    completion.notify();

    assert_eq!(cover_starts.get(), 1);
    assert_eq!(rendering_starts.get(), 1);
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
            total: Some(9),
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
    assert_eq!(total, Some(9));
    assert_eq!(current_path, PathBuf::from("second.flac"));
    assert!(receiver.is_empty());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_9_foreground_progress_view_replays_and_tracks_the_active_scan() {
    if gtk4::init().is_err() {
        return;
    }
    let button = gtk4::Button::new();
    let main = ScanProgressView::new();
    let controls = ScanControls::new(&button, &main);
    controls.show_progress(&ScanProgress::Scanning {
        processed: 2,
        total: Some(5),
        current_path: PathBuf::from("song.flac"),
    });
    let foreground = ScanChromeView::new();

    controls.attach_chrome_view(&foreground);

    assert!(main.widget().reveals_child());
    assert!(foreground.chip_widget().is_visible());
    assert!(foreground.line_widget().is_visible());
    controls.finish_progress();
    assert!(main.widget().reveals_child());
    assert!(foreground.chip_widget().is_visible());
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(Duration::from_millis(900), move || quit.quit());
    main_loop.run();
    assert!(!main.widget().reveals_child());
    assert!(!foreground.chip_widget().is_visible());
    assert!(!foreground.line_widget().is_visible());

    drop(foreground);
    controls.show_progress(&ScanProgress::Discovering);
    assert!(main.widget().reveals_child());
    assert_eq!(controls.foreground_progress_count(), 0);
}
