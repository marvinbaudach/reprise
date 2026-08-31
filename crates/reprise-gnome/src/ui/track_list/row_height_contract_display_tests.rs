use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use reprise_core::library::settings;

use super::*;

const TRACK_COUNT: i64 = 1_201;
const REVEAL_POSITION: u32 = 1_100;
const RESTING_SETTLE: Duration = Duration::from_millis(500);

fn large_flat_list() -> (Rc<TrackList>, gtk4::Window) {
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());
    let conn = crate::test_db::open().unwrap();
    assert_eq!(settings::get_row_height(&conn).unwrap(), None);
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=TRACK_COUNT {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/row-height-contract/{id:04}.flac"),
                format!("Track {id:04}"),
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let track_list = Rc::new(TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ));
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    (track_list, window)
}

fn all_realized_rows_reached_natural_height(column_view: &gtk4::ColumnView) -> bool {
    let rows = display_test_geometry::realized_row_measurements(column_view);
    !rows.is_empty() && rows.iter().all(|(allocated, natural)| allocated >= natural)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn row_height_contract_agrees_across_widgets_adjustment_persistence_and_reveal() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = large_flat_list();
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();

    let initially_settled =
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            adjustment.upper() > adjustment.page_size()
                && all_realized_rows_reached_natural_height(&track_list.shared.column_view)
                && display_test_geometry::measured_row_height(&track_list.shared.column_view)
                    .is_some()
        });
    assert!(
        initially_settled,
        "rows never reached natural height: {:?}",
        display_test_geometry::realized_row_measurements(&track_list.shared.column_view)
    );
    track_list.shared.column_view.scroll_to(
        REVEAL_POSITION,
        None,
        gtk4::ListScrollFlags::FOCUS,
        None,
    );
    assert!(crate::ui::test_settle::settle_until(
        crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
        || adjustment.value() > adjustment.page_size() * 2.0
    ));
    adjustment.emit_by_name::<()>("changed", &[]);
    track_list
        .shared
        .list_geometry_cache
        .seed_measured_row_height(30.0);
    settings::set_row_height(&track_list.shared.conn, Some(30.0)).unwrap();
    let settled_adjustment_height = adjustment.upper() / TRACK_COUNT as f64;
    let poisoned_height = crate::ui::list_geometry::RowHeight::new(
        settings::get_row_height(&track_list.shared.conn)
            .unwrap()
            .unwrap(),
    );
    let pre_reload_layout =
        track_list_geometry::layout(&track_list.shared, poisoned_height, TRACK_COUNT as usize)
            .expect("the allocated list has layout geometry");
    assert_eq!(
        pre_reload_layout.row_height().pixels(),
        settled_adjustment_height,
        "layout reused the poisoned cache instead of GTK's settled range"
    );
    reload(&track_list.shared);
    assert!(crate::ui::test_settle::settle_until(
        crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
        || {
            all_realized_rows_reached_natural_height(&track_list.shared.column_view)
                && display_test_geometry::measured_row_height(&track_list.shared.column_view)
                    .is_some()
                && settings::get_row_height(&track_list.shared.conn)
                    .unwrap()
                    .is_some()
        }
    ));

    let widget_height = display_test_geometry::measured_row_height(&track_list.shared.column_view)
        .expect("settled rows supply an independent modal height");
    let adjustment_height = adjustment.upper() / TRACK_COUNT as f64;
    assert!(
        (widget_height - adjustment_height).abs()
            < crate::ui::list_geometry::ROW_HEIGHT_AGREEMENT_EPSILON,
        "widget and adjustment evidence disagree: widget={widget_height}, \
         adjustment={adjustment_height}, upper={}",
        adjustment.upper()
    );
    assert_eq!(
        settings::get_row_height(&track_list.shared.conn).unwrap(),
        Some(adjustment_height)
    );
    crate::ui::test_settle::settle_for(RESTING_SETTLE);

    assert!(track_reveal::reveal_position(
        &track_list.shared,
        REVEAL_POSITION,
        8,
        track_reveal::RevealMotion::Glide,
    ));
    let layout = crate::ui::list_geometry_layout::ListLayout::rows_only(
        crate::ui::list_geometry::RowHeight::new(adjustment_height).unwrap(),
    );
    let expected = layout
        .centered_value(
            REVEAL_POSITION,
            TRACK_COUNT as usize,
            adjustment.page_size(),
        )
        .unwrap();
    let reached_expected =
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            (adjustment.value() - expected).abs() < 1.0
        });
    assert!(
        reached_expected,
        "deep reveal never reached its quotient-derived centre: actual={}, expected={expected}, \
         upper={}, page={}",
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size()
    );
    crate::ui::test_settle::settle_for(RESTING_SETTLE);
    assert!(
        (adjustment.value() - expected).abs() < 1.0,
        "deep reveal did not rest at the quotient-derived centre: actual={}, expected={expected}",
        adjustment.value()
    );
    window.close();
}
