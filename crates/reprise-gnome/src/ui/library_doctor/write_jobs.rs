//! Apply and revert: the two write jobs the user starts on purpose.
//!
//! The quiet write that follows a scan lives in `auto_apply.rs`. What these
//! share with it is the coordinator's bookkeeping — the write gate, the
//! cancellation flag, the sidebar card — and the rule that the page's screen is
//! decided by the branch that knows what the job produced, never by the
//! bookkeeping itself.

use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library_doctor::{
    DoctorApplyPlan, DoctorCleanupReport, DoctorWriteProgress, DoctorWriteReport, LibraryDoctor,
};

use super::jobs::{run_apply, run_revert};
use super::progress_card::DoctorJobKind;
use super::LibraryDoctorCoordinator;

impl LibraryDoctorCoordinator {
    pub(super) fn start_apply(self: &Rc<Self>, plan: DoctorApplyPlan) {
        if plan.track_count() == 0 || self.running.get() || self.scan_controls.is_scanning() {
            return;
        }
        let total = plan.track_count();
        let cancellation = Arc::new(AtomicBool::new(false));
        self.cancellation.borrow_mut().replace(cancellation.clone());
        self.running.set(true);
        self.job_kind.set(Some(DoctorJobKind::Apply));
        self.page.set_controls_locked(true);
        self.page.begin_job(DoctorJobKind::Apply, total);
        if let Some(review) = self.review.borrow().as_ref() {
            review.set_running(true);
        }
        self.scan_controls.button.set_sensitive(false);
        self.progress.show(DoctorJobKind::Apply, 0, total);
        let db_path = self.db_path.clone();
        let spawned = crate::ui::one_shot_task::spawn_with_progress(
            "reprise-library-doctor-apply",
            move |publish| run_apply(&db_path, &plan, &cancellation, publish),
        );
        let (progress, result) = match spawned {
            Ok(channels) => channels,
            Err(error) => {
                self.finish_write_job();
                tracing::error!(%error, "could not start Library Doctor apply worker");
                return;
            }
        };
        self.watch_write_job(DoctorJobKind::Apply, progress, result);
    }

