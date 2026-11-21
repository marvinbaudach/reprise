//! START-4a display test: the selection a restored place carries survives a
//! start that also presents a greeting, and the reload a startup scan's
//! reconcile triggers.
//!
//! Included as a sibling of `start_restore_tests` inside
//! `current_track_selection`; it shares that module's silent playback backend.

use std::rc::Rc;
use std::time::Duration;

use reprise_core::browser::{BrowserPlace, LibraryScope, TrackCollection, TrackViewState};
use reprise_core::queue::QueueSnapshot;
use reprise_core::up_next::UpNextQueue;

use super::start_restore_tests::SilentPlayback;
use super::*;

/// Seeds `rows` synthetic tracks with ids `1..=rows`.
fn seed_synthetic_tracks(conn: &reprise_core::db::Db, rows: i64) {
    let fixture_conn = crate::test_db::connection(conn);
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
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    seed_synthetic_tracks(&conn, 12);
    let track_list = Rc::new(TrackList::new_for_startup(
        conn.clone(),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ));
    let controller = crate::ui::playback::test_support::controller_with_db(
        test_root.path(),
        conn,
        Box::new(SilentPlayback),
    );
    wire(Some(&controller), &track_list);
    controller.set_random_start_chooser_for_test(|_| Ok(vec![GREETING, 5]));
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
            repeat: reprise_core::queue::Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        None,
        None,
    );
    controller.notify_restored_current_track();
    assert_eq!(
        track_list.shared.playing_track_id.get(),
        Some(GREETING),
        "precondition: the startup greeting is the loaded track"
    );

    let startup_place = BrowserPlace::tracks(
        TrackCollection::Library(LibraryScope::All),
        TrackViewState {
            selected_ids: vec![RESTORED],
            ..TrackViewState::default()
        },
    );
    assert!(track_list.restore_browser_place(&startup_place));
    track_list.finish_startup_load(&startup_place);
    track_list.center_loaded_track();
    crate::ui::test_settle::settle_for(Duration::from_millis(300));
    assert_eq!(
        saved_selected_ids(&track_list),
        vec![RESTORED],
        "the greeting replaced the selection the restored place carried"
    );

    track_list.reload();
    crate::ui::test_settle::settle_for(Duration::from_millis(300));
    assert_eq!(saved_selected_ids(&track_list), vec![RESTORED]);
    assert_eq!(
        track_list.shared.playing_track_id.get(),
        Some(GREETING),
        "the greeting keeps its marker"
    );

    window.close();
}
