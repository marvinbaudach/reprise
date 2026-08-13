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
fn sidebar_headings_and_surfaces_share_one_column_edge() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    libadwaita::init().unwrap();
    crate::ui::style::install();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let window = adw::ApplicationWindow::builder()
        .default_width(240)
        .default_height(900)
        .build();
    let sidebar = Sidebar::new(conn, &window, || 0);
    sidebar.widget().set_size_request(240, -1);

    let issue_row = sidebar_presentation::build_issue_nav_row(
        "Missing files",
        sidebar_presentation::issue_row_presentation(1, sidebar_presentation::NavIcon::Missing),
        sidebar_presentation::NavIcon::Missing,
    );
    sidebar.shared.issues_listbox.append(&issue_row);
    sidebar.shared.issues_listbox.set_visible(true);

    let device = crate::ui::sidebar::sidebar_device_card::tests::view(
        crate::ui::device_sync_runtime::PlannedSyncPhase::Idle,
    );
    let device_section =
        crate::ui::sidebar::sidebar_device_section::present_device_section_for_test(&device);
    sidebar.activity_slot.set_device_section(&device_section);

    window.set_content(Some(sidebar.widget()));
    window.present();
    drain_display_events();

    let root = sidebar.widget();
    let library_heading = sidebar
        .shared
        .listbox
        .row_at_index(0)
        .and_then(|row| row.child())
        .and_downcast::<gtk4::Label>()
        .expect("the first navigation row is the Library heading label");
    let library_row = find_row(
        &sidebar.shared,
        &reprise_core::view_source::ViewSource::Library,
    )
    .expect("the sidebar has a Music navigation row");
    let navigation_content = navigation_row_content(&library_row);
    // The title itself follows the row's 16 px icon and 10 px spacing. Reach
    // that real label so this cannot accidentally become a row-only test, but
    // compare the heading column with the icon-bearing row's leading content.
    let navigation_title = navigation_title_label(&library_row);
    assert_eq!(
        navigation_title.text(),
        crate::ui::strings::text(crate::ui::strings::SIDEBAR_MUSIC)
    );
    let navigation_icon = navigation_content
        .first_child()
        .expect("a navigation row starts with an icon");
    let issues_heading = descendant_label(
        root,
        &crate::ui::strings::text(crate::ui::strings::SIDEBAR_SECTION_ISSUES),
    );
    let devices_heading = descendant_label(&device_section, "DEVICES");
    let device_heading_content = devices_heading
        .parent()
        .and_downcast::<gtk4::Box>()
        .expect("the Devices label lives in the heading content box");
    let device_card = descendant_with_css_class(&device_section, "device-card");

    let text_edges = [
        ("LIBRARY", left_edge(&library_heading, root)),
        ("Music row content", left_edge(&navigation_content, root)),
        ("ISSUES", left_edge(&issues_heading, root)),
        ("DEVICES", left_edge(&devices_heading, root)),
    ];
    let expected_text_edge = text_edges[0].1;
    let navigation_title_edge = left_edge(&navigation_title, root);
    let expected_title_edge = left_edge(&navigation_icon, root)
        + navigation_icon.width() as f32
        + navigation_content.spacing() as f32;
    let navigation_surface_edge = left_edge(&library_row, root);
    let device_surface_edge = left_edge(&device_card, root);
    let navigation_content_right_edge = right_edge(&navigation_content, root);
    let device_heading_content_right_edge = right_edge(&device_heading_content, root);
    let navigation_surface_right_edge = right_edge(&library_row, root);
    let device_surface_right_edge = right_edge(&device_card, root);
    eprintln!("sidebar text edges: {text_edges:?}");
    eprintln!(
        "sidebar content right edges: navigation={navigation_content_right_edge:.1}, device heading={device_heading_content_right_edge:.1}"
    );
    eprintln!(
        "sidebar surface edges: navigation=({navigation_surface_edge:.1}, {navigation_surface_right_edge:.1}), device=({device_surface_edge:.1}, {device_surface_right_edge:.1})"
    );
    for (name, edge) in text_edges {
        assert!(
            (edge - expected_text_edge).abs() <= 1.0,
            "{name} starts at {edge:.1}px, but the sidebar text column starts at {expected_text_edge:.1}px"
        );
    }
    assert!(
        (navigation_title_edge - expected_title_edge).abs() <= 1.0,
        "Music title starts at {navigation_title_edge:.1}px, but its icon and spacing place it at {expected_title_edge:.1}px"
    );
    assert!(
        (device_heading_content_right_edge - navigation_content_right_edge).abs() <= 1.0,
        "device heading content ends at {device_heading_content_right_edge:.1}px, but navigation trailing content ends at {navigation_content_right_edge:.1}px"
    );

    assert!(
        (device_surface_edge - navigation_surface_edge).abs() <= 1.0,
        "device card starts at {device_surface_edge:.1}px, but navigation surfaces start at {navigation_surface_edge:.1}px"
    );
    assert!(
        (device_surface_right_edge - navigation_surface_right_edge).abs() <= 1.0,
        "device card ends at {device_surface_right_edge:.1}px, but navigation surfaces end at {navigation_surface_right_edge:.1}px"
    );

    window.close();
}

fn navigation_title_label(row: &gtk4::ListBoxRow) -> gtk4::Label {
    navigation_row_content(row)
        .first_child()
        .and_then(|icon| icon.next_sibling())
        .and_downcast::<gtk4::Label>()
        .expect("a navigation row has a title label after its icon")
}

fn navigation_row_content(row: &gtk4::ListBoxRow) -> gtk4::Box {
    row.child()
        .and_downcast::<gtk4::Button>()
        .and_then(|button| button.child())
        .and_downcast::<gtk4::Box>()
        .expect("a navigation row has a content box")
}

fn descendant_label(root: &impl IsA<gtk4::Widget>, text: &str) -> gtk4::Label {
    find_descendant(root.upcast_ref(), &|widget| {
        widget
            .downcast_ref::<gtk4::Label>()
            .is_some_and(|label| label.text() == text)
    })
    .and_downcast::<gtk4::Label>()
    .unwrap_or_else(|| panic!("no descendant label named {text:?}"))
}

fn descendant_with_css_class(root: &impl IsA<gtk4::Widget>, css_class: &str) -> gtk4::Widget {
    find_descendant(root.upcast_ref(), &|widget| widget.has_css_class(css_class))
        .unwrap_or_else(|| panic!("no descendant with CSS class {css_class:?}"))
}

fn find_descendant(
    root: &gtk4::Widget,
    predicate: &dyn Fn(&gtk4::Widget) -> bool,
) -> Option<gtk4::Widget> {
    if predicate(root) {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_descendant(&widget, predicate) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn left_edge(widget: &impl IsA<gtk4::Widget>, root: &impl IsA<gtk4::Widget>) -> f32 {
    widget
        .upcast_ref::<gtk4::Widget>()
        .compute_bounds(root.upcast_ref())
        .expect("the sidebar child is allocated")
        .x()
}

fn right_edge(widget: &impl IsA<gtk4::Widget>, root: &impl IsA<gtk4::Widget>) -> f32 {
    let bounds = widget
        .upcast_ref::<gtk4::Widget>()
        .compute_bounds(root.upcast_ref())
        .expect("the sidebar child is allocated");
    root.width() as f32 - (bounds.x() + bounds.width())
}

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
