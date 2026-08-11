mod auto_apply;
mod jobs;
#[cfg(test)]
mod jobs_tests;
mod navigation;
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
mod start_page_css;
mod summary_cards;
mod summary_model;
mod summary_page;
#[cfg(test)]
mod summary_page_tests;
#[cfg(test)]
mod tests;
mod write_jobs;
use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::fingerprint::{FingerprintBackend, FingerprintCapability};
use reprise_core::library_doctor::{
    DoctorScanOutcome, DoctorScanRequest, DoctorScopeRequest, DoctorViewSnapshot, LibraryDoctor,
};
use reprise_core::view_source::ViewSource;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::scan_flow::ScanControls;
use super::sidebar::Sidebar;
use super::track_list::TrackList;
use jobs::run_scan;
use navigation::DoctorNavigation;
use progress_card::{DoctorJobKind, DoctorProgressCard};
use review_page::LibraryDoctorReviewPage;
use summary_page::LibraryDoctorPage;

/// The glyph that stands for the Library Doctor: the design's stethoscope,
/// which the app now ships itself as
/// `data/icons/hicolor/symbolic/apps/reprise-stethoscope-symbolic.svg`, drawn
/// by `scripts/build-brand-assets.sh`.
///
/// Resolved here rather than at each call site, so the start page, the result
/// card and the sidebar entry cannot end up asking for different things — they
/// did, and both the card and the sidebar kept drawing the magnifier the
/// stethoscope replaced. A theme without the app's icon directory in reach
/// falls back to that magnifier; without the guard GTK would render the
/// missing-image box instead.
///
/// The sidebar reaches the same answer through the same two names rather than
/// through `doctor_glyph`: `NavIcon` already owns a name/fallback pair and
/// `nav_icon` already performs this theme check, so the row joins that
/// mechanism instead of adding a second one.
pub(in crate::ui) const DOCTOR_GLYPH: &str = "reprise-stethoscope-symbolic";
pub(in crate::ui) const DOCTOR_GLYPH_FALLBACK: &str = "system-search-symbolic";

pub(in crate::ui) fn doctor_glyph() -> &'static str {
    gtk4::gdk::Display::default().map_or(DOCTOR_GLYPH_FALLBACK, |display| {
        doctor_glyph_for(gtk4::IconTheme::for_display(&display).has_icon(DOCTOR_GLYPH))
    })
}

pub(in crate::ui) const fn doctor_glyph_for(theme_has_stethoscope: bool) -> &'static str {
    if theme_has_stethoscope {
        DOCTOR_GLYPH
    } else {
        DOCTOR_GLYPH_FALLBACK
    }
}

pub(in crate::ui) const DOCTOR_DONE_GLYPH: &str = crate::ui::icons::DONE;

