//! START-1 display tests: a normal start marks the loaded track like a paused
//! song and centers it, without touching selection or focus.
//!
//! Included as a child module of `current_track_selection` (see the bottom of
//! that file) for two reasons: the tests drive its private
//! `update_current_track`, and that file is already close to the project's
//! 800-line ceiling.

use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

use super::*;

/// A realised track table over `rows` synthetic tracks, in a window big
/// enough to scroll — centering needs `upper > page_size` to mean anything.
fn synthetic_track_list(rows: i64) -> (TrackList, gtk4::Window) {
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
    let track_list = TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
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

fn centered_value(track_list: &TrackList, position: u32) -> Option<f64> {
    let adjustment = track_list.shared.column_view.vadjustment()?;
    scroll_center::centered_scroll_value(
        position,
        track_list.shared.model.n_items(),
        adjustment.upper(),
        adjustment.page_size(),
    )
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_1_loaded_track_is_centered_and_marked_paused() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = synthetic_track_list(100);

    let position = 60_u32;
    let track_id = track_list.shared.model.track_at(position).unwrap().id;

    // Exactly what a normal start does, in order: the session restore marks
    // the loaded track, then the startup routing hands the viewport over.
    track_list.update_current_track(track_id, None, CurrentTrackChange::SessionRestore);
    track_list.center_loaded_track();

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        centered_value(&track_list, position)
            .is_some_and(|target| (adjustment.value() - target).abs() < 0.5)
    });

    let expected = centered_value(&track_list, position)
        .expect("a 100-row list in a 320px window must have centering geometry");
    assert!(
        (adjustment.value() - expected).abs() < 0.5,
        "a normal start must center the loaded track: actual {}, expected {expected}",
        adjustment.value()
    );
    assert!(
        track_list
            .shared
            .column_view
            .has_css_class("playback-paused"),
        "the restored row must look like a paused song, not a running one"
    );
    assert_eq!(
        track_list.shared.selection.selection().size(),
        0,
        "START-1 marks and centers; it never takes the selection (NAV-10a)"
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_1_absent_loaded_track_leaves_the_list_at_the_top() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = synthetic_track_list(100);

    // A loaded id the library view does not contain — the session ended on a
    // podcast episode, or the track was removed since.
    track_list.shared.playing_track_id.set(Some(9_999));
    track_list.center_loaded_track();
    crate::ui::test_settle::settle_for(Duration::from_millis(200));

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    assert!(
        adjustment.value().abs() < 0.5,
        "an unresolvable loaded track must leave the list at the top, actual {}",
        adjustment.value()
    );

    window.close();
}
