mod job_page;
mod jobs;
mod progress_card;
mod review_model;
mod review_page;
mod review_row;
mod summary_page;
#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::fingerprint::{FingerprintBackend, FingerprintCapability};
use reprise_core::library_doctor::{
    DoctorApplyPlan, DoctorScanOutcome, DoctorScanRequest, DoctorScopeRequest, DoctorViewSnapshot,
    DoctorWriteProgress, DoctorWriteReport, LibraryDoctor,
};
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use super::preferences::PreferencesContext;
use super::scan_flow::ScanControls;
use super::sidebar::Sidebar;
use super::track_list::TrackList;
use job_page::LibraryDoctorJobPage;
use jobs::{run_apply, run_revert, run_scan};
use progress_card::{DoctorJobKind, DoctorProgressCard};
use review_page::LibraryDoctorReviewPage;
use summary_page::LibraryDoctorPage;

pub(in crate::ui) struct LibraryDoctorCoordinator {
    conn: Rc<RefCell<Connection>>,
    db_path: PathBuf,
    navigation: adw::NavigationView,
    window: adw::ApplicationWindow,
    page: Rc<LibraryDoctorPage>,
    track_list: Rc<TrackList>,
    scan_controls: ScanControls,
    fingerprint: Arc<dyn FingerprintBackend>,
    preferences: std::rc::Weak<PreferencesContext>,
    cancellation: RefCell<Option<Arc<AtomicBool>>>,
    running: Cell<bool>,
    scan_generation: Cell<u64>,
    review: RefCell<Option<Rc<LibraryDoctorReviewPage>>>,
    job_kind: Cell<Option<DoctorJobKind>>,
    progress: DoctorProgressCard,
    toast_overlay: adw::ToastOverlay,
    tag_write_gate: crate::ui::tag_write_gate::TagWriteGate,
    refresh_views: Rc<dyn Fn()>,
    job_page: LibraryDoctorJobPage,
    selection_override: RefCell<Option<Vec<i64>>>,
}

pub(in crate::ui) struct LibraryDoctorContext<'a> {
    pub(in crate::ui) conn: &'a Rc<RefCell<Connection>>,
    pub(in crate::ui) db_path: &'a Path,
    pub(in crate::ui) navigation: &'a adw::NavigationView,
    pub(in crate::ui) window: &'a adw::ApplicationWindow,
    pub(in crate::ui) track_list: &'a Rc<TrackList>,
    pub(in crate::ui) scan_controls: &'a ScanControls,
    pub(in crate::ui) fingerprint: Arc<dyn FingerprintBackend>,
    pub(in crate::ui) preferences: &'a Rc<PreferencesContext>,
    pub(in crate::ui) sidebar: &'a Rc<Sidebar>,
    pub(in crate::ui) toast_overlay: &'a adw::ToastOverlay,
    pub(in crate::ui) refresh_views: Rc<dyn Fn()>,
}

