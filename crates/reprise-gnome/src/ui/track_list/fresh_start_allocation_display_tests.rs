use super::*;

const TRACK_COUNT: i64 = 2_132;
const ANCHOR_POSITION: usize = 944;
const CAPTURED_ROW_HEIGHT: f64 = 53.0;

fn allocated_row_count(column_view: &gtk4::ColumnView) -> usize {
    let mut allocated = 0;
    let mut pending = vec![column_view.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = pending.pop() {
        if widget.type_().name().contains("ColumnViewRow") && widget.height() > 0 {
            allocated += 1;
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    allocated
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
fn fresh_start_deep_restore_allocates_visible_rows_at_the_anchor() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=TRACK_COUNT {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/synthetic/{id:04}.flac"),
                format!("Track {id:04}"),
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

    let current_ids = (1..=TRACK_COUNT).collect::<Vec<_>>();
    let anchor_id = current_ids[ANCHOR_POSITION];
    let captured = reload_restore::capture_with_row_height(
        vec![anchor_id],
        Some((anchor_id, 0.0)),
        RowHeight::new(CAPTURED_ROW_HEIGHT),
    );
    reload_with_anchor_and_viewport(
        &track_list.shared,
        &captured,
        ReloadViewport::PreserveAnchor,
        None,
        Some(current_ids),
    );

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    let target = ANCHOR_POSITION as f64 * CAPTURED_ROW_HEIGHT;
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        (adjustment.value() - target).abs() <= CAPTURED_ROW_HEIGHT
    });
    let allocated = allocated_row_count(&track_list.shared.column_view);

    assert!(
        has_row_intersecting_viewport(&track_list.shared.column_view),
        "fresh-start ColumnView has no allocated row intersecting its viewport"
    );
    assert!(
        allocated > 1,
        "fresh-start ColumnView allocated only {allocated} row(s)"
    );
    assert!(
        (adjustment.value() - target).abs() <= CAPTURED_ROW_HEIGHT,
        "fresh-start restore missed its anchor: target={target}, value={}, page={}",
        adjustment.value(),
        adjustment.page_size()
    );
    window.close();
}
