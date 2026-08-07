//! Visible proof for SEARCH-9's viewport half: a typed query reads from the
//! top, and clearing it returns to where the search began. The rule-named
//! tests live in `track_list_reload.rs` and are display-free by design; these
//! need a real `ColumnView` with a real allocation and are `#[ignore]`d.

use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::track_list::TrackList;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn typed_search_reads_from_the_top_and_clearing_comes_back() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=200 {
        let title = if (150..=170).contains(&id) {
            format!("Match Track {id:03}")
        } else {
            format!("Other Track {id:03}")
        };
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (id, format!("/synthetic/{id:03}.flac"), title),
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let track_list = TrackList::new(
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

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    // A freshly presented `ColumnView` reports no usable geometry until it has
    // been allocated, and `upper` then still equals the page size — a scroll
    // written before that point is clamped straight back to zero, and the
    // precondition below fails for a reason that has nothing to do with what
    // this test is about. Pumping the loop until the adjustment can actually
    // hold a value is the difference between this test being deterministic and
    // it passing whenever the allocation happens to win the race.
    for _ in 0..200 {
        while gtk4::glib::MainContext::default().iteration(false) {}
        if adjustment.upper() > adjustment.page_size() {
            break;
        }
    }
    adjustment.set_value(1200.0);
    while gtk4::glib::MainContext::default().iteration(false) {}
    let departed_from = adjustment.value();
    assert!(
        departed_from > 0.0,
        "the test must start away from the top, else it proves nothing \
         (upper {}, page {})",
        adjustment.upper(),
        adjustment.page_size()
    );

    track_list.set_filter("Match");
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(
        adjustment.value(),
        0.0,
        "a typed query reads from the top of its results"
    );

    track_list.set_filter("");
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert!(
        (adjustment.value() - departed_from).abs() < 40.0,
        "clearing returns within a row of where the search began: expected \
         about {departed_from}, got {}",
        adjustment.value()
    );

    window.close();
}