impl LibraryDoctorCoordinator {
    pub(in crate::ui) fn new(context: LibraryDoctorContext<'_>) -> Rc<Self> {
        let LibraryDoctorContext {
            conn,
            db_path,
            navigation,
            window,
            track_list,
            scan_controls,
            fingerprint,
            preferences,
            sidebar,
            toast_overlay,
            refresh_views,
        } = context;
        let coordinator = Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let refresh = {
                let weak = weak.clone();
                Rc::new(move |visible| {
                    if let Some(coordinator) = weak.upgrade() {
                        coordinator.page.refresh();
                        if let Some(review) = coordinator.review.borrow().as_ref() {
                            review.set_remote_active(visible);
                        }
                    }
                }) as Rc<dyn Fn(bool)>
            };
            let fingerprint_available = matches!(
                fingerprint.capability(),
                FingerprintCapability::Available { .. }
            );
            let page = LibraryDoctorPage::new(conn, window, fingerprint_available, refresh);
            let progress = DoctorProgressCard::new();
            let job_page = LibraryDoctorJobPage::new();
            Self {
                conn: conn.clone(),
                db_path: db_path.to_path_buf(),
                navigation: navigation.clone(),
                window: window.clone(),
                page,
                track_list: track_list.clone(),
                scan_controls: scan_controls.clone(),
                fingerprint,
                preferences: Rc::downgrade(preferences),
                cancellation: RefCell::new(None),
                running: Cell::new(false),
                scan_generation: Cell::new(0),
                review: RefCell::new(None),
                job_kind: Cell::new(None),
                progress,
                toast_overlay: toast_overlay.clone(),
                tag_write_gate: track_list.tag_write_gate(),
                refresh_views,
                job_page,
                selection_override: RefCell::new(None),
            }
        });
        sidebar.append_doctor_card(coordinator.progress.widget());
        coordinator.load_last_scan();
        {
            let weak = Rc::downgrade(&coordinator);
            coordinator.page.connect_run(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.start_scan();
                }
            });
        }
        {
            let weak = Rc::downgrade(&coordinator);
            coordinator.progress.set_on_cancel(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.request_cancel();
                }
            });
        }
        {
            let weak = Rc::downgrade(&coordinator);
            coordinator.progress.set_on_activate(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.open_running_job();
                }
            });
        }
        {
            let weak = Rc::downgrade(&coordinator);
            coordinator.page.connect_review_all(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator
                        .open_review(reprise_core::library_doctor::DoctorReviewFilter::AllChanges);
                }
            });
        }
        {
            let weak = Rc::downgrade(&coordinator);
            coordinator.page.connect_review_safe(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.open_review(
                        reprise_core::library_doctor::DoctorReviewFilter::LocalSafeOnly,
                    );
                }
            });
        }
        coordinator.install_tag_edit_observer();
        {
            let run = Rc::downgrade(&coordinator);
            let revert = Rc::downgrade(&coordinator);
            preferences.doctor_controls.set_callbacks(
                move |scope| {
                    if let Some(coordinator) = run.upgrade() {
                        coordinator.run_from_preferences(scope);
                    }
                },
                move || {
                    if let Some(coordinator) = revert.upgrade() {
                        coordinator.start_revert();
                    }
                },
            );
        }
        coordinator
    }

    pub(in crate::ui) fn open(&self) {
        if self.running.get() {
            self.open_running_job();
            return;
        }
        self.selection_override.borrow_mut().take();
        let view = current_view_snapshot(&self.track_list);
        self.page.set_selected_scope(suggested_scope(&view));
        self.open_available();
    }

    pub(in crate::ui) fn open_for_selection(&self, ids: Vec<i64>) {
        if self.running.get() {
            self.open_running_job();
            return;
        }
        self.selection_override.borrow_mut().replace(ids);
        self.page.set_selected_scope(2);
        self.open_available();
    }

    fn open_available(&self) {
        {
            let conn = self.conn.borrow();
            self.page.sync_remote_preference(&conn);
        }
        self.open_root_page();
    }

    fn open_root_page(&self) {
        if let Some(page) = self.navigation.find_page("library-doctor") {
            self.navigation.pop_to_page(&page);
        } else {
            self.navigation.push(self.page.navigation_page());
        }
    }

    fn run_from_preferences(self: &Rc<Self>, scope: u32) {
        if scope != 2 {
            self.selection_override.borrow_mut().take();
        }
        self.page.set_selected_scope(scope);
        self.open_available();
        self.start_scan();
    }

    fn load_last_scan(&self) {
        let scan = {
            let mut conn = self.conn.borrow_mut();
            LibraryDoctor::new(&mut conn)
                .last_complete_scan()
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "could not load the last Library Doctor result");
                    None
                })
        };
        self.page.set_scan(scan);
    }

    fn start_scan(self: &Rc<Self>) {
        if self.running.get() || self.scan_controls.is_scanning() {
            return;
        }
        let scope = self.page.selected_scope();
        let selection = if scope == 2 {
            self.selection_override
                .borrow_mut()
                .take()
                .unwrap_or_else(|| {
                    super::track_list_context_menu::current_selection_ids(&self.track_list.shared)
                })
        } else {
            self.selection_override.borrow_mut().take();
            Vec::new()
        };
        let request = DoctorScanRequest {
            scope: scope_request(scope, current_view_snapshot(&self.track_list), selection),
            options: reprise_core::library_doctor::DoctorScanOptions {
                remote_enabled: self.page.remote_active(),
            },
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        let generation = self.scan_generation.get().wrapping_add(1);
        self.scan_generation.set(generation);
        self.cancellation.borrow_mut().replace(cancellation.clone());
        self.running.set(true);
        if let Some(preferences) = self.preferences.upgrade() {
            preferences.set_library_doctor_job_running(true);
        }
        self.job_kind.set(Some(DoctorJobKind::Scan));
        self.page.set_running(true);
        self.page.begin_partial_scan();
        self.progress.show(DoctorJobKind::Scan, 0, 0);
        self.scan_controls.button.set_sensitive(false);
        let db_path = self.db_path.clone();
        let fingerprint = self.fingerprint.clone();
        let spawned = super::one_shot_task::spawn_with_progress(
            "reprise-library-doctor-scan",
            move |publish| {
                run_scan(
                    &db_path,
                    &request,
                    fingerprint.as_ref(),
                    &cancellation,
                    publish,
                )
            },
        );
        let (progress, result) = match spawned {
            Ok(channels) => channels,
            Err(error) => {
                self.finish_scan();
                tracing::error!(%error, "could not start Library Doctor scan worker");
                self.track_list.toast(&error.to_string());
                return;
            }
        };
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(progress) = progress.recv().await {
                if let Some(coordinator) = weak.upgrade() {
                    if !accepts_scan_progress(
                        coordinator.scan_generation.get(),
                        generation,
                        coordinator.running.get(),
                        coordinator.job_kind.get(),
                    ) {
                        break;
                    }
                    coordinator.page.set_partial_summary(progress.summary);
                    coordinator.progress.show(
                        DoctorJobKind::Scan,
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
            coordinator.finish_scan();
            match received {
                Ok(Ok(DoctorScanOutcome::Completed(scan))) => {
                    coordinator.review.borrow_mut().take();
                    coordinator.page.set_scan(Some(scan));
                }
                Ok(Ok(DoctorScanOutcome::Cancelled { .. })) => {}
                Ok(Ok(DoctorScanOutcome::ScopeFallbackRequired)) => {
                    coordinator.page.set_selected_scope(0);
                    coordinator.track_list.toast(&crate::ui::strings::text(
                        crate::ui::strings::DOCTOR_SCOPE_FALLBACK,
                    ));
                    coordinator.start_scan();
                }
                Ok(Err(error)) => {
                    tracing::error!(%error, "Library Doctor scan failed");
                    coordinator.track_list.toast(&error);
                }
                Err(error) => {
                    tracing::error!(%error, "Library Doctor scan worker disappeared");
                    coordinator.track_list.toast(&error.to_string());
                }
            }
        });
    }

    fn request_cancel(&self) {
        if let Some(cancellation) = self.cancellation.borrow().as_ref() {
            cancellation.store(true, Ordering::Relaxed);
        }
    }

    fn finish_scan(&self) {
        self.cancellation.borrow_mut().take();
        self.running.set(false);
        if let Some(preferences) = self.preferences.upgrade() {
            preferences.set_library_doctor_job_running(false);
        }
        self.job_kind.set(None);
        self.page.set_running(false);
        self.page.clear_partial_scan();
        self.progress.hide();
        self.scan_controls.button.set_sensitive(true);
    }

    fn open_review(self: &Rc<Self>, filter: reprise_core::library_doctor::DoctorReviewFilter) {
        let Some(scan) = self.page.scan() else {
            return;
        };
        let existing = self.review.borrow().clone();
        if existing.filter(|page| page.filter() == filter).is_none() {
            let weak = Rc::downgrade(self);
            let track_list = self.track_list.clone();
            let on_edit = Rc::new(move |track_id| track_list.edit_tags_for_ids(&[track_id]))
                as Rc<dyn Fn(i64)>;
            let page = LibraryDoctorReviewPage::new(
                &self.conn,
                &self.window,
                scan,
                filter,
                Rc::new(move |_| {
                    if let Some(coordinator) = weak.upgrade() {
                        let conn = coordinator.conn.borrow();
                        coordinator.page.sync_remote_preference(&conn);
                    }
                }),
                &on_edit,
            );
            {
                let weak = Rc::downgrade(self);
                page.connect_apply(move |plan| {
                    if let Some(coordinator) = weak.upgrade() {
                        coordinator.start_apply(plan);
                    }
                });
            }
            self.review.borrow_mut().replace(page.clone());
        }
        self.open_review_page();
    }

    fn install_tag_edit_observer(self: &Rc<Self>) {
        let prior = self.track_list.shared.on_tags_mutated.borrow().clone();
        let weak = Rc::downgrade(self);
        self.track_list.set_on_tags_mutated(move |paths| {
            if let Some(prior) = &prior {
                prior(paths);
            }
            if let Some(coordinator) = weak.upgrade() {
                if let Some(review) = coordinator.review.borrow().as_ref() {
                    review.mark_paths_stale(paths);
                }
            }
        });
    }

    fn open_running_job(&self) {
        match self.job_kind.get() {
            Some(DoctorJobKind::Apply) => self.open_review_page(),
            Some(DoctorJobKind::Revert) => self.open_job_page(),
            Some(DoctorJobKind::Scan) | None => self.open_root_page(),
        }
    }

    fn open_job_page(&self) {
        if self.navigation.find_page("library-doctor-job").is_some() {
            self.navigation.pop_to_tag("library-doctor-job");
        } else {
            self.navigation.push(self.job_page.navigation_page());
        }
    }

    fn open_review_page(&self) {
        let Some(review) = self.review.borrow().as_ref().cloned() else {
            self.open_root_page();
            return;
        };
        if self.navigation.find_page("library-doctor-review").is_some() {
            self.navigation.pop_to_tag("library-doctor-review");
        } else {
            self.navigation.push(review.navigation_page());
        }
    }

    fn start_apply(self: &Rc<Self>, plan: DoctorApplyPlan) {
        if plan.track_count() == 0 || self.running.get() || self.scan_controls.is_scanning() {
            return;
        }
        let Some(tag_write_lease) = self.tag_write_gate.try_acquire() else {
            crate::ui::toasts::show(
                &self.toast_overlay,
                &crate::ui::strings::text(crate::ui::strings::TAG_WRITE_BUSY),
            );
            return;
        };
        let total = plan.track_count();
        let cancellation = Arc::new(AtomicBool::new(false));
        self.cancellation.borrow_mut().replace(cancellation.clone());
        self.running.set(true);
        if let Some(preferences) = self.preferences.upgrade() {
            preferences.set_library_doctor_job_running(true);
        }
        self.job_kind.set(Some(DoctorJobKind::Apply));
        self.page.set_running(true);
        if let Some(review) = self.review.borrow().as_ref() {
            review.set_running(true);
        }
        self.scan_controls.button.set_sensitive(false);
        self.progress.show(DoctorJobKind::Apply, 0, total);
        let db_path = self.db_path.clone();
        let spawned = super::one_shot_task::spawn_with_progress(
            "reprise-library-doctor-apply",
            move |publish| {
                let _tag_write_lease = tag_write_lease;
                run_apply(&db_path, &plan, &cancellation, publish)
            },
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

    fn start_revert(self: &Rc<Self>) {
        if self.running.get() || self.scan_controls.is_scanning() {
            return;
        }
        let total = {
            let mut conn = self.conn.borrow_mut();
            LibraryDoctor::new(&mut conn)
                .last_cleanup()
                .ok()
                .flatten()
                .map(|cleanup| cleanup.track_count)
        };
        let Some(total) = total else {
            return;
        };
        let Some(tag_write_lease) = self.tag_write_gate.try_acquire() else {
            crate::ui::toasts::show(
                &self.toast_overlay,
                &crate::ui::strings::text(crate::ui::strings::TAG_WRITE_BUSY),
            );
            return;
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        self.cancellation.borrow_mut().replace(cancellation.clone());
        self.running.set(true);
        if let Some(preferences) = self.preferences.upgrade() {
            preferences.set_library_doctor_job_running(true);
        }
        self.job_kind.set(Some(DoctorJobKind::Revert));
        self.job_page.set_running(DoctorJobKind::Revert);
        self.open_job_page();
        self.page.set_running(true);
        if let Some(review) = self.review.borrow().as_ref() {
            review.set_running(true);
        }
        self.scan_controls.button.set_sensitive(false);
        self.progress.show(DoctorJobKind::Revert, 0, total);
        let db_path = self.db_path.clone();
        let spawned = super::one_shot_task::spawn_with_progress(
            "reprise-library-doctor-revert",
            move |publish| {
                let _tag_write_lease = tag_write_lease;
                run_revert(&db_path, &cancellation, publish)
            },
        );
        let (progress, result) = match spawned {
            Ok(channels) => channels,
            Err(error) => {
                self.finish_write_job();
                tracing::error!(%error, "could not start Library Doctor revert worker");
                self.job_page.set_error(&error.to_string());
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
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    tracing::error!(%error, "Library Doctor write failed");
                    if kind == DoctorJobKind::Revert {
                        coordinator.job_page.set_error(&error);
                    }
                    crate::ui::toasts::show(&coordinator.toast_overlay, &error);
                }
                Err(error) => {
                    tracing::error!(%error, "Library Doctor write worker disappeared");
                    if kind == DoctorJobKind::Revert {
                        coordinator.job_page.set_error(&error.to_string());
                    }
                    crate::ui::toasts::show(&coordinator.toast_overlay, &error.to_string());
                }
            }
        });
    }

    fn finish_write_job(&self) {
        self.cancellation.borrow_mut().take();
        self.running.set(false);
        if let Some(preferences) = self.preferences.upgrade() {
            preferences.set_library_doctor_job_running(false);
        }
        self.job_kind.set(None);
        self.page.set_running(false);
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
        if let Some(review) = self.review.borrow().as_ref() {
            review.set_write_report(report);
        }
        self.page
            .set_write_report(report, kind == DoctorJobKind::Revert);
        if kind == DoctorJobKind::Revert {
            let remaining = report.cancelled_tracks
                + report.failed_tracks
                + report.conflict_tracks
                + report.unavailable_tracks;
            self.job_page
                .set_result(report.updated_tracks, remaining, true);
        }
        self.show_write_toasts(kind, report);
    }

    fn show_write_toasts(self: &Rc<Self>, kind: DoctorJobKind, report: &DoctorWriteReport) {
        if report.updated_tracks > 0 {
            let title = if report.cancelled_tracks > 0 {
                crate::ui::strings::doctor_write_cancelled(
                    report.updated_tracks,
                    report.cancelled_tracks,
                )
            } else if kind == DoctorJobKind::Revert {
                crate::ui::strings::doctor_tags_reverted(report.updated_tracks)
            } else {
                crate::ui::strings::doctor_tags_updated(report.updated_tracks)
            };
            let toast = adw::Toast::new(&title);
            toast.set_priority(adw::ToastPriority::High);
            if kind == DoctorJobKind::Apply {
                toast.set_button_label(Some(&crate::ui::strings::text(
                    crate::ui::strings::DOCTOR_REVERT,
                )));
                let weak = Rc::downgrade(self);
                toast.connect_button_clicked(move |_| {
                    if let Some(coordinator) = weak.upgrade() {
                        coordinator.start_revert();
                    }
                });
            }
            self.toast_overlay.add_toast(toast);
        }
        let failed = report.failed_tracks + report.conflict_tracks + report.unavailable_tracks;
        if failed > 0 {
            let toast = adw::Toast::new(&crate::ui::strings::doctor_write_failures(
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

fn accepts_scan_progress(
    current_generation: u64,
    expected_generation: u64,
    running: bool,
    job_kind: Option<DoctorJobKind>,
) -> bool {
    current_generation == expected_generation && running && job_kind == Some(DoctorJobKind::Scan)
}

fn current_view_snapshot(track_list: &TrackList) -> DoctorViewSnapshot {
    let shared = &track_list.shared;
    let source = shared.source.borrow().clone();
    let sort = shared.sort.borrow().clone();
    let queue_ids = if source == ViewSource::Queue {
        shared.current_view_ids()
    } else {
        Vec::new()
    };
    DoctorViewSnapshot {
        source,
        sort_field: sort.field,
        sort_dir: sort.dir,
        filter: shared.filter.borrow().clone(),
        browse: shared.browse_filter.borrow().clone(),
        queue_ids,
    }
}

fn suggested_scope(view: &DoctorViewSnapshot) -> u32 {
    if view.filter.is_empty() && view.browse.is_empty() {
        0
    } else {
        1
    }
}

fn scope_request(
    choice: u32,
    current_view: DoctorViewSnapshot,
    selection: Vec<i64>,
) -> DoctorScopeRequest {
    match choice {
        1 => DoctorScopeRequest::CurrentView(Box::new(current_view)),
        2 => DoctorScopeRequest::Selection {
            track_ids: selection,
        },
        _ => DoctorScopeRequest::WholeLibrary,
    }
}
