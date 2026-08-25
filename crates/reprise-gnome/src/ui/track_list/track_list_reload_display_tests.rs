use super::*;

use reprise_core::library::settings;

const GEOMETRY_TRACK_COUNT: i64 = 200;
const STALE_ROW_HEIGHT: f64 = 53.0;

fn geometry_fixture(seed: Option<f64>) -> (super::super::TrackList, gtk4::Window) {
    let conn = crate::test_db::open().unwrap();
    settings::set_row_height(&conn, seed).unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=GEOMETRY_TRACK_COUNT {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/geometry/{id:03}.flac"),
                format!("Track {id:03}"),
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let track_list = super::super::TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    (track_list, window)
}

fn uniform_allocated_row_height(column_view: &gtk4::ColumnView) -> Option<f64> {
    let measurement = crate::ui::list_geometry::ListGeometry::for_view(column_view).measurement();
    measurement
        .is_uniform()
        .then(|| measurement.modal())
        .flatten()
        .map(crate::ui::list_geometry::RowHeight::pixels)
}

fn has_row_intersecting_viewport(column_view: &gtk4::ColumnView) -> bool {
    let viewport_width = column_view.width() as f32;
    let viewport_height = column_view.height() as f32;
    let mut pending = vec![column_view.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = pending.pop() {
        let is_row = widget.type_().name().contains("ColumnViewRow");
        if is_row
            && widget.compute_bounds(column_view).is_some_and(|bounds| {
                bounds.x() < viewport_width
                    && bounds.x() + bounds.width() > 0.0
                    && bounds.y() < viewport_height
                    && bounds.y() + bounds.height() > 0.0
            })
        {
            return true;
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    false
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_query_reloading_metadata_save_keeps_the_live_viewport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=100 {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let track_list = super::super::TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let position = 60;
    track_list
        .shared
        .column_view
        .scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > 0.0
    });
    let before = adjustment.value();
    assert!(
        before > 0.0,
        "precondition: the list must be scrolled away from the top"
    );

    let opened_anchor = capture_reload_anchor(&track_list.shared);
    // Reproduce the asynchronous Tag Editor boundary: by the time the
    // worker completes, GTK may already report position zero while the
    // closing dialog restores focus. Capturing at completion would
    // therefore preserve the wrong position.
    adjustment.set_value(0.0);
    track_list.shared.selection.unselect_all();
    let written_id = track_list.shared.model.track_at(position).unwrap().id;
    let mut save_anchor = opened_anchor;
    save_anchor.selected_ids = vec![written_id];
    reload_with_anchor(&track_list.shared, &save_anchor);
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        (adjustment.value() - before).abs() < 1.0
    });

    assert!(
        adjustment.value() > 0.0,
        "rating save must not leave the viewport at the table top"
    );
    assert!(
        (adjustment.value() - before).abs() < 1.0,
        "rating save moved the viewport: before={before}, after={}",
        adjustment.value()
    );
    assert!(track_list.shared.selection.is_selected(position));
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_reload_with_a_deep_anchor_keeps_a_row_inside_the_viewport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=200 {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let track_list = super::super::TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let position = 150;
    track_list
        .shared
        .column_view
        .scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > adjustment.page_size() * 2.0
    });
    let anchor = capture_reload_anchor(&track_list.shared);
    assert!(
        anchor.anchor.is_some(),
        "deep viewport must capture an anchor"
    );

    reload_with_anchor(&track_list.shared, &anchor);
    crate::ui::test_settle::settle_for(SCROLL_ADJUSTMENT_HOLD);

    assert!(
        has_row_intersecting_viewport(&track_list.shared.column_view),
        "reloaded ColumnView has no row widget intersecting its viewport"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn library_reload_replaces_a_contradicted_persisted_row_height() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = geometry_fixture(None);

    assert!(crate::ui::test_settle::settle_until(
        crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
        || uniform_allocated_row_height(&track_list.shared.column_view).is_some()
    ));
    let allocated = uniform_allocated_row_height(&track_list.shared.column_view).unwrap();
    assert_ne!(
        allocated, STALE_ROW_HEIGHT,
        "fixture must contradict its seed"
    );
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    adjustment.emit_by_name::<()>("changed", &[]);
    track_list
        .shared
        .list_geometry_cache
        .seed_measured_row_height(STALE_ROW_HEIGHT);
    settings::set_row_height(&track_list.shared.conn, Some(STALE_ROW_HEIGHT)).unwrap();
    adjustment.set_upper(STALE_ROW_HEIGHT * GEOMETRY_TRACK_COUNT as f64);
    assert_eq!(
        settings::get_row_height(&track_list.shared.conn).unwrap(),
        Some(STALE_ROW_HEIGHT),
        "precondition: the reload must start from contradicted persisted geometry"
    );
    reload(&track_list.shared);

    let persisted = settings::get_row_height(&track_list.shared.conn).unwrap();
    assert_eq!(persisted, Some(allocated));
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn library_reload_schedules_row_height_measurement() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = geometry_fixture(None);

    let scheduled_before = track_list
        .shared
        .diagnostic_trail
        .snapshot()
        .into_iter()
        .filter(|line| line.contains(" GeometryMeasurementScheduled "))
        .count();
    reload(&track_list.shared);
    let scheduled = track_list.shared.diagnostic_trail.snapshot();
    let scheduled_after = scheduled
        .iter()
        .filter(|line| line.contains(" GeometryMeasurementScheduled "))
        .count();

    assert_eq!(scheduled_after, scheduled_before + 1, "{scheduled:#?}");
    assert!(scheduled.iter().any(|line| {
        line.contains(" GeometryMeasurementScheduled ")
            && line.contains("rows=200")
            && line.contains("sections=0")
    }));
    window.close();
}
