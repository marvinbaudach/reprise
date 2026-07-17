//! Background scan execution and GTK-thread result reconciliation.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library;
use reprise_core::library::scanner::{ScanError, ScanOutcome, ScanProgress};
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScanFailureNotice {
    title: String,
    action: String,
    target: reprise_core::view_source::ViewSource,
}

#[derive(Debug)]
struct ProcessedScanOutcome {
    outcome: ScanOutcome,
    auto_cleaned_ids: Vec<i64>,
}

fn scan_heal_toast(moved: u32, healed: u32) -> Option<String> {
    let mut parts = Vec::with_capacity(2);
    if moved > 0 {
        parts.push(strings::moved_files_relinked(moved));
    }
    if healed > 0 {
        parts.push(strings::failed_files_imported(healed));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn scan_failure_notices(failed: u32) -> Vec<ScanFailureNotice> {
    (failed > 0)
        .then(|| ScanFailureNotice {
            title: strings::import_issue_failed(failed),
            action: strings::issue_text(strings::IMPORT_ISSUE_DETAILS),
            target: reprise_core::view_source::ViewSource::ImportErrors,
        })
        .into_iter()
        .collect()
}

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
    outcome: Result<Result<ProcessedScanOutcome, ScanError>, async_channel::RecvError>,
    folder: &std::path::Path,
    db_path: PathBuf,
    controls: &ScanControls,
    toast_overlay: &adw::ToastOverlay,
    track_list: &Rc<TrackList>,
    sidebar: &Rc<Sidebar>,
    watcher_state: &Rc<RefCell<Option<WatcherHandle>>>,
) {
    match outcome {
        Ok(Ok(ProcessedScanOutcome {
            outcome: ScanOutcome::Completed(report),
            auto_cleaned_ids,
        })) => {
            controls.set_library_root_unavailable(false);
            track_list.notify_scan_postprocessed(&auto_cleaned_ids);
            if controls.is_cancel_requested() {
                tracing::info!("scan cancelled by user; keeping already-imported tracks");
                track_list.reload();
                sidebar.refresh("scan cancelled");
            } else {
                tracing::info!(?report, "scan complete");
                let result = report.to_scan_result();
                track_list.set_library_root_unavailable(None);
                sidebar.refresh("scan completed");
                if let Some(notice) = scan_failure_notices(result.failed).into_iter().next() {
                    let toast = adw::Toast::new(&notice.title);
                    toast.set_button_label(Some(&notice.action));
                    let target = notice.target;
                    let sidebar = sidebar.clone();
                    toast.connect_button_clicked(move |_| {
                        sidebar.refresh_and_select(target.clone(), "scan error details");
                    });
                    toast_overlay.add_toast(toast);
                }
                if let Some(heal_toast) = scan_heal_toast(report.moved, report.healed) {
                    toasts::show(toast_overlay, &heal_toast);
                } else if result.failed == 0 {
                    toasts::show(
                        toast_overlay,
                        &strings::scan_complete_toast(result.new_tracks, 0),
                    );
                }
                start_or_restart_watcher(
                    watcher_state,
                    folder,
                    db_path,
                    controls.clone(),
                    Rc::downgrade(track_list),
                    Rc::downgrade(sidebar),
                );
            }
            controls.notify_complete();
        }
        // `scan_folder` ran but had no evidence that the persisted root was
        // reachable. This is a durable StatusPage/sidebar-card state with
        // Retry, not a transient error toast.
        Ok(Ok(ProcessedScanOutcome {
            outcome: ScanOutcome::RootUnavailable { root },
            auto_cleaned_ids,
        })) => {
            debug_assert!(auto_cleaned_ids.is_empty());
            tracing::warn!(root = %root.display(), "scan: library folder unavailable");
            controls.set_library_root_unavailable(true);
            controls.show_root_unavailable(&root);
            track_list.set_library_root_unavailable(Some(root));
            sidebar.refresh("library root unavailable");
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
) -> Result<ProcessedScanOutcome, ScanError> {
    let mut worker_conn = reprise_core::db::open_migrated(Some(db_path))?;
    let outcome =
        library::scanner::scan_folder_with_progress(&mut worker_conn, folder, on_progress)?;
    if let Err(error) = settings::set_library_root(&worker_conn, &folder.to_string_lossy()) {
        tracing::error!(%error, "failed to persist library root after scan");
    }
    let processed = postprocess_scan_outcome(&mut worker_conn, outcome, now_unix())?;
    analyze_waveforms(db_path, waveform_backend);
    Ok(processed)
}

fn postprocess_scan_outcome(
    conn: &mut rusqlite::Connection,
    outcome: ScanOutcome,
    now: i64,
) -> Result<ProcessedScanOutcome, ScanError> {
    let auto_cleaned_ids = match &outcome {
        ScanOutcome::Completed(report) => {
            library::scanner::finalize_completed_scan(conn, report, now)?
        }
        ScanOutcome::RootUnavailable { .. } => Vec::new(),
    };
    Ok(ProcessedScanOutcome {
        outcome,
        auto_cleaned_ids,
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

#[cfg(test)]
mod task_3_3_tests {
    use super::*;

    // UX FB-3: one scan may collect many file failures, but completion
    // projects them into one persistent, actionable notice rather than one
    // toast per file.
    #[test]
    fn fb_3_scan_failures_produce_one_actionable_completion_notice() {
        let notices = scan_failure_notices(3);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].title, "3 failed");
        assert_eq!(notices[0].action, "Details");
        assert_eq!(
            notices[0].target,
            reprise_core::view_source::ViewSource::ImportErrors
        );
        assert_eq!(scan_failure_notices(1)[0].title, "1 failed");
        assert!(scan_failure_notices(0).is_empty());
    }
}

#[cfg(test)]
mod task_5_5_tests {
    use super::*;
    use reprise_core::library::scanner::ScanReport;
    use reprise_core::library::settings::AutoCleanSetting;

    #[test]
    fn heal_toast_omits_zero_parts_and_is_absent_when_nothing_healed() {
        assert_eq!(scan_heal_toast(0, 0), None);
        assert_eq!(
            scan_heal_toast(3, 0).as_deref(),
            Some("3 moved files relinked")
        );
        assert_eq!(
            scan_heal_toast(0, 2).as_deref(),
            Some("2 previously failed files imported")
        );
        assert_eq!(
            scan_heal_toast(3, 2).as_deref(),
            Some("3 moved files relinked · 2 previously failed files imported")
        );
    }

    #[test]
    fn completed_scan_persists_relinks_and_runs_auto_clean_but_unavailable_does_neither() {
        let mut completed_conn = reprise_core::db::open_migrated(None).unwrap();
        completed_conn
            .execute(
                "INSERT INTO tracks \
                 (id,path,title,artist,added_at,missing_since,missing_reason) \
                 VALUES (1,'/gone.flac','Gone','Artist',0,0,'deleted')",
                [],
            )
            .unwrap();
        settings::set_missing_auto_clean(&completed_conn, AutoCleanSetting::Days(0)).unwrap();
        settings::set_auto_clean_armed_at(&completed_conn, 0).unwrap();
        let report = ScanReport {
            moved: 3,
            ..ScanReport::default()
        };

        let processed =
            postprocess_scan_outcome(&mut completed_conn, ScanOutcome::Completed(report), 10)
                .unwrap();
        assert_eq!(processed.auto_cleaned_ids, vec![1]);
        assert_eq!(
            settings::get_last_scan_relinked(&completed_conn).unwrap(),
            Some(3)
        );

        let mut unavailable_conn = reprise_core::db::open_migrated(None).unwrap();
        unavailable_conn
            .execute(
                "INSERT INTO tracks \
                 (id,path,title,artist,added_at,missing_since,missing_reason) \
                 VALUES (1,'/gone.flac','Gone','Artist',0,0,'deleted')",
                [],
            )
            .unwrap();
        settings::set_missing_auto_clean(&unavailable_conn, AutoCleanSetting::Days(0)).unwrap();
        settings::set_auto_clean_armed_at(&unavailable_conn, 0).unwrap();
        let processed = postprocess_scan_outcome(
            &mut unavailable_conn,
            ScanOutcome::RootUnavailable {
                root: PathBuf::from("/offline"),
            },
            10,
        )
        .unwrap();
        assert!(processed.auto_cleaned_ids.is_empty());
        assert_eq!(
            settings::get_last_scan_relinked(&unavailable_conn).unwrap(),
            None
        );
        assert_eq!(
            unavailable_conn
                .query_row("SELECT count(*) FROM tracks", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
