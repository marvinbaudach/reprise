//! BROWSE-4 display tests: revealing a track inside the view that is already
//! open must leave the viewport at the revealed row.
//!
//! The player bar's title link routes through `library_shell::route_to_place`,
//! which re-selects the same sidebar source (a `reload()` that preserves the
//! *old* viewport) and only then restores the router's place (whose anchor is
//! the loaded track). Both halves write the same adjustment, so the second one
//! has to win — otherwise the jump is visible for a frame and then pulled back
//! to where the user came from.

use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use reprise_core::browser::{
    BrowserPlace, LibraryScope, TrackAnchor, TrackCollection, TrackFocus,
};
use reprise_core::view_source::ViewSource;

use crate::ui::track_list::{reload_restore, TrackList};

/// Comfortably past `track_list_reload::SCROLL_ADJUSTMENT_HOLD`, so a hold
/// that is still guarding the old position has had every chance to pull the
/// viewport back before the assertion reads it.
const PAST_THE_SCROLL_HOLD: Duration = Duration::from_millis(500);

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

/// The scroll value the revealed track's anchor asks for, against the live
/// geometry — the same computation the restore path itself performs.
fn anchor_target(track_list: &TrackList, track_id: i64) -> Option<f64> {
    let adjustment = track_list.shared.column_view.vadjustment()?;
    let ids = track_list.shared.current_view_ids();
    let height = adjustment.upper() / ids.len() as f64;
    reload_restore::scroll_target(
        Some((track_id, 0.0)),
        &ids,
        height,
        adjustment.page_size(),
    )
}

/// What the player bar's title link does while Music is already open and the
/// user has scrolled somewhere else: the sidebar re-selects the same source,
/// then the router restores the place whose anchor is the loaded track.
fn reveal_track_like_the_title_link(track_list: &TrackList, track_id: i64) {
    track_list.set_source(ViewSource::Library);
    let mut state = track_list
        .browser_place()
        .track_state()
        .expect("the library place must carry track view state")
        .clone();
    state.anchor = Some(TrackAnchor::new(track_id, 0.0));
    state.selected_ids = vec![track_id];
    state.focus = TrackFocus::Track(track_id);
    assert!(track_list.restore_browser_place(&BrowserPlace::tracks(
        TrackCollection::Library(LibraryScope::All),
        state,
    )));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_4_the_title_link_leaves_the_viewport_at_the_revealed_track() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = synthetic_track_list(200);

    let position = 150;
    track_list
        .shared
        .column_view
        .scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > adjustment.page_size() * 2.0
    });
    let before = adjustment.value();
    assert!(
        before > 0.0,
        "precondition: the user is somewhere else in the list"
    );

    let revealed_id = track_list.shared.model.track_at(10).unwrap().id;
    reveal_track_like_the_title_link(&track_list, revealed_id);
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);

    let expected = anchor_target(&track_list, revealed_id)
        .expect("a 200-row list in a 320px window must have scrollable geometry");
    assert!(
        (adjustment.value() - expected).abs() < 1.0,
        "the reveal was pulled back: actual {}, expected {expected}, came from {before}",
        adjustment.value()
    );
    let revealed_position = track_list
        .shared
        .current_view_ids()
        .iter()
        .position(|id| *id == revealed_id)
        .unwrap() as u32;
    assert!(
        track_list.shared.selection.is_selected(revealed_position),
        "the revealed track must stay selected"
    );

    window.close();
}
