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

/// The stethoscope ships with the app, but the app can run against a theme
/// that has not been installed alongside it — a build tree, a broken package.
/// Then both doctor surfaces take the same step down to the magnifier rather
/// than drawing GTK's missing-image box.
#[test]
fn doc_8d_the_start_page_icon_falls_back_when_the_theme_lacks_it() {
    assert_eq!(
        super::doctor_glyph_for(true),
        "reprise-stethoscope-symbolic"
    );
    assert_eq!(super::doctor_glyph_for(false), "system-search-symbolic");
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
    let chrome = include_str!("../window/library_chrome.rs");
    let header = include_str!("review_header.rs");

    assert!(coordinator.contains("self.navigation.show_review_or_root(review)"));
    assert!(!coordinator.contains("set_review_actions"));
    assert!(navigation.contains("self.show_review(review.navigation_page())"));
    assert!(navigation.contains("self.doctor_navigation.push(page)"));
    assert!(!review.contains("chrome_actions"));
    assert!(!chrome.contains("review_actions"));
    assert!(header.contains("groups.selection.add_widget(&select_all)"));
    assert!(review.contains("header.select_all.connect_toggled"));
    assert!(!review.contains("adw::HeaderBar::new"));
    assert!(!review.contains("toolbar.add_top_bar"));
}

#[test]
fn style_12_the_title_bar_only_carries_permanent_actions() {
    let coordinator = include_str!("mod.rs");
    let review = include_str!("review_page.rs");
    let chrome = include_str!("../window/library_chrome.rs");

    assert!(!coordinator.contains("set_review_actions"));
    assert!(!review.contains("chrome_actions"));
    assert!(!chrome.contains("review_actions"));
    assert!(review.contains("header.select_all.connect_toggled"));
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

fn review_scan(id: i64, proposed: &str) -> reprise_core::library_doctor::DoctorScan {
    use reprise_core::library_doctor::{
        DoctorField, DoctorProposal, DoctorScan, DoctorScanOptions, DoctorTrackRef,
        DoctorTrackSnapshot, DoctorValue, ProblemClass, ProposalSource,
    };

    DoctorScan {
        id,
        scope_kind: "whole_library".into(),
        created_at: id,
        options: DoctorScanOptions::local_only(),
        checked_tracks: 1,
        skipped_tracks: 0,
        track_ids: vec![7],
        tracks: vec![DoctorTrackSnapshot {
            reference: DoctorTrackRef {
                track_id: 7,
                path: std::path::PathBuf::from("/tmp/doctor-review.flac"),
                file_mtime: 1,
                file_size: 2,
                device: None,
                inode: None,
            },
            tags: Some(reprise_core::library::tag_edit::EditableTags {
                title: "Review track".into(),
                artist: "Artist".into(),
                album: "Album".into(),
                album_artist: "Artist".into(),
                year: Some(2020),
                track_no: Some(1),
                genre: "Rock".into(),
            }),
            stale: false,
        }],
        proposals: vec![DoctorProposal {
            track_id: 7,
            field: DoctorField::Genre,
            current: DoctorValue::Text("Rock".into()),
            proposed: DoctorValue::Text(proposed.into()),
            source: ProposalSource::MusicBrainz,
            confidence: 90,
            preselected: false,
            never_preselect: false,
            problem_class: ProblemClass::GenreVariant,
            resolved_release_mbid: None,
            evidence: Vec::new(),
            local_fallback: None,
        }],
        unresolved_groups: Vec::new(),
    }
}

/// Every review page carries the same navigation tag, so a second scan's
/// findings must take the first one's place instead of being swallowed by it.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_7c_a_second_review_session_replaces_the_first() {
    use std::rc::Rc;

    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    let window = adw::ApplicationWindow::builder()
        .default_width(900)
        .default_height(700)
        .build();
    let content_navigation = adw::NavigationView::new();
    let content_stack = gtk4::Stack::new();
    let doctor_navigation = adw::NavigationView::new();
    content_stack.add_named(&doctor_navigation, Some("library-doctor"));
    let navigation = super::navigation::DoctorNavigation::new(
        &content_navigation,
        &content_stack,
        &doctor_navigation,
    );
    let root = adw::NavigationPage::builder()
        .title("Library Doctor")
        .tag("library-doctor")
        .child(&gtk4::Label::new(Some("start")))
        .build();
    navigation.add_root(&root);
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    let page = |scan| {
        super::review_page::LibraryDoctorReviewPage::new(
            &conn,
            &window,
            &scan,
            Rc::new(|_| {}),
            Rc::new(|| {}),
            &on_edit,
        )
    };

    let first = page(review_scan(1, "Alternative"));
    navigation.show_review(first.navigation_page());
    let second = page(review_scan(2, "Indie"));
    navigation.show_review(second.navigation_page());
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(
        doctor_navigation.visible_page().as_ref(),
        Some(second.navigation_page()),
        "the review page on screen must be the one built for the second scan"
    );
}
