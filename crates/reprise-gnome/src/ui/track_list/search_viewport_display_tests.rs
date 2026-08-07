//! Visible proof for SEARCH-9's viewport half: a typed query reads from the
//! top, and clearing it returns to where the search began. The rule-named
//! tests live in `track_list_reload.rs` and are display-free by design; these
//! need a real `ColumnView` with a real allocation and are `#[ignore]`d.

use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::track_list::TrackList;

/// Lets the frame clock run for a moment so GTK can allocate. `iteration(false)`
/// alone never blocks, so the clock never ticks and widgets stay unallocated —
/// mirrors `preferences_window::tests::settle_for`.
fn settle() {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || quit.quit());
    main_loop.run();
}

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
    settle();

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    // A freshly presented `ColumnView` reports no usable geometry until it has
    // been allocated, and a scroll written before that point is clamped
    // straight back to zero — the precondition below then fails for a reason
    // that has nothing to do with what this test is about.
    //
    // Draining with `iteration(false)` is not enough and looked like it was:
    // it never blocks, so the frame clock that drives allocation never gets a
    // turn, and the adjustment stayed at `upper 0, page 0`. Running a real
    // `MainLoop` briefly is what the preferences display tests already do
    // (`preferences_window::tests::settle_for`), and it is the difference
    // between this test being deterministic and passing whenever the
    // allocation happens to win the race.
    settle();
    for _ in 0..20 {
        if adjustment.upper() > adjustment.page_size() {
            break;
        }
        settle();
    }
    adjustment.set_value(1200.0);
    settle();
    let departed_from = adjustment.value();
    assert!(
        departed_from > 0.0,
        "the test must start away from the top, else it proves nothing \
         (upper {}, page {})",
        adjustment.upper(),
        adjustment.page_size()
    );

    track_list.set_filter("Match");
    settle();
    assert_eq!(
        adjustment.value(),
        0.0,
        "a typed query reads from the top of its results"
    );

    track_list.set_filter("");
    settle();
    assert!(
        (adjustment.value() - departed_from).abs() < 40.0,
        "clearing returns within a row of where the search began: expected \
         about {departed_from}, got {}",
        adjustment.value()
    );

    window.close();
}
