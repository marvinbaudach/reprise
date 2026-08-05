use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library_doctor::{
    DoctorReviewFilter, DoctorReviewSession, DoctorScan, DoctorWriteReport, DoctorWriteRowState,
};

use super::jobs::run_auto_apply;
use super::progress_card::DoctorJobKind;
use super::LibraryDoctorCoordinator;

impl LibraryDoctorCoordinator {
    pub(super) fn start_auto_apply(self: &Rc<Self>, scan: DoctorScan) {
        let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::AutoApply);
        let total = session.freeze_plan().track_count();
        if total == 0 {
            self.page.complete_auto_apply(None);
            self.sidebar.refresh("Library Doctor scan completed");
            return;
        }
        let Some(tag_write_lease) = self.tag_write_gate.try_acquire() else {
            crate::ui::toasts::show(
                &self.toast_overlay,
                &crate::ui::strings::text(crate::ui::strings::TAG_WRITE_BUSY),
            );
            self.sidebar
                .refresh("Library Doctor automatic fixes are waiting");
            self.page.fail_auto_apply();
            return;
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        self.cancellation.borrow_mut().replace(cancellation.clone());
        self.running.set(true);
        self.job_kind.set(Some(DoctorJobKind::Apply));
        self.page.set_running(true);
        self.progress.show(DoctorJobKind::Apply, 0, total);
        self.scan_controls.button.set_sensitive(false);
        let db_path = self.db_path.clone();
        let spawned = super::super::one_shot_task::spawn_with_progress(
            "reprise-library-doctor-auto-apply",
            move |publish| {
                let _tag_write_lease = tag_write_lease;
                run_auto_apply(&db_path, &scan, &cancellation, publish)
            },
        );
        let (progress, result) = match spawned {
            Ok(channels) => channels,
            Err(error) => {
                self.finish_write_job();
                self.page.fail_auto_apply();
                tracing::error!(%error, "could not start Library Doctor automatic apply worker");
                crate::ui::toasts::show(&self.toast_overlay, &error.to_string());
                return;
            }
        };
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(progress) = progress.recv().await {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.progress.show(
                        DoctorJobKind::Apply,
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
                Ok(Ok(report)) => coordinator.handle_auto_apply_report(report),
                Ok(Err(error)) => {
                    coordinator.page.fail_auto_apply();
                    tracing::error!(%error, "Library Doctor automatic apply failed");
                    crate::ui::toasts::show(&coordinator.toast_overlay, &error);
                }
                Err(error) => {
                    coordinator.page.fail_auto_apply();
                    tracing::error!(%error, "Library Doctor automatic apply worker disappeared");
                    crate::ui::toasts::show(&coordinator.toast_overlay, &error.to_string());
                }
            }
        });
    }

    fn handle_auto_apply_report(self: &Rc<Self>, report: Option<DoctorWriteReport>) {
        let changes = report
            .as_ref()
            .map(applied_change_count)
            .unwrap_or_default();
        let failed = report.as_ref().map_or(0, |report| {
            report.failed_tracks + report.conflict_tracks + report.unavailable_tracks
        });
        if let Some(report) = &report {
            self.refresh_written_paths(report);
        }
        self.page.complete_auto_apply(report);
        self.sidebar
            .refresh("Library Doctor automatic fixes completed");
        if changes > 0 && !self.doctor_page_visible() {
            let weak = Rc::downgrade(self);
            crate::ui::toasts::show_with_action(
                &self.toast_overlay,
                &crate::ui::strings::doctor_tags_fixed(changes),
                &crate::ui::strings::text(crate::ui::strings::DOCTOR_UNDO),
                move || {
                    if let Some(coordinator) = weak.upgrade() {
                        coordinator.start_revert();
                    }
                },
            );
        }
        if failed > 0 {
            crate::ui::toasts::show(
                &self.toast_overlay,
                &crate::ui::strings::doctor_write_failures(changes, failed),
            );
        }
    }

    fn doctor_page_visible(&self) -> bool {
        self.navigation
            .visible_page_tag()
            .is_some_and(|tag| tag.starts_with("library-doctor"))
    }
}

fn applied_change_count(report: &DoctorWriteReport) -> usize {
    report
        .rows
        .iter()
        .filter(|row| row.state == DoctorWriteRowState::Applied)
        .count()
}
