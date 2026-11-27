//! START-4a display tests: the selection a restored place carries survives a
//! start that also presents a greeting, and the reload a startup scan's
//! reconcile triggers.
//!
//! Included as a sibling of `start_restore_tests` inside
//! `current_track_selection`; it reuses that module's centring helpers.

use std::rc::Rc;
use std::time::Duration;

use reprise_core::browser::{
    BrowserPlace, LibraryScope, TrackAnchor, TrackCollection, TrackViewState,
};
use reprise_core::playback::{AudioEffects, PlaybackBackend, PlaybackError, PlaybackState};
use reprise_core::queue::{QueueSnapshot, Repeat};
use reprise_core::up_next::UpNextQueue;

use super::start_restore_tests::{centered_value, centering_tolerance};
use super::*;

const SETTLE: Duration = Duration::from_millis(300);

/// A backend that accepts every start, for tests that press Play.
struct AcceptingPlayback;

impl PlaybackBackend for AcceptingPlayback {
    fn play(&self, _: reprise_core::playback::PlaybackItem<'_>) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        Ok(PlaybackState::Paused)
    }

    fn seek_to(&self, _: i64) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn set_volume(&self, _: f64) {}

    fn set_audio_effects(&self, _: AudioEffects) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn set_next(&self, _: Option<reprise_core::playback::PlaybackItem<'_>>) {}

    fn set_transition(&self, _: reprise_core::library::settings::TrackTransition, _: u8) {}
}

/// A started app: track list and player wired as `wire_session_restore` wires
/// them, the greeting armed and its row marked, the restored place routed.
struct Startup {
    track_list: Rc<TrackList>,
    controller: Rc<PlayerController>,
    window: gtk4::Window,
    _root: tempfile::TempDir,
}

/// Seeds `rows` synthetic tracks, then runs the real startup order up to, but
/// not including, `place_startup_item`: deferred load, session queue restore
/// with `greeting` as the random pick, the routed `state`.
fn start_until_placement(rows: i64, greeting: i64, state: TrackViewState) -> Startup {
    let root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
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
    let track_list = Rc::new(TrackList::new_for_startup(
        conn.clone(),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ));
    let controller = crate::ui::playback::test_support::controller_with_db(
        root.path(),
        conn,
        Box::new(AcceptingPlayback),
    );
    wire(Some(&controller), &track_list);
    controller.set_random_start_chooser_for_test(move |_| Ok(vec![greeting]));
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();

    controller.restore_session_queue(
        QueueSnapshot {
            ids: Vec::new(),
            order: Vec::new(),
            position: None,
            repeat: Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        None,
        None,
    );
    controller.notify_restored_current_track();
    assert_eq!(
        track_list.shared.playing_track_id.get(),
        Some(greeting),
        "precondition: the startup greeting is the loaded track"
    );

    let startup_place = BrowserPlace::tracks(TrackCollection::Library(LibraryScope::All), state);
    assert!(track_list.restore_browser_place(&startup_place));
    track_list.finish_startup_load(&startup_place);
    Startup {
        track_list,
        controller,
        window,
        _root: root,
    }
}

/// The selected ids a saved session would carry for the visible place.
fn saved_selected_ids(track_list: &TrackList) -> Vec<i64> {
    track_list
        .browser_place()
        .track_state()
        .expect("the track list must expose its track view state")
        .selected_ids
        .clone()
}

fn selecting(ids: &[i64]) -> TrackViewState {
    TrackViewState {
        selected_ids: ids.to_vec(),
        ..TrackViewState::default()
    }
}

/// Walks the real startup order from the starting state (the Reachability
/// rule): deferred load, routed place, greeting, centring, then the reload a
/// startup scan's reconcile triggers, then the value the close handler saves.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_4a_a_restored_selection_survives_the_greeting_and_a_reconcile() {
    const RESTORED: i64 = 7;
    const GREETING: i64 = 3;
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let start = start_until_placement(12, GREETING, selecting(&[RESTORED]));
    let track_list = &start.track_list;
    track_list.place_startup_item(Some(&start.controller));
    crate::ui::test_settle::settle_for(SETTLE);
    assert_eq!(
        saved_selected_ids(track_list),
        vec![RESTORED],
        "the greeting replaced the selection the restored place carried"
    );

    track_list.reload();
    crate::ui::test_settle::settle_for(SETTLE);
    assert_eq!(saved_selected_ids(track_list), vec![RESTORED]);
    assert_eq!(
        track_list.shared.playing_track_id.get(),
        Some(GREETING),
        "the greeting keeps its marker"
    );

    start.window.close();
}

/// START-4a's viewport promise: the place keeps the viewport it was saved at,
/// not the greeting's centred one.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_4a_the_restored_viewport_stays_where_the_place_was_saved() {
    const ANCHOR: i64 = 40;
    const GREETING: i64 = 90;
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let state = TrackViewState {
        anchor: Some(TrackAnchor::new(ANCHOR, 0.0)),
        selected_ids: vec![ANCHOR + 1],
        ..TrackViewState::default()
    };
    let start = start_until_placement(100, GREETING, state);
    let track_list = &start.track_list;
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > 0.0
    });
    crate::ui::test_settle::settle_for(SETTLE);
    let saved_value = adjustment.value();
    assert!(
        saved_value > 0.0,
        "precondition: the saved anchor scrolled the list away from the top"
    );

    track_list.place_startup_item(Some(&start.controller));
    crate::ui::test_settle::settle_for(SETTLE);

    assert!(
        (adjustment.value() - saved_value).abs() < 1.0,
        "the greeting moved the restored viewport: saved {saved_value}, now {}",
        adjustment.value()
    );

    start.window.close();
}

