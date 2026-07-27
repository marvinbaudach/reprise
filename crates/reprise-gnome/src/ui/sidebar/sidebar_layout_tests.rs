use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::scanner::ScanProgress;
use reprise_core::library::settings::PlayerBarPosition;

use super::*;
use crate::ui::player_bar::library_player_bar::LibraryPlayerBarShell;
use crate::ui::scan::scan_progress::ScanProgressView;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_8_progress_region_reaches_split_view_bottom() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
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
        total: 2,
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
    assert!(
        sidebar.activity_slot.progress_widget().vexpands(),
        "the active progress page must fill the allocation owned by the non-expanding stack"
    );
    assert_eq!(
        sidebar.activity_slot.progress_widget().valign(),
        gtk4::Align::Fill,
        "the allocated progress root must lay out its bottom spacer"
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