pub(in crate::ui) fn css() -> String {
    [
        ".doctor-conflicts-dashed { border: 1px dashed color-mix(in srgb, currentColor 18%, transparent); border-radius: 12px; padding: 20px 22px; background: transparent; }",
        ".doctor-conflicts-warning { color: color-mix(in srgb, currentColor 50%, transparent); }",
        ".doctor-conflicts-optional { font-size: 13px; color: color-mix(in srgb, currentColor 45%, transparent); }",
        ".doctor-conflicts-intro { font-size: 13px; color: color-mix(in srgb, currentColor 52%, transparent); }",
        ".doctor-conflict-row { padding: 12px 0; border-top: 1px solid color-mix(in srgb, currentColor 8%, transparent); }",
        ".doctor-conflict-scope { font-size: 13px; color: color-mix(in srgb, currentColor 55%, transparent); }",
        ".doctor-conflict-choice { padding: 5px 12px; border-radius: 8px; box-shadow: inset 0 0 0 1px color-mix(in srgb, currentColor 14%, transparent); }",
        ".doctor-conflict-choice.selected { color: var(--accent-color); box-shadow: inset 0 0 0 1px var(--accent-bg-color); }",
        ".doctor-album-header-later { border-top: 1px solid color-mix(in srgb, currentColor 7%, transparent); padding-top: 20px; }",
        ".doctor-album-check { min-width: 16px; min-height: 16px; border-radius: 4px; }",
        ".doctor-album-check:checked { background: var(--accent-bg-color); color: var(--window-bg-color); }",
        ".doctor-album-check:not(:checked) { box-shadow: inset 0 0 0 1.5px color-mix(in srgb, currentColor 30%, transparent); }",
        ".doctor-album-cover { background: color-mix(in srgb, currentColor 8%, transparent); border-radius: 5px; -gtk-icon-size: 16px; }",
        ".doctor-album-title { font-size: 15px; font-weight: 500; }",
        ".doctor-album-detail { font-size: 13px; color: color-mix(in srgb, currentColor 50%, transparent); }",
        ".doctor-album-caret { color: color-mix(in srgb, currentColor 40%, transparent); }",
        ".doctor-review-row { font-size: 13.5px; }",
        ".doctor-review-row-deselected { opacity: 0.55; }",
        ".doctor-album-wide-track { color: color-mix(in srgb, currentColor 45%, transparent); }",
        ".doctor-review-arrow { color: color-mix(in srgb, currentColor 32%, transparent); }",
        ".doctor-review-current { color: color-mix(in srgb, currentColor 52%, transparent); }",
        ".doctor-current-empty { color: color-mix(in srgb, currentColor 42%, transparent); }",
        ".doctor-review-source { font-size: 12.5px; color: color-mix(in srgb, currentColor 55%, transparent); }",
        ".doctor-review-source.accent { color: var(--accent-color); }",
        // The review card is the only one that carries emphasis: an accent
        // hairline on top of the plain `.card` surface.
        ".doctor-card-accent { box-shadow: inset 0 0 0 1px alpha(@accent_color, 0.45); }",
        // The conflicts card is the quietest thing on the page: an outline, no
        // fill, no shadow.
        ".doctor-card-dashed { border: 1px dashed alpha(@borders, 0.9); border-radius: 12px; }",
        &start_page_css::css(),
        ".doctor-review-header-action { font-size: 13px; padding: 5px 12px; }",
        ".doctor-review-meta { padding: 12px 28px; background: color-mix(in srgb, var(--card-bg-color) 45%, var(--window-bg-color)); }",
        ".doctor-review-meta-summary { font-size: 14px; }",
        ".doctor-review-meta-hint { font-size: 13px; color: color-mix(in srgb, currentColor 45%, transparent); }",
        ".doctor-review-footer { padding: 14px 28px; background: color-mix(in srgb, var(--card-bg-color) 55%, var(--window-bg-color)); border-top: 1px solid color-mix(in srgb, currentColor 10%, transparent); }",
        ".doctor-review-footer-summary { font-size: 13.5px; color: color-mix(in srgb, currentColor 62%, transparent); }",
        ".doctor-review-apply { font-size: 14.5px; padding: 9px 18px; }",
    ]
    .join(" ")
}

pub(in crate::ui) struct LibraryDoctorCoordinator {
    conn: Rc<Db>,
    db_path: PathBuf,
    navigation: DoctorNavigation,
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
    _doctor_chrome: Rc<crate::ui::window::library_chrome::DoctorChrome>,
}

pub(in crate::ui) struct LibraryDoctorLauncher {
    coordinator: Rc<crate::ui::startup_quiet::Deferred<LibraryDoctorCoordinator>>,
}

pub(in crate::ui) struct LibraryDoctorContext<'a> {
    pub(in crate::ui) conn: &'a Rc<Db>,
    pub(in crate::ui) db_path: &'a Path,
    pub(in crate::ui) content_navigation: &'a adw::NavigationView,
    pub(in crate::ui) content_stack: &'a gtk4::Stack,
    pub(in crate::ui) doctor_navigation: &'a adw::NavigationView,
    pub(in crate::ui) doctor_chrome: &'a Rc<crate::ui::window::library_chrome::DoctorChrome>,
    pub(in crate::ui) window: &'a adw::ApplicationWindow,
    pub(in crate::ui) track_list: &'a Rc<TrackList>,
    pub(in crate::ui) scan_controls: &'a ScanControls,
    pub(in crate::ui) fingerprint: Arc<dyn FingerprintBackend>,
    pub(in crate::ui) sidebar: &'a Rc<Sidebar>,
    pub(in crate::ui) toast_overlay: &'a adw::ToastOverlay,
    pub(in crate::ui) refresh_views: Rc<dyn Fn()>,
}

struct OwnedLibraryDoctorContext {
    conn: Rc<Db>,
    db_path: PathBuf,
    content_navigation: adw::NavigationView,
    content_stack: gtk4::Stack,
    doctor_navigation: adw::NavigationView,
    doctor_chrome: Rc<crate::ui::window::library_chrome::DoctorChrome>,
    window: adw::ApplicationWindow,
    track_list: Rc<TrackList>,
    scan_controls: ScanControls,
    fingerprint: Arc<dyn FingerprintBackend>,
    sidebar: Rc<Sidebar>,
    toast_overlay: adw::ToastOverlay,
    refresh_views: Rc<dyn Fn()>,
}

