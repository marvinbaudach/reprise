//! Background scan execution and GTK-thread result reconciliation.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library;
use reprise_core::library::scanner::{ScanError, ScanProgress, ScanReport};
use reprise_core::library::settings;
use reprise_core::library::watcher::WatcherHandle;
use reprise_core::waveform::WaveformBackend;

use super::scan_controls::ScanControls;
use super::scan_watcher::start_or_restart_watcher;
use super::scan_waveform_analysis::analyze_waveforms;
use super::sidebar::Sidebar;
use super::strings;
use super::toasts;
use super::track_list::TrackList;

pub(in crate::ui) fn spawn_scan(
    folder: PathBuf,
    db_path: PathBuf,
    controls: ScanControls,
    toast_overlay: adw::ToastOverlay,
    track_list: Rc<TrackList>,
    sidebar: Rc<Sidebar>,
    watcher_state: Rc<RefCell<Option<WatcherHandle>>>,
) {
    begin_scan_ui(&controls);

    let (progress_sender, progress_receiver) = async_channel::bounded::<ScanProgress>(1);
    let (result_sender, result_receiver) = async_channel::bounded(1);
    let (drained_sender, drained_receiver) = async_channel::bounded(1);

    let thread_folder = folder.clone();
    let thread_db_path = db_path.clone();
    let waveform_backend = controls.waveform_backend();
    let stale_receiver = progress_receiver.clone();
    std::thread::spawn(move || {
        let result = run_scan(
            &thread_db_path,
            &thread_folder,
            waveform_backend.as_ref(),
            |progress| {
                publish_latest_progress(&progress_sender, &stale_receiver, progress);
            },
        );
        drop(progress_sender);
        drop(stale_receiver);
        if let Err(error) = result_sender.send_blocking(result) {
            tracing::warn!(%error, "scan result dropped: UI receiver is gone");
        }
    });

    let progress_controls = controls.clone();
    glib::spawn_future_local(async move {
        while let Ok(progress) = progress_receiver.recv().await {
            progress_controls.show_progress(&progress);
        }
        let _ = drained_sender.try_send(());
    });

    glib::spawn_future_local(async move {
        let outcome = result_receiver.recv().await;
        let _ = drained_receiver.recv().await;
        finish_scan_ui(&controls);
        reconcile_outcome(
            outcome,
            &folder,
            db_path,
            &controls,
            &toast_overlay,
            &track_list,
            &sidebar,
            &watcher_state,
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn reconcile_outcome(
    outcome: Result<Result<ScanReport, ScanError>, async_channel::RecvError>,
    folder: &std::path::Path,
    db_path: PathBuf,
    controls: &ScanControls,
    toast_overlay: &adw::ToastOverlay,
    track_list: &Rc<TrackList>,
    sidebar: &Rc<Sidebar>,
    watcher_state: &Rc<RefCell<Option<WatcherHandle>>>,
) {
    match outcome {
        Ok(Ok(report)) => {
            if controls.is_cancel_requested() {
                tracing::info!("scan cancelled by user; keeping already-imported tracks");
                track_list.reload();
                sidebar.refresh("scan cancelled");
            } else {
                tracing::info!(?report, "scan complete");
                let result = report.to_scan_result();
                toasts::show(
                    toast_overlay,
                    &strings::scan_complete_toast(result.new_tracks, result.failed),
                );
                track_list.reload();
                sidebar.refresh("scan completed");
                start_or_restart_watcher(
                    watcher_state,
                    folder,
                    db_path,
                    Rc::downgrade(track_list),
                    Rc::downgrade(sidebar),
                );
            }
            controls.notify_complete();
        }
        Ok(Err(error)) => {
            tracing::error!(%error, "scan failed");
            toasts::show(
                toast_overlay,
                &format!("{}{error}", &strings::text(strings::SCAN_FAILED_PREFIX)),
            );
        }
        Err(error) => {
            tracing::error!(%error, "scan worker channel closed unexpectedly");
            toasts::show(
                toast_overlay,
                &format!("{}{error}", &strings::text(strings::SCAN_FAILED_PREFIX)),
            );
        }
    }
}

pub(in crate::ui) fn publish_latest_progress(
    sender: &async_channel::Sender<ScanProgress>,
    receiver: &async_channel::Receiver<ScanProgress>,
    progress: ScanProgress,
) {
    match sender.try_send(progress) {
        Ok(()) => {}
        Err(async_channel::TrySendError::Full(progress)) => {
            let _ = receiver.try_recv();
            if let Err(error) = sender.try_send(progress) {
                tracing::warn!(%error, "scan progress dropped: UI receiver is gone");
            }
        }
        Err(async_channel::TrySendError::Closed(_)) => {
            tracing::warn!("scan progress dropped: UI receiver is gone");
        }
    }
}

fn begin_scan_ui(controls: &ScanControls) {
    controls.reset_cancel();
    controls.button.set_sensitive(false);
    controls.notify_scan_state();
    controls.button.set_label(&strings::text(strings::SCANNING));
    controls
        .button
        .set_tooltip_text(Some(&strings::text(strings::SCANNING)));
    controls.show_progress(&ScanProgress::Discovering);
}

fn finish_scan_ui(controls: &ScanControls) {
    controls.finish_progress();
    controls.button.set_sensitive(true);
    controls.notify_scan_state();
    controls
        .button
        .set_label(&strings::text(strings::SCAN_FOLDER));
    controls.button.set_tooltip_text(None);
}

fn run_scan(
    db_path: &std::path::Path,
    folder: &std::path::Path,
    waveform_backend: &dyn WaveformBackend,
    on_progress: impl FnMut(ScanProgress),
) -> Result<ScanReport, ScanError> {
    let mut worker_conn = reprise_core::db::open_migrated(Some(db_path))?;
    let report =
        library::scanner::scan_folder_with_progress(&mut worker_conn, folder, on_progress)?;
    if let Err(error) = settings::set_library_root(&worker_conn, &folder.to_string_lossy()) {
        tracing::error!(%error, "failed to persist library root after scan");
    }
    analyze_waveforms(db_path, waveform_backend);
    Ok(report)
}
