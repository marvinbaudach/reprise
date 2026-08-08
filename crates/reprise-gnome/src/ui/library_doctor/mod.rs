mod auto_apply;
mod jobs;
#[cfg(test)]
mod jobs_tests;
mod progress_card;
pub(in crate::ui) mod remote_toggle;
mod result_pages;
mod review_conflicts;
mod review_filter_bar;
mod review_header;
mod review_model;
mod review_page;
mod review_row;
mod running_page;
mod start_page;
mod summary_cards;
mod summary_model;
mod summary_page;
#[cfg(test)]
mod summary_page_tests;
#[cfg(test)]
mod tests;
mod write_jobs;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::fingerprint::{FingerprintBackend, FingerprintCapability};
use reprise_core::library_doctor::{
    DoctorScanOutcome, DoctorScanRequest, DoctorScopeRequest, DoctorViewSnapshot, LibraryDoctor,
};
use reprise_core::view_source::ViewSource;

use super::scan_flow::ScanControls;
use super::sidebar::Sidebar;
use super::track_list::TrackList;
use jobs::run_scan;
use progress_card::{DoctorJobKind, DoctorProgressCard};
use review_page::LibraryDoctorReviewPage;
use summary_page::LibraryDoctorPage;

/// The glyph that stands for the Library Doctor. The sidebar's
/// `NavIcon::LibraryDoctor` uses the same theme icon, so the result card and the
/// `ISSUES` entry that leads to it are recognisably the same thing. No
/// stethoscope symbolic ships with the icon theme or with this app, and the
/// design's stethoscope is not worth a bundled asset for two 20px slots.
pub(in crate::ui) const DOCTOR_GLYPH: &str = "system-search-symbolic";

pub(in crate::ui) const DOCTOR_DONE_GLYPH: &str = crate::ui::icons::DONE;

pub(in crate::ui) fn css() -> String {
    [
        ".doctor-conflicts-dashed { border: 1px dashed @borders; border-radius: 12px; padding: 12px; }",
        // The review card is the only one that carries emphasis: an accent
        // hairline on top of the plain `.card` surface.
        ".doctor-card-accent { box-shadow: inset 0 0 0 1px alpha(@accent_color, 0.45); }",
        // The conflicts card is the quietest thing on the page: an outline, no
        // fill, no shadow.
        ".doctor-card-dashed { border: 1px dashed alpha(@borders, 0.9); border-radius: 12px; }",
    ]
    .join(" ")
}

pub(in crate::ui) struct LibraryDoctorCoordinator {
    conn: Rc<Db>,
    db_path: PathBuf,
    navigation: adw::NavigationView,
    window: adw::ApplicationWindow,
    page: Rc<LibraryDoctorPage>,
    track_list: Rc<TrackList>,
    scan_controls: ScanControls,
    fingerprint: Arc<dyn FingerprintBackend>,
    cancellation: RefCell<Option<Arc<AtomicBool>>>,
    running: Cell<bool>,
    scan_generation: Cell<u64>,
    review: RefCell<Option<Rc<LibraryDoctorReviewPage>>>,
    job_kind: Cell<Option<DoctorJobKind>>,
    progress: DoctorProgressCard,
    toast_overlay: adw::ToastOverlay,
    tag_write_gate: crate::ui::tag_write_gate::TagWriteGate,
    refresh_views: Rc<dyn Fn()>,
    sidebar: Rc<Sidebar>,
    selection_override: RefCell<Option<Vec<i64>>>,
}

