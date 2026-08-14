use gtk4::prelude::*;
use reprise_core::view_source::ViewSource;

use super::{apply_marking, place_for_content_page, SidebarPlace};
use crate::ui::sidebar::{rebuild, surface::test_shared};

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
