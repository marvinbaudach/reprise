mod jobs;
mod progress_card;
mod review_model;
mod review_page;
mod review_row;
mod summary_page;

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
use jobs::{run_apply, run_revert, run_scan};
use progress_card::{DoctorJobKind, DoctorProgressCard};
use review_page::LibraryDoctorReviewPage;
use summary_page::LibraryDoctorPage;

const PLUGIN_TARGETS: &[&str] = &["library_doctor"];

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
    review: RefCell<Option<Rc<LibraryDoctorReviewPage>>>,
    job_kind: Cell<Option<DoctorJobKind>>,
    progress: DoctorProgressCard,
    toast_overlay: adw::ToastOverlay,
    tag_write_gate: crate::ui::tag_write_gate::TagWriteGate,
    refresh_views: Rc<dyn Fn()>,
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
                review: RefCell::new(None),
                job_kind: Cell::new(None),
                progress,
                toast_overlay: toast_overlay.clone(),
                tag_write_gate: track_list.tag_write_gate(),
                refresh_views,
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
        coordinator
    }

    pub(in crate::ui) fn open(&self) {
        if self.running.get() {
            self.open_running_job();
            return;
        }
        let enabled = {
            let conn = self.conn.borrow();
            reprise_core::modules::is_enabled(&conn, &reprise_core::modules::LIBRARY_DOCTOR_MODULE)
                .unwrap_or(false)
        };
        if !enabled {
            if let Some(preferences) = self.preferences.upgrade() {
                preferences.present_plugins(PLUGIN_TARGETS);
            }
            return;
        }
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
        let request = DoctorScanRequest {
            scope: scope_request(
                self.page.selected_scope(),
                current_view_snapshot(&self.track_list),
                super::track_list_context_menu::current_selection_ids(&self.track_list.shared),
            ),
            options: reprise_core::library_doctor::DoctorScanOptions {
                remote_enabled: self.page.remote_active(),
            },
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        self.cancellation.borrow_mut().replace(cancellation.clone());
        self.running.set(true);
        if let Some(preferences) = self.preferences.upgrade() {
            preferences.set_library_doctor_job_running(true);
        }
        self.job_kind.set(Some(DoctorJobKind::Scan));
        self.page.set_running(true);
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
            Some(DoctorJobKind::Scan | DoctorJobKind::Revert) | None => self.open_root_page(),
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
                    crate::ui::toasts::show(&coordinator.toast_overlay, &error);
                }
                Err(error) => {
                    tracing::error!(%error, "Library Doctor write worker disappeared");
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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    use reprise_core::fingerprint::{
        FingerprintBackend, FingerprintCapability, FingerprintControl, FingerprintError,
        FingerprintOutcome, FingerprintProgress,
    };
    use reprise_core::library_doctor::{DoctorScopeRequest, DoctorViewSnapshot};
    use reprise_core::queries::BrowseFilter;
    use reprise_core::view_source::ViewSource;

    struct NeverFingerprint;

    impl FingerprintBackend for NeverFingerprint {
        fn capability(&self) -> FingerprintCapability {
            FingerprintCapability::MissingPlugin {
                elements: vec!["chromaprint".into()],
            }
        }

        fn fingerprint(
            &self,
            _path: &Path,
            _progress: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
        ) -> Result<FingerprintOutcome, FingerprintError> {
            panic!("a local-only empty scan must not fingerprint")
        }
    }

    fn snapshot() -> DoctorViewSnapshot {
        DoctorViewSnapshot {
            source: ViewSource::Library,
            sort_field: "artist".into(),
            sort_dir: "asc".into(),
            filter: String::new(),
            browse: BrowseFilter::default(),
            queue_ids: Vec::new(),
        }
    }

    #[test]
    fn doc_2a_scope_choice_freezes_the_requested_input_shape() {
        assert!(matches!(
            super::scope_request(0, snapshot(), vec![7]),
            DoctorScopeRequest::WholeLibrary
        ));
        assert!(matches!(
            super::scope_request(1, snapshot(), vec![7]),
            DoctorScopeRequest::CurrentView(_)
        ));
        assert_eq!(
            super::scope_request(2, snapshot(), vec![7, 8]),
            DoctorScopeRequest::Selection {
                track_ids: vec![7, 8]
            }
        );
    }

    #[test]
    fn doctor_worker_uses_only_its_isolated_database_connection() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("doctor.db");
        drop(reprise_core::db::open_migrated(Some(&database)).unwrap());
        let request = reprise_core::library_doctor::DoctorScanRequest {
            scope: DoctorScopeRequest::WholeLibrary,
            options: reprise_core::library_doctor::DoctorScanOptions::local_only(),
        };
        let mut progress = Vec::new();

        let outcome = super::run_scan(
            &database,
            &request,
            &NeverFingerprint,
            &AtomicBool::new(false),
            &mut |item| progress.push(item),
        )
        .unwrap();

        let reprise_core::library_doctor::DoctorScanOutcome::Completed(scan) = outcome else {
            panic!("empty local scan must complete")
        };
        assert_eq!(scan.checked_tracks, 0);
        assert!(progress.is_empty());
    }
}