pub(in crate::ui) struct LibraryDoctorContext<'a> {
    pub(in crate::ui) conn: &'a Rc<Db>,
    pub(in crate::ui) db_path: &'a Path,
    pub(in crate::ui) navigation: &'a adw::NavigationView,
    pub(in crate::ui) window: &'a adw::ApplicationWindow,
    pub(in crate::ui) track_list: &'a Rc<TrackList>,
    pub(in crate::ui) scan_controls: &'a ScanControls,
    pub(in crate::ui) fingerprint: Arc<dyn FingerprintBackend>,
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
            sidebar,
            toast_overlay,
            refresh_views,
        } = context;
        let coordinator = Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let refresh = {
                let weak = weak.clone();
                Rc::new(move |visible| {
                    if let Some(coordinator) = weak.upgrade() {
                        coordinator.page.refresh_remote_availability();
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
                cancellation: RefCell::new(None),
                running: Cell::new(false),
                scan_generation: Cell::new(0),
                review: RefCell::new(None),
                job_kind: Cell::new(None),
                progress,
                toast_overlay: toast_overlay.clone(),
                tag_write_gate: track_list.tag_write_gate(),
                refresh_views,
                sidebar: sidebar.clone(),
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
            coordinator.page.connect_cancel(move || {
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
            coordinator.page.connect_review(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.open_review();
                }
            });
        }
        {
            let weak = Rc::downgrade(&coordinator);
            coordinator.page.connect_start_revert(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.start_revert();
                }
            });
        }
        {
            let weak = Rc::downgrade(&coordinator);
            coordinator.page.connect_summary_undo(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.start_revert();
                }
            });
        }
        {
            let weak = Rc::downgrade(&coordinator);
            coordinator.page.connect_result_undo(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.start_revert();
                }
            });
        }
        {
            let weak = Rc::downgrade(&coordinator);
            coordinator.page.connect_scan_again(Rc::new(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.page.show_start(&coordinator.conn);
                }
            }));
        }
        {
            let weak = Rc::downgrade(&coordinator);
            coordinator.page.connect_done(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.acknowledge_scan();
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
        self.selection_override.borrow_mut().take();
        let view = current_view_snapshot(&self.track_list);
        self.page.set_selected_scope(suggested_scope(&view));
        self.open_available();
    }

    /// Opens the findings themselves, for the sidebar's `ISSUES` entry.
    ///
    /// That row exists only while a completed scan has changes nobody has
    /// looked at, so it is a pointer to those changes — not to the page that
    /// summarises them. The ⋮ menu keeps the other door.
    pub(in crate::ui) fn open_findings(self: &Rc<Self>) {
        if self.running.get() {
            self.open_running_job();
            return;
        }
        self.selection_override.borrow_mut().take();
        self.page.sync_remote_preference(&self.conn);
        self.open_review();
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
        self.page.sync_remote_preference(&self.conn);
        self.open_root_page();
    }

    fn open_root_page(&self) {
        if let Some(page) = self.navigation.find_page("library-doctor") {
            self.navigation.pop_to_page(&page);
        } else {
            self.navigation.push(self.page.navigation_page());
        }
    }

    pub(super) fn load_last_scan(&self) {
        let scan = {
            let conn = &self.conn;
            LibraryDoctor::new(conn)
                .last_complete_scan()
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "could not load the last Library Doctor result");
                    None
                })
        };
        self.page.set_scan(scan, self.revert_available());
    }

    /// `Undo` is only offered while there is a cleanup left to undo. Without
    /// this the button would be sensitive after a revert and do nothing when
    /// clicked, because `start_revert` bails out on an empty cleanup.
    fn revert_available(&self) -> bool {
        let conn = &self.conn;
        LibraryDoctor::new(conn)
            .last_cleanup()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not read the last Library Doctor cleanup");
                None
            })
            .is_some_and(|cleanup| cleanup.track_count > 0)
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
        self.job_kind.set(Some(DoctorJobKind::Scan));
        self.page.set_controls_locked(true);
        self.page.begin_job(DoctorJobKind::Scan, 0);
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
                    coordinator.page.set_live_summary(progress.summary);
                    coordinator.page.update_job(
                        DoctorJobKind::Scan,
                        progress.completed_tracks,
                        progress.total_tracks,
                    );
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
                    // No `end_job` here: `start_auto_apply` keeps the page on
                    // the running screen until the quiet write is done, so the
                    // summary never appears before the fixes it reports.
                    coordinator.start_auto_apply(scan);
                }
                Ok(Ok(DoctorScanOutcome::Cancelled { .. })) => coordinator.page.end_job(),
                Ok(Ok(DoctorScanOutcome::ScopeFallbackRequired)) => {
                    coordinator.page.set_selected_scope(0);
                    coordinator.track_list.toast(&crate::ui::strings::text(
                        crate::ui::strings::DOCTOR_SCOPE_FALLBACK,
                    ));
                    coordinator.start_scan();
                }
                Ok(Err(error)) => {
                    tracing::error!(%error, "Library Doctor scan failed");
                    coordinator.page.end_job();
                    coordinator.track_list.toast(&error);
                }
                Err(error) => {
                    tracing::error!(%error, "Library Doctor scan worker disappeared");
                    coordinator.page.end_job();
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

    /// Coordinator bookkeeping only. Which screen the page lands on is decided
    /// by the branch that knows what the scan produced — see `start_scan`.
    fn finish_scan(&self) {
        self.cancellation.borrow_mut().take();
        self.running.set(false);
        self.job_kind.set(None);
        self.page.set_controls_locked(false);
        self.progress.hide();
        self.scan_controls.button.set_sensitive(true);
    }

    fn acknowledge_scan(&self) {
        let Some(scan) = self.page.scan() else {
            return;
        };
        if let Err(error) = LibraryDoctor::new(&self.conn).set_reviewed_scan(scan.id) {
            tracing::error!(%error, scan_id = scan.id, "failed to acknowledge Library Doctor scan");
            crate::ui::toasts::show(&self.toast_overlay, &error.to_string());
            return;
        }
        self.sidebar.refresh("Library Doctor scan acknowledged");
        self.page.show_start(&self.conn);
    }

    fn open_review(self: &Rc<Self>) {
        let Some(scan) = self.page.scan() else {
            return;
        };
        let existing = self.review.borrow().clone();
        if existing.is_none() {
            let remote_weak = Rc::downgrade(self);
            let reviewed_weak = Rc::downgrade(self);
            let track_list = self.track_list.clone();
            let on_edit = Rc::new(move |track_ids: &[i64]| track_list.edit_tags_for_ids(track_ids))
                as Rc<dyn Fn(&[i64])>;
            let page = LibraryDoctorReviewPage::new(
                &self.conn,
                &self.window,
                &scan,
                Rc::new(move |_| {
                    if let Some(coordinator) = remote_weak.upgrade() {
                        coordinator.page.sync_remote_preference(&coordinator.conn);
                    }
                }),
                Rc::new(move || {
                    if let Some(coordinator) = reviewed_weak.upgrade() {
                        coordinator
                            .sidebar
                            .refresh("Library Doctor scan acknowledged");
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
            Some(DoctorJobKind::Revert | DoctorJobKind::Scan) | None => self.open_root_page(),
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
