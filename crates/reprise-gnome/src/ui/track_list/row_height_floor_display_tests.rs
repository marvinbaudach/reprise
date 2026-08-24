//! The one thing `ROW_MIN_HEIGHT` must never do: claim a row is taller than it
//! renders.
//!
//! It is the height `ListGeometry` assumes before a settled frame has measured
//! one, and the centred reveal seeds the adjustment's range from it. Assume too
//! much and the seed and GTK's own anchor place the same row at two different
//! offsets, each overwriting the other — the viewport then never settles in one
//! move. #660 raised the token from 28 to 36 against rows that render at 34 and
//! turned eight viewport tests red at once, none of them anywhere near this
//! constant.
//!
//! Assuming too *little* is safe: the first settled frame replaces the
//! assumption with the measurement. So this asserts one direction only.

use std::rc::Rc;

use gtk4::prelude::*;

const TRACK_COUNT: i64 = 200;

/// The tallest realized `ColumnViewRow`, or `None` before any is allocated.
fn tallest_row_height(column_view: &gtk4::ColumnView) -> Option<i32> {
    let mut tallest: Option<i32> = None;
    let mut pending = vec![column_view.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = pending.pop() {
        if widget.type_().name().contains("ColumnViewRow") && widget.height() > 0 {
            tallest = Some(tallest.map_or(widget.height(), |seen| seen.max(widget.height())));
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    tallest
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_assumed_row_height_never_exceeds_the_rendered_one() {
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

    let track_list = super::TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();

    let column_view = track_list.shared.column_view.clone();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        tallest_row_height(&column_view).is_some()
    });
    let rendered =
        tallest_row_height(&column_view).expect("the list must allocate at least one row");
    let assumed = crate::ui::style::tokens::ROW_MIN_HEIGHT;

    assert!(
        assumed <= rendered,
        "the assumed row height is above the rendered one: ROW_MIN_HEIGHT={assumed}, \
         rendered={rendered}. The centred reveal seeds the scroll range from the \
         assumption, so a row placed at `position * {assumed}` disagrees with GTK's \
         anchor at `position * {rendered}` and the viewport takes two moves instead \
         of one."
    );
    window.close();
}
