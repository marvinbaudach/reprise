use gtk4::prelude::*;
use libadwaita::prelude::*;
use reprise_core::view_source::ViewSource;

use super::{apply_marking, place_for_content_page, SidebarPlace};
use crate::ui::sidebar::{find_row, rebuild, surface::test_shared, Sidebar};

#[test]
fn nav_18_only_the_two_placeless_pages_leave_the_source_marking() {
    assert_eq!(
        place_for_content_page(Some("library"), None),
        SidebarPlace::Source
    );
    assert_eq!(
        place_for_content_page(Some("stats"), None),
        SidebarPlace::Source
    );
    assert_eq!(
        place_for_content_page(Some("podcasts"), None),
        SidebarPlace::Source
    );
    assert_eq!(place_for_content_page(None, None), SidebarPlace::Source);
    assert_eq!(
        place_for_content_page(Some("library-doctor"), None),
        SidebarPlace::LibraryDoctor
    );
    assert_eq!(
        place_for_content_page(Some("device-sync"), Some("pixel")),
        SidebarPlace::Device("pixel".to_string())
    );
    assert_eq!(
        place_for_content_page(Some("device-sync"), None),
        SidebarPlace::Unknown
    );
}

fn seed_pending_doctor_finding(shared: &crate::ui::sidebar::Shared) {
    let conn = crate::test_db::connection(&shared.conn);
    conn.execute(
        "INSERT INTO library_doctor_scans \
         (id, scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
         VALUES (1, 'selection', 1, 0, 1, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE library_doctor_state SET last_complete_scan_id=1 WHERE singleton=1",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, added_at, file_mtime, file_size) \
         VALUES (1, '/fixtures/doctor.flac', 'Fixture', 0, 11, 22)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO library_doctor_scan_tracks \
         (scan_id, position, track_id, path, file_mtime, file_size, read_ok, \
          title, artist, album, album_artist, year, track_no, genre) \
         VALUES (1, 0, 1, '/fixtures/doctor.flac', 11, 22, 1, \
                 'Fixture', ' Before ', 'Album', '', NULL, NULL, 'Rock')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO library_doctor_proposals \
         (scan_id, position, track_id, field, current_value, proposed_value, source, \
          confidence, preselected, problem_class, evidence_json, local_fallback_json) \
         VALUES (1, 0, 1, 'artist', ' Before ', 'Before', 'musicbrainz', \
                 100, 1, 'casing_whitespace', '[]', 'null')",
        [],
    )
    .unwrap();
    assert_eq!(
        reprise_core::queries::count_pending_doctor_findings(&shared.conn).unwrap(),
        1,
        "the fixture must make the Doctor action row visible"
    );
}

fn present_sidebar(shared: &crate::ui::sidebar::Shared) -> gtk4::Window {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.append(&shared.listbox);
    root.append(&shared.issues_listbox);
    let window = gtk4::Window::builder().child(&root).build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    window
}

fn assert_only_doctor_is_marked(shared: &crate::ui::sidebar::Shared) {
    let doctor_row = shared
        .doctor_row
        .borrow()
        .clone()
        .expect("a pending finding must build the Doctor row");
    assert_eq!(
        shared.issues_listbox.selected_row().as_ref(),
        Some(&doctor_row)
    );
    assert!(shared.listbox.selected_row().is_none());
}

fn doctor_marking_fixture() -> (std::rc::Rc<crate::ui::sidebar::Shared>, gtk4::Window) {
    gtk4::init().unwrap();
    crate::ui::style::install();
    let shared = test_shared();
    seed_pending_doctor_finding(&shared);
    *shared.current_source.borrow_mut() = ViewSource::MyStats;
    rebuild(&shared, None, "test build");
    let window = present_sidebar(&shared);
    *shared.current_place.borrow_mut() = SidebarPlace::LibraryDoctor;
    apply_marking(&shared);
    (shared, window)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_18_the_doctor_page_marks_the_doctor_row_and_no_source_row() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let (shared, window) = doctor_marking_fixture();

    assert_only_doctor_is_marked(&shared);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_18_a_rebuild_while_the_doctor_is_visible_does_not_take_the_marking_back() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let (shared, window) = doctor_marking_fixture();

    rebuild(&shared, None, "counts refresh");
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_only_doctor_is_marked(&shared);
    window.close();
}

struct RoutingFixture {
    sidebar: std::rc::Rc<Sidebar>,
    stack: gtk4::Stack,
    window: libadwaita::ApplicationWindow,
    outside: gtk4::Button,
    routed: std::rc::Rc<std::cell::RefCell<Vec<ViewSource>>>,
    shown: std::rc::Rc<std::cell::Cell<u32>>,
}

impl RoutingFixture {
    fn open_doctor_through_findings(&self) {
        let doctor_row = self
            .sidebar
            .shared
            .doctor_row
            .borrow()
            .clone()
            .expect("the Doctor row must exist");
        doctor_row.emit_by_name::<()>("activate", &[]);
        pump_main_context();
    }

    fn open_doctor_through_selection(&self) {
        // Production's Edit tag path ends in DoctorNavigation::show_root,
        // which changes this same stack child without touching the sidebar.
        crate::ui::window::content_stack::show_page(&self.stack, "library-doctor");
        pump_main_context();
    }

    fn activate_my_stats(&self) {
        let row = find_row(&self.sidebar.shared, &ViewSource::MyStats).unwrap();
        row.emit_by_name::<()>("activate", &[]);
        pump_main_context();
    }

    fn assert_returned_to_my_stats(&self) {
        assert_eq!(self.stack.visible_child_name().as_deref(), Some("stats"));
        assert_eq!(*self.routed.borrow(), vec![ViewSource::MyStats]);
        let stats_row = find_row(&self.sidebar.shared, &ViewSource::MyStats).unwrap();
        assert_eq!(
            self.sidebar.shared.listbox.selected_row().as_ref(),
            Some(&stats_row)
        );
        assert!(self.sidebar.shared.issues_listbox.selected_row().is_none());
        assert_eq!(self.shown.get(), 1);
    }
}

fn routing_fixture() -> RoutingFixture {
    gtk4::init().unwrap();
    crate::ui::style::install();
    let conn = std::rc::Rc::new(crate::test_db::open().unwrap());
    let window = libadwaita::ApplicationWindow::builder()
        .default_width(900)
        .default_height(600)
        .build();
    let sidebar = std::rc::Rc::new(Sidebar::new(conn, &window, || 0));
    seed_pending_doctor_finding(&sidebar.shared);
    *sidebar.shared.current_source.borrow_mut() = ViewSource::MyStats;
    sidebar.refresh("Doctor route fixture");

    let stack = gtk4::Stack::new();
    stack.add_named(&gtk4::Label::new(Some("Stats")), Some("stats"));
    stack.add_named(
        &gtk4::Label::new(Some("Library Doctor")),
        Some("library-doctor"),
    );
    stack.set_visible_child_name("stats");
    sidebar.bind_content_stack(&stack);

    let routed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    {
        let stack = stack.clone();
        let routed = routed.clone();
        sidebar.set_on_select(move |source, _| {
            routed.borrow_mut().push(source);
            crate::ui::window::content_stack::show_page(&stack, "stats");
        });
    }
    let shown = std::rc::Rc::new(std::cell::Cell::new(0));
    {
        let shown = shown.clone();
        sidebar.set_on_show_content(move || shown.set(shown.get() + 1));
    }

    let doctor_action = gtk4::gio::SimpleAction::new("library-doctor-findings", None);
    {
        let stack = stack.clone();
        doctor_action.connect_activate(move |_, _| {
            crate::ui::window::content_stack::show_page(&stack, "library-doctor");
        });
    }
    let window_actions = gtk4::gio::SimpleActionGroup::new();
    window_actions.add_action(&doctor_action);
    window.insert_action_group("win", Some(&window_actions));

    let outside = gtk4::Button::with_label("Outside");
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    body.append(sidebar.widget());
    body.append(&stack);
    content.append(&body);
    content.append(&outside);
    window.set_content(Some(&content));
    window.present();
    settle_until_active(&window);

    RoutingFixture {
        sidebar,
        stack,
        window,
        outside,
        routed,
        shown,
    }
}

fn settle_until_active(window: &libadwaita::ApplicationWindow) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !window.is_active() {
        assert!(
            std::time::Instant::now() < deadline,
            "test window did not become active within 2s"
        );
        gtk4::glib::MainContext::default().iteration(true);
    }
}

