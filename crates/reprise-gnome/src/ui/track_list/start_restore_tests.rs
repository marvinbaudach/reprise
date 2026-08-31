//! START-3 display tests: a normal start marks, selects, and centers the
//! restored loaded track like a paused song without taking keyboard focus.
//!
//! Included as a child module of `current_track_selection` (see the bottom of
//! that file) for two reasons: the tests drive its private
//! `update_current_track`, and that file is already close to the project's
//! 800-line ceiling.

use std::rc::Rc;
use std::time::Duration;

use reprise_core::browser::{BrowserPlace, LibraryScope, TrackCollection, TrackViewState};

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

/// How far from the arithmetic centre a centered restore may land.
///
/// The restore places the viewport on a **row edge** — the one nearest the
/// centre — because that is the only kind of value GTK's own anchor
/// reproduces: `scroll_to` aligns a row with the top of the viewport, and a
/// value no anchor row explains is overwritten during the allocation pass that
/// follows a model swap (`centered_scroll_restore::centered_anchor`). Half a
/// row is therefore the honest bound, and it is what these tests hold the path
/// to. In this fixture's geometry the edge falls 0.5 px from the centre.
fn centering_tolerance(track_list: &TrackList) -> f64 {
    let adjustment = track_list
        .shared
        .column_view
        .vadjustment()
        .expect("a realized track list has a vertical adjustment");
    // Range-derived height only bounds permissible error; it is not the target oracle.
    adjustment.upper() / f64::from(track_list.shared.model.n_items()) / 2.0
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
fn start_3_loaded_track_is_selected_centered_and_marked_paused() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = synthetic_track_list(100);

    let position = 60_u32;
    let track_id = track_list.shared.model.track_at(position).unwrap().id;
    let sort = track_list
        .browser_place()
        .track_state()
        .expect("the synthetic table must expose track view state")
        .sort
        .clone();

    // Exactly what a normal start does, in order: session restore marks the
    // loaded track, then anchor-free library routing hands the viewport over.
    track_list.update_current_track(track_id, None, CurrentTrackChange::SessionRestore);
    let startup_place = BrowserPlace::tracks(
        TrackCollection::Library(LibraryScope::All),
        TrackViewState {
            sort,
            ..TrackViewState::default()
        },
    );
    assert!(track_list.restore_browser_place(&startup_place));
    track_list.center_loaded_track();

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        let tolerance = centering_tolerance(&track_list);
        centered_value(&track_list, position)
            .is_some_and(|target| (adjustment.value() - target).abs() <= tolerance)
    });

    let expected = centered_value(&track_list, position)
        .expect("a 100-row list in a 320px window must have centering geometry");
    let tolerance = centering_tolerance(&track_list);
    assert!(
        (adjustment.value() - expected).abs() <= tolerance,
        "a normal start must center the loaded track: actual {}, expected {expected} \
         (within {tolerance})",
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
        1,
        "START-3 gives the restored loaded track the sole selection"
    );
    assert!(track_list.shared.selection.is_selected(position));

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_3_absent_loaded_track_does_not_move_the_live_viewport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = synthetic_track_list(100);

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

    // A loaded id the library view does not contain — the session ended on a
    // podcast episode, or the track was removed since.
    track_list.shared.playing_track_id.set(Some(9_999));
    track_list.center_loaded_track();
    crate::ui::test_settle::settle_for(Duration::from_millis(200));

    assert!(
        (adjustment.value() - before).abs() < 1.0,
        "an unresolvable loaded track moved the viewport: before={before}, after={}",
        adjustment.value()
    );

    window.close();
}
