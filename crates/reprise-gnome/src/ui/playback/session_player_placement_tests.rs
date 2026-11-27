//! Display-backed regressions for who may claim startup's row placement: the
//! greeting dismissal and MPRIS Play (START-4, START-4a, NAV-10b).

use std::cell::RefCell;
use std::rc::Rc;

use reprise_core::media_integration::MprisCommand;
use reprise_core::queue::{QueueSnapshot, Repeat};
use reprise_core::up_next::{QueueItem, UpNextQueue};

use super::session_player_tests::{RecordingPlayback, SilentPlayback};
use super::test_support::controller_with_db;
use crate::ui::current_track_selection::CurrentTrackChange;

/// START-4a left the greeting unplaced; Next or Previous discarding it must
/// not turn the restored item into a placed one.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_4a_dismissing_the_greeting_does_not_place_an_unplaced_row() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/restored.flac', 'Restored', 'Artist', 120000, 0);
             INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (9, '/music/greeting.flac', 'Greeting', 'Artist', 120000, 0);",
        )
        .unwrap();
    let controller = controller_with_db(test_root.path(), conn, Box::new(SilentPlayback));
    controller.set_random_start_chooser_for_test(|_| Ok(vec![9, 7]));
    controller.restore_session_queue(
        QueueSnapshot {
            ids: vec![7],
            order: vec![0],
            position: Some(0),
            repeat: Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        None,
        None,
    );
    controller.forget_restored_placement();

    assert!(controller.dismiss_random_start_greeting());

    assert!(
        !controller.restored_placement_intact.get(),
        "dismissing the greeting claimed a placement startup never made"
    );
}

/// The startup-placed Play Next item is where the view already is, so MPRIS
/// Play only starts the audio, exactly like the bar's Play.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_4a_mpris_play_of_a_placed_restored_item_does_not_reveal_it_again() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (8, '/music/play-next.flac', 'Play Next', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    let controller = controller_with_db(
        test_root.path(),
        conn,
        Box::new(RecordingPlayback {
            played_paths: Rc::new(RefCell::new(Vec::new())),
        }),
    );
    controller.restore_session_queue(
        QueueSnapshot {
            ids: Vec::new(),
            order: Vec::new(),
            position: None,
            repeat: Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        Some(QueueItem::Track(8)),
        None,
    );
    assert!(
        controller.restored_placement_intact.get(),
        "precondition: startup placed the restored item"
    );
    let changes = Rc::new(RefCell::new(Vec::new()));
    let changes_for_callback = changes.clone();
    controller.add_on_current_track_changed(move |_, _, change| {
        changes_for_callback.borrow_mut().push(change);
    });

    controller.handle_mpris_command(MprisCommand::Play);

    assert_eq!(
        changes.borrow().as_slice(),
        [CurrentTrackChange::PlaybackStarted],
        "MPRIS Play re-centered a row startup had already placed"
    );
}