fn pump_main_context() {
    while gtk4::glib::MainContext::default().iteration(false) {}
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_18_activating_the_marked_source_from_the_doctor_findings_routes_back() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = routing_fixture();

    fixture.open_doctor_through_findings();
    fixture.activate_my_stats();

    fixture.assert_returned_to_my_stats();
    fixture.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_18_activating_the_marked_source_from_a_doctor_selection_routes_back() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = routing_fixture();

    fixture.open_doctor_through_selection();
    fixture.activate_my_stats();

    fixture.assert_returned_to_my_stats();
    fixture.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_18_focus_leaving_the_sidebar_does_not_snap_the_marking_back_to_a_source() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = routing_fixture();
    fixture.open_doctor_through_selection();
    let stats_row = find_row(&fixture.sidebar.shared, &ViewSource::MyStats).unwrap();

    assert!(stats_row.grab_focus());
    fixture.sidebar.shared.listbox.select_row(Some(&stats_row));
    assert!(fixture.routed.borrow().is_empty());
    assert!(fixture.outside.grab_focus());
    pump_main_context();

    assert_only_doctor_is_marked(&fixture.sidebar.shared);
    assert!(fixture.routed.borrow().is_empty());
    assert_eq!(
        fixture.stack.visible_child_name().as_deref(),
        Some("library-doctor")
    );
    fixture.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_18_activating_the_doctor_row_while_the_doctor_is_visible_changes_nothing() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = routing_fixture();
    fixture.open_doctor_through_findings();

    fixture.open_doctor_through_findings();

    assert!(fixture.routed.borrow().is_empty());
    assert_eq!(fixture.shown.get(), 0);
    assert_eq!(
        fixture.stack.visible_child_name().as_deref(),
        Some("library-doctor")
    );
    assert_only_doctor_is_marked(&fixture.sidebar.shared);
    fixture.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_18_the_vanishing_doctor_row_leaves_nothing_marked_instead_of_the_old_source() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let fixture = routing_fixture();
    fixture.open_doctor_through_findings();
    crate::test_db::connection(&fixture.sidebar.shared.conn)
        .execute(
            "UPDATE library_doctor_state SET reviewed_scan_id=last_complete_scan_id \
             WHERE singleton=1",
            [],
        )
        .unwrap();

    fixture.sidebar.refresh("findings applied");
    pump_main_context();

    assert!(fixture.sidebar.shared.doctor_row.borrow().is_none());
    assert!(fixture.sidebar.shared.listbox.selected_row().is_none());
    assert!(fixture
        .sidebar
        .shared
        .issues_listbox
        .selected_row()
        .is_none());
    fixture.activate_my_stats();
    fixture.assert_returned_to_my_stats();
    fixture.window.close();
}
