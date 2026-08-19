//! NAV-10b display test: a reload that lands mid-glide must not abort it.
//!
//! The follow-the-song glide yields to anything else that writes the
//! adjustment (`scroll_glide.rs`'s `foreign_write`) — which is right for a
//! user scroll and wrong for a reload's `AdjustmentHold`, whose whole job is
//! writing the position it captured. A library scan reloads the list in
//! bursts, so this collision is the everyday case, not a corner one.

use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::track_list::TrackList;

fn synthetic_track_list(rows: i64) -> (Rc<TrackList>, gtk4::Window) {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=rows {
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
    let track_list = Rc::new(TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ));
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        track_list
            .shared
            .column_view
            .vadjustment()
            .is_some_and(|adjustment| adjustment.upper() > adjustment.page_size())
    });
    (track_list, window)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_a_scan_reload_mid_glide_does_not_strand_the_follow() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    gtk4::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    let (track_list, window) = synthetic_track_list(200);

    // Somewhere in the middle, so the reload has a viewport worth preserving.
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::track_list::track_reveal::reveal_position(
        &track_list.shared,
        100,
        8,
        crate::ui::track_list::track_reveal::RevealMotion::Glide,
    );
    let (_, start) = crate::ui::scroll_center::centered_scroll_target(
        &track_list.shared.column_view,
        track_list.shared.model.n_items(),
        100,
    )
    .unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        (adjustment.value() - start).abs() < 1.0
    });

    // Playback moves on: near enough for a glide rather than a jump.
    let (_, target) = crate::ui::scroll_center::centered_scroll_target(
        &track_list.shared.column_view,
        track_list.shared.model.n_items(),
        115,
    )
    .unwrap();
    crate::ui::track_list::track_reveal::reveal_position(
        &track_list.shared,
        115,
        8,
        crate::ui::track_list::track_reveal::RevealMotion::Glide,
    );
    assert_eq!(
        track_list.shared.scroll_glide.destination(),
        Some(target),
        "precondition: the follow must be a glide, not a jump"
    );

    // A scan's reload arrives while the glide is still in flight.
    crate::ui::track_list::reload(&track_list.shared);
    let landed =
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            (adjustment.value() - target).abs() < 1.0
        });

    assert!(
        landed,
        "the reload stranded the glide: actual {}, target {target}, started at {start}",
        adjustment.value()
    );

    window.close();
}