    pub(super) fn start_revert(self: &Rc<Self>) {
        if self.running.get() || self.scan_controls.is_scanning() {
            return;
        }
        let total = {
            let conn = &self.conn;
            LibraryDoctor::new(conn)
                .last_cleanup()
                .ok()
                .flatten()
                .map(|cleanup| cleanup.track_count)
        };
        let Some(total) = total else {
            return;
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        self.cancellation.borrow_mut().replace(cancellation.clone());
        self.running.set(true);
        self.job_kind.set(Some(DoctorJobKind::Revert));
        self.open_root_page();
        self.page.set_controls_locked(true);
        self.page.begin_job(DoctorJobKind::Revert, total);
        if let Some(review) = self.review.borrow().as_ref() {
            review.set_running(true);
        }
        self.scan_controls.button.set_sensitive(false);
        self.progress.show(DoctorJobKind::Revert, 0, total);
        let db_path = self.db_path.clone();
        let spawned = crate::ui::one_shot_task::spawn_with_progress(
            "reprise-library-doctor-revert",
            move |publish| {
                run_revert(&db_path, &cancellation, publish)
                    .map(|report| report.map(combined_cleanup_report))
            },
        );
        let (progress, result) = match spawned {
            Ok(channels) => channels,
            Err(error) => {
                self.finish_write_job();
                tracing::error!(%error, "could not start Library Doctor revert worker");
                crate::ui::toasts::show(&self.toast_overlay, &error.to_string());
                return;
            }
        };
        self.watch_write_job(DoctorJobKind::Revert, progress, result);
    }

    fn watch_write_job(
        self: &Rc<Self>,
        kind: DoctorJobKind,
        progress: async_channel::Receiver<DoctorWriteProgress>,
        result: async_channel::Receiver<Result<Option<DoctorWriteReport>, String>>,
    ) {
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(progress) = progress.recv().await {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.progress.show(
                        kind,
                        progress.completed_tracks,
                        progress.total_tracks,
                    );
                }
            }
        });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let received = result.recv().await;
            let Some(coordinator) = weak.upgrade() else {
                return;
            };
            coordinator.finish_write_job();
            match received {
                Ok(Ok(Some(report))) => coordinator.handle_write_report(kind, &report),
                Ok(Ok(None)) => coordinator.page.end_job(),
                Ok(Err(error)) => {
                    tracing::error!(%error, "Library Doctor write failed");
                    coordinator.page.end_job();
                    crate::ui::toasts::show(&coordinator.toast_overlay, &error);
                }
                Err(error) => {
                    tracing::error!(%error, "Library Doctor write worker disappeared");
                    coordinator.page.end_job();
                    crate::ui::toasts::show(&coordinator.toast_overlay, &error.to_string());
                }
            }
        });
    }

    pub(super) fn finish_write_job(&self) {
        self.cancellation.borrow_mut().take();
        self.running.set(false);
        self.job_kind.set(None);
        self.page.set_controls_locked(false);
        if let Some(review) = self.review.borrow().as_ref() {
            review.set_running(false);
        }
        self.scan_controls.button.set_sensitive(true);
        self.progress.hide();
    }

    fn handle_write_report(self: &Rc<Self>, kind: DoctorJobKind, report: &DoctorWriteReport) {
        tracing::info!(
            ?kind,
            updated = report.updated_tracks,
            cancelled = report.cancelled_tracks,
            failed = report.failed_tracks,
            conflicts = report.conflict_tracks,
            unavailable = report.unavailable_tracks,
            "Library Doctor write completed"
        );
        self.refresh_written_paths(report);
        if let Some(review) = self.review.borrow().as_ref() {
            review.set_write_report(report);
        }
        self.sidebar.refresh("Library Doctor write completed");
        if kind == DoctorJobKind::Apply {
            let (albums, conflicts) = self.page.scan().map_or((0, 0), |scan| {
                let session = reprise_core::library_doctor::DoctorReviewSession::from_scan(
                    scan.clone(),
                    reprise_core::library_doctor::DoctorReviewFilter::NeedsReview,
                );
                (
                    reprise_core::library_doctor::group_review_rows(&scan, &session).len(),
                    scan.unresolved_groups.len(),
                )
            });
            self.page.show_post_apply(report, albums, conflicts);
            self.open_root_page();
        } else {
            // A finished revert put the tags back, so the applied block has
            // nothing left to report and `Undo` nothing left to undo…
            self.page.mark_reverted();
            // …and the findings it reverted are open again. The page is holding
            // the projection from before the revert, in which those proposals
            // were finished and therefore filtered out of the stored scan; only
            // a reload sees them come back. Without this the page claims
            // "Nothing to fix" while the tags on disk are unfixed, and says so
            // until the next restart.
            self.load_last_scan();
        }
        self.show_write_toasts(kind, report);
    }

    pub(super) fn refresh_written_paths(&self, report: &DoctorWriteReport) {
        let paths = report
            .rows
            .iter()
            .filter(|row| row.file_written)
            .map(|row| row.path.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let ids = report
            .rows
            .iter()
            .filter(|row| row.file_written)
            .map(|row| row.track_id)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if !paths.is_empty() {
            self.track_list.refresh_after_tag_mutation(&ids, &paths);
            (self.refresh_views)();
        }
    }

    fn show_write_toasts(self: &Rc<Self>, kind: DoctorJobKind, report: &DoctorWriteReport) {
        if report.updated_tracks > 0 && kind == DoctorJobKind::Revert {
            let title = if report.cancelled_tracks > 0 {
                crate::ui::strings::doctor_write_cancelled(
                    report.updated_tracks,
                    report.cancelled_tracks,
                )
            } else {
                crate::ui::strings::doctor_tags_reverted(report.updated_tracks)
            };
            let toast = crate::ui::toasts::plain(&title);
            toast.set_priority(adw::ToastPriority::High);
            self.toast_overlay.add_toast(toast);
        }
        let failed = report.failed_tracks + report.conflict_tracks + report.unavailable_tracks;
        if failed > 0 {
            let toast = crate::ui::toasts::plain(&crate::ui::strings::doctor_write_failures(
                report.updated_tracks,
                failed,
            ));
            toast.set_priority(adw::ToastPriority::High);
            toast.set_button_label(Some(&crate::ui::strings::text(
                crate::ui::strings::DOCTOR_DETAILS,
            )));
            let weak = Rc::downgrade(self);
            toast.connect_button_clicked(move |_| {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.open_review_page();
                }
            });
            self.toast_overlay.add_toast(toast);
        }
    }
}

fn combined_cleanup_report(cleanup: DoctorCleanupReport) -> DoctorWriteReport {
    let job_id = cleanup
        .reports
        .first()
        .map(|report| report.job_id)
        .unwrap_or_default();
    let cancelled_tracks = cleanup
        .reports
        .iter()
        .map(|report| report.cancelled_tracks)
        .sum();
    let rows = cleanup
        .reports
        .into_iter()
        .flat_map(|report| report.rows)
        .collect();
    DoctorWriteReport {
        job_id,
        source_job_id: None,
        updated_tracks: cleanup.reverted_tracks,
        cancelled_tracks,
        failed_tracks: cleanup.failed_tracks,
        conflict_tracks: cleanup.conflict_tracks,
        unavailable_tracks: cleanup.unavailable_tracks,
        rows,
    }
}