/// A saved id the library no longer holds leaves the place without a
/// selection, so the greeting is the sole selection as START-4 says.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_4a_a_saved_id_that_no_longer_exists_leaves_the_greeting_selected() {
    const GONE: i64 = 999;
    const GREETING: i64 = 3;
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let start = start_until_placement(12, GREETING, selecting(&[GONE]));
    let track_list = &start.track_list;
    track_list.place_startup_item(Some(&start.controller));
    crate::ui::test_settle::settle_for(SETTLE);

    assert_eq!(saved_selected_ids(track_list), vec![GREETING]);

    start.window.close();
}

/// Play from Stopped rests on the start having placed the row. When the
/// restored selection kept the start from placing it, Play must reveal it.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_4a_play_from_stopped_reveals_a_row_the_start_did_not_place() {
    const GREETING: i64 = 90;
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let start = start_until_placement(100, GREETING, selecting(&[2]));
    let track_list = &start.track_list;
    track_list.place_startup_item(Some(&start.controller));
    crate::ui::test_settle::settle_for(SETTLE);
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    let expected = centered_value(track_list, 89).expect("a 100-row list must have geometry");
    assert!(
        (adjustment.value() - expected).abs() > centering_tolerance(track_list),
        "precondition: the greeting row starts off-screen"
    );

    start.controller.toggle_pause();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        let tolerance = centering_tolerance(track_list);
        centered_value(track_list, 89)
            .is_some_and(|target| (adjustment.value() - target).abs() <= tolerance)
    });

    let expected = centered_value(track_list, 89).unwrap();
    assert!(
        (adjustment.value() - expected).abs() <= centering_tolerance(track_list),
        "Play did not reveal the playing row: actual {}, expected {expected}",
        adjustment.value()
    );
    assert_eq!(
        saved_selected_ids(track_list),
        vec![2],
        "Play keeps the selection"
    );

    start.window.close();
}

/// A row startup placed is the one the first Play may skip its reveal for, so
/// placing it must leave the one-shot set; a second scroll on that Play is the
/// #1369 bug.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_4a_a_placed_row_keeps_the_placement_flag_for_the_first_play() {
    const GREETING: i64 = 3;
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let start = start_until_placement(12, GREETING, TrackViewState::default());
    let track_list = &start.track_list;

    track_list.place_startup_item(Some(&start.controller));
    crate::ui::test_settle::settle_for(SETTLE);

    assert_eq!(
        saved_selected_ids(track_list),
        vec![GREETING],
        "precondition: startup placed the greeting row"
    );
    assert!(
        start.controller.restored_placement_intact.get(),
        "a placed row lost its placement flag, so the first Play would scroll it again"
    );

    start.window.close();
}

/// A loaded track the restored view does not list is not placed, but there is
/// no row for a Play to reveal either, so the flag stays set.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_4a_a_greeting_outside_the_view_keeps_the_placement_flag() {
    const GREETING: i64 = 3;
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let state = TrackViewState {
        search: "Track 001".to_owned(),
        ..TrackViewState::default()
    };
    let start = start_until_placement(12, GREETING, state);
    let track_list = &start.track_list;
    assert!(
        !track_list.shared.current_view_ids().contains(&GREETING),
        "precondition: the restored view does not list the greeting"
    );

    track_list.place_startup_item(Some(&start.controller));
    crate::ui::test_settle::settle_for(SETTLE);

    assert!(
        saved_selected_ids(track_list).is_empty(),
        "startup selected a row in a view that does not list the greeting"
    );
    assert!(start.controller.restored_placement_intact.get());

    start.window.close();
}
