use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::view_source::ViewSource;

use super::*;

fn numeric_badge_text(widget: &gtk4::Widget) -> Option<String> {
    if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
        if label.has_css_class("numeric") && label.is_visible() {
            return Some(label.text().to_string());
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = numeric_badge_text(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn queue_change_avoids_rebuild_while_library_mutation_refreshes_counts() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let window = adw::ApplicationWindow::builder().build();
    let queue_len = Rc::new(Cell::new(0_usize));
    let sidebar = Sidebar::new(conn.clone(), &window, {
        let queue_len = queue_len.clone();
        move || queue_len.get()
    });
    let row_before = find_row(&sidebar.shared, &ViewSource::Queue).unwrap();
    let rebuilds_before = sidebar.shared.refresh_count.get();

    queue_len.set(2_340);
    sidebar.refresh_queue_count();

    let row_after = find_row(&sidebar.shared, &ViewSource::Queue).unwrap();
    assert_eq!(
        row_after, row_before,
        "the Queue row must retain its identity"
    );
    assert_eq!(
        numeric_badge_text(row_after.upcast_ref()),
        Some("2,340".to_string())
    );
    assert_eq!(
        sidebar.shared.refresh_count.get(),
        rebuilds_before,
        "a queue mutation must not rerun the sidebar query projection"
    );

    queue_len.set(0);
    sidebar.refresh_queue_count();
    assert_eq!(
        numeric_badge_text(row_after.upcast_ref()),
        None,
        "an empty queue must hide its numeric badge"
    );

    let music_before = find_row(&sidebar.shared, &ViewSource::Library).unwrap();
    assert_eq!(numeric_badge_text(music_before.upcast_ref()), None);

    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (path, title, added_at)
             VALUES ('/music/new.flac', 'New track', 0)",
            [],
        )
        .unwrap();

    queue_len.set(1);
    sidebar.refresh_queue_count();
    let music_after_queue_change = find_row(&sidebar.shared, &ViewSource::Library).unwrap();
    assert_eq!(music_after_queue_change, music_before);
    assert_eq!(
        numeric_badge_text(music_after_queue_change.upcast_ref()),
        None,
        "a queue-only refresh must not reproject unrelated counters"
    );

    sidebar.refresh("library content changed");
    let music_after_library_change = find_row(&sidebar.shared, &ViewSource::Library).unwrap();
    assert_ne!(music_after_library_change, music_before);
    assert_eq!(
        numeric_badge_text(music_after_library_change.upcast_ref()),
        Some("1".to_string()),
        "the mutation refresh route must keep database-backed counts current"
    );
}
