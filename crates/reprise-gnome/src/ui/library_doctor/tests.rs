use std::path::Path;
use std::sync::atomic::AtomicBool;

use gtk4::prelude::*;
use libadwaita as adw;
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
fn doc_7c_entry_scope_defaults_to_library_and_suggests_filtered_view() {
    assert_eq!(super::suggested_scope(&snapshot()), 0);

    let mut filtered = snapshot();
    filtered.filter = "needle".into();
    assert_eq!(super::suggested_scope(&filtered), 1);

    let mut browsed = snapshot();
    browsed.browse.genre = Some("Jazz".into());
    assert_eq!(super::suggested_scope(&browsed), 1);
}

#[test]
fn doc_7c_the_doctor_is_a_content_stack_child_not_a_content_nav_push() {
    let coordinator = include_str!("mod.rs");
    let navigation = include_str!("navigation.rs");
    let window = include_str!("../window/window.rs");

    assert!(window.contains("Some(\"library-doctor\")"));
    assert!(navigation.contains("content_stack::show_page"));
    assert!(coordinator.contains("self.navigation.show_root()"));
    assert!(!navigation.contains("content_navigation.push"));
}

#[test]
fn doc_7c_the_review_page_is_pushed_inside_the_doctors_own_navigation_view() {
    let coordinator = include_str!("mod.rs");
    let navigation = include_str!("navigation.rs");
    let review = include_str!("review_page.rs");

    assert!(coordinator.contains("self.navigation.show_review_or_root(review)"));
    assert!(navigation.contains("self.show_review(review.navigation_page())"));
    assert!(navigation.contains("self.doctor_navigation.push(page)"));
    assert!(review.contains("adw::WindowTitle::new"));
    assert!(review.contains("doctor-review-header-action"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_7c_opening_the_doctor_keeps_the_now_playing_pane_open() {
    if gtk4::init().is_err() {
        return;
    }
    let stack = gtk4::Stack::new();
    stack.add_named(&gtk4::Label::new(Some("Library")), Some("library"));
    stack.add_named(&adw::NavigationView::new(), Some("library-doctor"));
    stack.set_visible_child_name("library");
    let info_panel = gtk4::Label::new(Some("Now Playing"));
    let split = adw::OverlaySplitView::builder()
        .content(&stack)
        .sidebar(&info_panel)
        .show_sidebar(true)
        .collapsed(false)
        .build();
    let window = adw::ApplicationWindow::builder()
        .default_width(1_000)
        .default_height(700)
        .content(&split)
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    crate::ui::window::content_stack::show_page(&stack, "library-doctor");

    assert!(split.shows_sidebar());
    assert_eq!(
        stack.visible_child_name().as_deref(),
        Some("library-doctor")
    );
    window.close();
}

#[test]
fn doctor_worker_uses_only_its_isolated_database_connection() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("doctor.db");
    drop(reprise_core::db::Db::open_migrated(Some(&database)).unwrap());
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

#[test]
fn late_scan_progress_is_rejected_after_finish_or_new_generation() {
    assert!(super::accepts_scan_progress(
        4,
        4,
        true,
        Some(super::DoctorJobKind::Scan)
    ));
    assert!(!super::accepts_scan_progress(
        5,
        4,
        true,
        Some(super::DoctorJobKind::Scan)
    ));
    assert!(!super::accepts_scan_progress(4, 4, false, None));
}

#[test]
fn doc_8a_done_marks_the_scan_reviewed_and_clears_the_sidebar_entry() {
    let db = crate::test_db::open().unwrap();
    let request = reprise_core::library_doctor::DoctorScanRequest {
        scope: DoctorScopeRequest::WholeLibrary,
        options: reprise_core::library_doctor::DoctorScanOptions::local_only(),
    };
    let outcome = reprise_core::library_doctor::LibraryDoctor::new(&db)
        .scan(&request, Some(&NeverFingerprint), |_| {
            reprise_core::library_doctor::ScanControl::Continue
        })
        .unwrap();
    let reprise_core::library_doctor::DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("empty scan must complete")
    };

    reprise_core::library_doctor::LibraryDoctor::new(&db)
        .set_reviewed_scan(scan.id)
        .unwrap();
    assert_eq!(
        reprise_core::queries::count_pending_doctor_findings(&db).unwrap(),
        0
    );
    let coordinator = include_str!("mod.rs");
    assert!(coordinator.contains("Library Doctor scan acknowledged"));
}
