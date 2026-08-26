//! NAV-19: choosing a different place in the sidebar centers the running
//! track in the table it opens — and leaves a view that does not list it
//! exactly where that view was.
//!
//! Both cases go through `TrackList::set_source`, which is the sidebar's own
//! entry point. Back and Forward reach `restore_browser_place` directly and
//! are deliberately not exercised here: BROWSE-2 keeps them restoring what
//! was left behind, and a test that made them centre would be recording the
//! rule's failure rather than the rule.

use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use reprise_core::view_source::ViewSource;

use crate::ui::track_list::TrackList;

/// Rows and window height chosen so the library scrolls several screens: a
/// centred row has to be a deliberate placement, not the only place it fits.
const ROWS: i64 = 200;

/// The track left out of the seven-day window, and therefore the one
/// "Recently Added" does not list.
const OLD_TRACK: i64 = 90;

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// A library of `ROWS` tracks, all added just now except [`OLD_TRACK`], which
/// is dated well outside the window "Recently Added" asks for.
fn two_source_track_list() -> (Rc<TrackList>, gtk4::Window, gtk4::Adjustment) {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    let fresh = now_seconds();
    for id in 1..=ROWS {
        let added_at = if id == OLD_TRACK { 0 } else { fresh };
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', ?4)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
                added_at,
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
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        track_list.shared.model.n_items() == ROWS as u32
            && adjustment.upper() > adjustment.page_size()
    });
    (track_list, window, adjustment)
}

/// Where the arithmetic centre for `track_id` lies in the view as it stands.
fn centered_target(track_list: &TrackList, track_id: i64, adjustment: &gtk4::Adjustment) -> f64 {
    let current_ids = track_list.shared.current_view_ids();
    let row_height =
        super::super::display_test_geometry::measured_row_height(&track_list.shared.column_view)
            .expect("the settled source must expose measured rows");
    crate::ui::track_list::reload_restore::flat_list_centered_track_scroll_target(
        Some(track_id),
        &current_ids,
        row_height,
        adjustment.page_size(),
    )
    .expect("the opened view must have a centered target for the running track")
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_19_switching_source_centers_the_running_track() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window, adjustment) = two_source_track_list();

    let playing_id = track_list.shared.model.track_at(140).unwrap().id;
    assert_ne!(
        playing_id, OLD_TRACK,
        "this case needs a track both sources list"
    );
    track_list.shared.playing_track_id.set(Some(playing_id));
    let selected_before = track_list.shared.selection.selection().size();

    track_list.set_source(ViewSource::RecentlyAdded);
    crate::ui::test_settle::settle_for(Duration::from_millis(500));

    let expected = centered_target(&track_list, playing_id, &adjustment);
    let row_height =
        super::super::display_test_geometry::measured_row_height(&track_list.shared.column_view)
            .expect("the settled source must expose measured rows");
    // Half a row, because the restore lands on the row edge nearest the
    // centre — the only value GTK's own anchor reproduces. See
    // `centered_scroll_restore::centered_anchor`.
    assert!(
        (adjustment.value() - expected).abs() <= row_height / 2.0,
        "switching source must centre the running track: actual {}, expected \
         {expected} (within {})",
        adjustment.value(),
        row_height / 2.0
    );
    assert_eq!(
        track_list.shared.selection.selection().size(),
        selected_before,
        "NAV-19 places the viewport and nothing else; the selection is the \
         view's own"
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_19_a_view_without_the_running_track_keeps_its_own_place() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window, adjustment) = two_source_track_list();

    // Give "Recently Added" a place of its own to remember, then leave it.
    track_list.set_source(ViewSource::RecentlyAdded);
    crate::ui::test_settle::settle_for(Duration::from_millis(300));
    assert_eq!(
        track_list.shared.model.n_items(),
        ROWS as u32 - 1,
        "precondition: the old track is outside the seven-day window"
    );
    adjustment.set_value(1_200.0);
    crate::ui::test_settle::settle_for(Duration::from_millis(300));
    let departed_from = adjustment.value();
    assert!(departed_from > 0.0, "precondition: that view scrolled");

    track_list.set_source(ViewSource::Library);
    crate::ui::test_settle::settle_for(Duration::from_millis(300));
    track_list.shared.playing_track_id.set(Some(OLD_TRACK));

    // Back to the view that does not list the running track.
    track_list.set_source(ViewSource::RecentlyAdded);
    crate::ui::test_settle::settle_for(Duration::from_millis(500));

    let row_height =
        super::super::display_test_geometry::measured_row_height(&track_list.shared.column_view)
            .expect("the settled source must expose measured rows");
    assert!(
        (adjustment.value() - departed_from).abs() <= row_height,
        "a view that does not list the running track keeps its remembered \
         place: actual {}, remembered {departed_from}",
        adjustment.value()
    );

    window.close();
}