impl LibraryDoctorContext<'_> {
    fn into_owned(self) -> OwnedLibraryDoctorContext {
        OwnedLibraryDoctorContext {
            conn: self.conn.clone(),
            db_path: self.db_path.to_path_buf(),
            content_navigation: self.content_navigation.clone(),
            content_stack: self.content_stack.clone(),
            doctor_navigation: self.doctor_navigation.clone(),
            doctor_chrome: self.doctor_chrome.clone(),
            window: self.window.clone(),
            track_list: self.track_list.clone(),
            scan_controls: self.scan_controls.clone(),
            fingerprint: self.fingerprint,
            sidebar: self.sidebar.clone(),
            toast_overlay: self.toast_overlay.clone(),
            refresh_views: self.refresh_views,
        }
    }
}

impl LibraryDoctorLauncher {
    pub(in crate::ui) fn new(context: LibraryDoctorContext<'_>) -> Rc<Self> {
        let context = context.into_owned();
        let progress = DoctorProgressCard::new();
        context.sidebar.append_doctor_card(progress.widget());
        let coordinator = crate::ui::startup_quiet::deferred_after_quiet(move || {
            super::startup_report::mark("LibraryDoctorCoordinator::new begin");
            let coordinator = LibraryDoctorCoordinator::new(context, &progress);
            super::startup_report::mark("LibraryDoctorCoordinator::new end");
            coordinator
        });
        Rc::new(Self { coordinator })
    }

    pub(in crate::ui) fn open(&self) {
        self.coordinator.get().open();
    }

    pub(in crate::ui) fn open_findings(&self) {
        self.coordinator.get().open_findings();
    }

    pub(in crate::ui) fn open_for_selection(&self, ids: Vec<i64>) {
        self.coordinator.get().open_for_selection(ids);
    }
}

impl LibraryDoctorCoordinator {
    fn new(context: OwnedLibraryDoctorContext, progress: &DoctorProgressCard) -> Rc<Self> {
        let OwnedLibraryDoctorContext {
            conn,
            db_path,
            content_navigation,
            content_stack,
            doctor_navigation,
            doctor_chrome,
            window,
            track_list,
            scan_controls,
            fingerprint,
            sidebar,
            toast_overlay,
            refresh_views,
        } = context;
        let navigation =
            DoctorNavigation::new(&content_navigation, &content_stack, &doctor_navigation);
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
            super::startup_report::mark("LibraryDoctorCoordinator::fingerprint.capability begin");
            let fingerprint_available = matches!(
                fingerprint.capability(),
                FingerprintCapability::Available { .. }
            );
            super::startup_report::mark("LibraryDoctorCoordinator::fingerprint.capability end");
            super::startup_report::mark("LibraryDoctorPage::new begin");
            let page = LibraryDoctorPage::new(&conn, &window, fingerprint_available, refresh);
            super::startup_report::mark("LibraryDoctorPage::new end");
            Self {
                conn: conn.clone(),
                db_path: db_path.clone(),
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
                progress: progress.clone(),
                toast_overlay: toast_overlay.clone(),
                tag_write_gate: track_list.tag_write_gate(),
                refresh_views,
                sidebar: sidebar.clone(),
                selection_override: RefCell::new(None),
                _doctor_chrome: doctor_chrome,
            }
        });
        super::startup_report::mark("LibraryDoctorCoordinator::load_last_scan begin");
        coordinator.load_last_scan();
        super::startup_report::mark("LibraryDoctorCoordinator::load_last_scan end");
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
        coordinator
            .navigation
            .add_root(coordinator.page.navigation_page());
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
        self.navigation.show_root();
    }

    pub(super) fn load_last_scan(&self) {
        super::startup_report::mark("LibraryDoctorCoordinator::last_complete_scan begin");
        let scan = {
            let conn = &self.conn;
            LibraryDoctor::new(conn)
                .last_complete_scan()
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "could not load the last Library Doctor result");
                    None
                })
        };
        super::startup_report::mark("LibraryDoctorCoordinator::last_complete_scan end");
        super::startup_report::mark("LibraryDoctorCoordinator::last_cleanup begin");
        let revert_available = self.revert_available();
        super::startup_report::mark("LibraryDoctorCoordinator::last_cleanup end");
        self.page.set_scan(scan, revert_available);
    }

    /// `Undo` is only offered while there is a cleanup left to undo. Without
    /// this the button would be sensitive after a revert and do nothing when
    /// clicked, because `start_revert` bails out on an empty cleanup.
    fn revert_available(&self) -> bool {
        let conn = &self.conn;
        LibraryDoctor::new(conn)
            .cleanup_available()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not read the last Library Doctor cleanup");
                false
            })
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
        self.progress.show_scan(
            reprise_core::library_doctor::DoctorScanPhase::ReadingTags,
            0,
            0,
        );
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
                    coordinator.page.update_scan_job(
                        progress.phase,
                        progress.completed_tracks,
                        progress.total_tracks,
                    );
                    coordinator.progress.show_scan(
                        progress.phase,
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
        let review = self.review.borrow().as_ref().cloned();
        self.navigation.show_review_or_root(review);
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
