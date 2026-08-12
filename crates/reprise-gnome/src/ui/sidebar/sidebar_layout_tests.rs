use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::scanner::ScanProgress;
use reprise_core::library::settings::PlayerBarPosition;

use super::*;
use crate::ui::player_bar::library_player_bar::LibraryPlayerBarShell;
use crate::ui::scan::scan_progress::ScanProgressView;
use crate::ui::sidebar_presentation;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_8_progress_region_reaches_split_view_bottom() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let window = adw::ApplicationWindow::builder()
        .default_width(1200)
        .default_height(800)
        .build();
    window.set_size_request(1200, 800);
    let sidebar = Sidebar::new(conn, &window, || 0);
    for index in 0..8 {
        sidebar
            .shared
            .listbox
            .append(&gtk4::Label::new(Some(&format!("Extra row {index}"))));
    }
    let scanner = ScanProgressView::new();
    scanner.show(&ScanProgress::Scanning {
        processed: 1,
        total: Some(2),
        current_path: "sine.flac".into(),
    });
    sidebar.append_scan_card(scanner.widget());
    let relink = gtk4::Revealer::new();
    relink.set_height_request(88);
    sidebar.append_relink_card(&relink);
    let doctor = gtk4::Revealer::new();
    doctor.set_height_request(88);
    sidebar.append_doctor_card(&doctor);
    assert!(!relink.is_visible());
    assert!(!doctor.is_visible());
    // FB-8, amended: the cards no longer occupy a stack page of their own, so
    // the progress root does not have to fight for an allocation — the bottom
    // region is what hugs the sidebar's bottom edge, with the Issues block above
    // the cards and both visible at once.
    assert!(
        !sidebar.activity_slot.progress_widget().vexpands(),
        "the progress root rides along with the bottom region instead of expanding"
    );
    let root = sidebar.widget();
    let page = adw::NavigationPage::builder()
        .title("Library")
        .child(root)
        .build();
    let content = adw::NavigationView::new();
    let split = adw::OverlaySplitView::builder()
        .sidebar(&page)
        .content(&content)
        .collapsed(false)
        .show_sidebar(true)
        .build();
    let player = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    player.set_height_request(90);
    let shell =
        LibraryPlayerBarShell::new(&split, Some(player.upcast_ref()), PlayerBarPosition::Bottom);
    window.set_content(Some(shell.widget()));

    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let region = root.last_child().expect("sidebar bottom region");
    assert!(
        region.is_vexpand_set(),
        "the bottom region must block descendant expansion from propagating"
    );
    let bounds = region.compute_bounds(root).expect("region root bounds");
    let region_bottom = bounds.y() + bounds.height();
    let scanner_bounds = scanner
        .widget()
        .compute_bounds(root)
        .expect("scanner root bounds");
    let scanner_bottom = scanner_bounds.y() + scanner_bounds.height();
    assert_eq!(page.height(), split.height());
    assert_eq!(root.height(), page.height());
    assert!(
        (region_bottom - root.height() as f32).abs() < 1.0,
        "progress bottom={region_bottom}, root height={}",
        root.height()
    );
    assert!(
        (scanner_bottom - root.height() as f32).abs() < 1.0,
        "scanner bottom={scanner_bottom}, root height={}",
        root.height()
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_1_visible_job_card_keeps_the_real_split_sidebar_at_240px() {
    libadwaita::init().unwrap();
    crate::ui::style::install();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let window = adw::ApplicationWindow::builder()
        .default_width(1200)
        .default_height(800)
        .build();
    let sidebar = Sidebar::new(conn, &window, || 0);
    let scanner = ScanProgressView::new();
    scanner.show_batch(
        "Lyrics batch check complete",
        "2,177 of 2,177 checked · 0 downloaded · 0 unavailable",
        1.0,
    );
    sidebar.append_scan_card(scanner.widget());
    let page = adw::NavigationPage::builder()
        .title("Library")
        .child(sidebar.widget())
        .build();
    let content = adw::NavigationView::new();
    let split = adw::OverlaySplitView::builder()
        .sidebar(&page)
        .content(&content)
        .collapsed(false)
        .show_sidebar(true)
        .build();
    sidebar_presentation::style_overlay_split_view(&split);
    window.set_content(Some(&split));
    window.present();
    drain_display_events();

    assert_eq!(
        page.width(),
        240,
        "a visible job card must not override the split view's fixed sidebar width"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_5c_a_visible_job_card_never_overlaps_a_navigation_row() {
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    let window = adw::ApplicationWindow::builder()
        .default_width(240)
        .default_height(700)
        .build();
    let sidebar = Sidebar::new(conn, &window, || 0);
    sidebar.widget().set_size_request(240, -1);
    let missing = gtk4::ListBoxRow::new();
    missing.set_child(Some(&gtk4::Label::new(Some("Missing files"))));
    sidebar.shared.issues_listbox.append(&missing);
    sidebar.shared.issues_listbox.set_visible(true);

    let doctor = diagnostic_job_card("Checking tracks…", false);
    let relink = diagnostic_job_card("Searching for missing files…", true);
    sidebar.append_doctor_card(&doctor);
    sidebar.append_relink_card(&relink);
    window.set_content(Some(sidebar.widget()));
    window.present();

    doctor.set_reveal_child(true);
    drain_display_events();
    assert_card_below_issues(&sidebar, &doctor, "doctor");

    doctor.set_reveal_child(false);
    relink.set_reveal_child(true);
    drain_display_events();
    assert_card_below_issues(&sidebar, &relink, "relink");

    window.close();
}

fn diagnostic_job_card(title: &str, carries_open_action: bool) -> gtk4::Revealer {
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header.append(&gtk4::Spinner::new());
    let title = gtk4::Label::builder()
        .label(title)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .css_classes(["scan-card-title"])
        .build();
    header.append(&title);
    header.append(&gtk4::Label::new(Some("45%")));
    if carries_open_action {
        header.append(&gtk4::Button::with_label("Missing files"));
    }
    header.append(&gtk4::Button::with_label("Cancel"));
    let progress = gtk4::ProgressBar::new();
    progress.set_height_request(3);
    let detail = gtk4::Label::builder()
        .label("742/1,648 tracks")
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .css_classes(["scan-card-detail"])
        .build();
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    body.add_css_class("scan-card");
    body.append(&header);
    body.append(&progress);
    body.append(&detail);
    gtk4::Revealer::builder()
        .transition_type(gtk4::RevealerTransitionType::None)
        .child(&body)
        .build()
}

fn drain_display_events() {
    while gtk4::glib::MainContext::default().iteration(false) {}
}

fn assert_card_below_issues(sidebar: &Sidebar, card: &impl IsA<gtk4::Widget>, kind: &str) {
    let root = sidebar.widget();
    let issues = sidebar
        .shared
        .issues_listbox
        .compute_bounds(root)
        .expect("issues list bounds");
    let progress = sidebar
        .activity_slot
        .progress_widget()
        .compute_bounds(root)
        .expect("progress root bounds");
    let card = card
        .upcast_ref::<gtk4::Widget>()
        .compute_bounds(root)
        .expect("visible job card bounds");
    let issues_bottom = issues.y() + issues.height();
    let card_bottom = card.y() + card.height();
    tracing::warn!(
        kind,
        issues_y = issues.y(),
        issues_height = issues.height(),
        progress_y = progress.y(),
        progress_height = progress.height(),
        card_y = card.y(),
        card_height = card.height(),
        "DOC-5c sidebar job-card allocation"
    );
    eprintln!(
        "DOC-5c {kind}: issues={:.1}..{issues_bottom:.1}, progress={:.1}..{:.1}, card={:.1}..{card_bottom:.1}",
        issues.y(),
        progress.y(),
        progress.y() + progress.height(),
        card.y(),
    );
    assert!(
        issues_bottom <= card.y(),
        "{kind} card overlaps the Missing files row by {:.1}px (issues {:.1}..{:.1}, card {:.1}..{:.1})",
        issues_bottom - card.y(),
        issues.y(),
        issues_bottom,
        card.y(),
        card_bottom
    );
}
