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
    DoctorScanOutcome, DoctorScanProgress, DoctorScanRequest, DoctorScopeRequest,
    DoctorViewSnapshot, LibraryDoctor, ScanControl,
};
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use super::preferences::PreferencesContext;
use super::scan_flow::ScanControls;
use super::track_list::TrackList;
use summary_page::LibraryDoctorPage;

const PLUGIN_TARGETS: &[&str] = &["library_doctor"];

pub(in crate::ui) struct LibraryDoctorCoordinator {
    conn: Rc<RefCell<Connection>>,
    db_path: PathBuf,
    navigation: adw::NavigationView,
    page: Rc<LibraryDoctorPage>,
    track_list: Rc<TrackList>,
    scan_controls: ScanControls,
    fingerprint: Arc<dyn FingerprintBackend>,
    preferences: std::rc::Weak<PreferencesContext>,
    cancellation: RefCell<Option<Arc<AtomicBool>>>,
    running: Cell<bool>,
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
        } = context;
        let coordinator = Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let refresh = {
                let weak = weak.clone();
                Rc::new(move |_| {
                    if let Some(coordinator) = weak.upgrade() {
                        coordinator.page.refresh();
                    }
                }) as Rc<dyn Fn(bool)>
            };
            let fingerprint_available = matches!(
                fingerprint.capability(),
                FingerprintCapability::Available { .. }
            );
            let page = LibraryDoctorPage::new(conn, window, fingerprint_available, refresh);
            Self {
                conn: conn.clone(),
                db_path: db_path.to_path_buf(),
                navigation: navigation.clone(),
                page,
                track_list: track_list.clone(),
                scan_controls: scan_controls.clone(),
                fingerprint,
                preferences: Rc::downgrade(preferences),
                cancellation: RefCell::new(None),
                running: Cell::new(false),
            }
        });
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
            coordinator.page.connect_cancel(move || {
                if let Some(coordinator) = weak.upgrade() {
                    coordinator.request_cancel();
                }
            });
        }
        coordinator
    }

    pub(in crate::ui) fn open(&self) {
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
        self.page.set_running(true);
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
                    coordinator
                        .page
                        .set_progress(progress.completed_tracks, progress.total_tracks);
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
                Ok(Ok(DoctorScanOutcome::Completed(scan))) => coordinator.page.set_scan(Some(scan)),
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
        self.page.set_running(false);
        self.scan_controls.button.set_sensitive(true);
    }
}

fn run_scan(
    db_path: &Path,
    request: &DoctorScanRequest,
    fingerprint: &dyn FingerprintBackend,
    cancellation: &AtomicBool,
    publish: &mut dyn FnMut(DoctorScanProgress),
) -> Result<DoctorScanOutcome, String> {
    let mut conn =
        reprise_core::db::open_migrated(Some(db_path)).map_err(|error| error.to_string())?;
    LibraryDoctor::new(&mut conn)
        .scan(request, Some(fingerprint), |progress| {
            publish(progress);
            if cancellation.load(Ordering::Relaxed) {
                ScanControl::Cancel
            } else {
                ScanControl::Continue
            }
        })
        .map_err(|error| error.to_string())
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
