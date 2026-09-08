//! Display-backed startup restoration regressions.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::WidgetExt;
use reprise_core::media_integration::{MprisCommand, MprisPlaybackStatus};
use reprise_core::playback::{AudioEffects, PlaybackBackend, PlaybackError, PlaybackState};
use reprise_core::queue::{QueueSnapshot, Repeat};
use reprise_core::up_next::{QueueItem, UpNextQueue};

use super::test_support::controller_with_db;

struct SilentPlayback;

impl PlaybackBackend for SilentPlayback {
    fn play(&self, _: &str) -> Result<(), PlaybackError> {
        panic!("startup restore must not start playback")
    }

    fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
        panic!("startup restore must not start playback")
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        panic!("startup restore must not toggle playback")
    }

    fn seek_to(&self, _: i64) -> Result<(), PlaybackError> {
        panic!("startup restore must not seek")
    }

    fn set_volume(&self, _: f64) {}

    fn set_audio_effects(&self, _: AudioEffects) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        panic!("startup restore must not stop an inactive backend")
    }

    fn set_next(&self, _: Option<&str>) {}

    fn set_transition(&self, _: reprise_core::library::settings::TrackTransition, _: u8) {}
}

struct RecordingPlayback {
    played_paths: Rc<RefCell<Vec<String>>>,
}

impl PlaybackBackend for RecordingPlayback {
    fn play(&self, path: &str) -> Result<(), PlaybackError> {
        self.played_paths.borrow_mut().push(path.to_owned());
        Ok(())
    }

    fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
        panic!("the random greeting must start a library track")
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        panic!("the stopped greeting must start instead of toggling the pipeline")
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

    fn set_next(&self, _: Option<&str>) {}

    fn set_transition(&self, _: reprise_core::library::settings::TrackTransition, _: u8) {}
}

fn empty_snapshot() -> QueueSnapshot {
    QueueSnapshot {
        ids: Vec::new(),
        order: Vec::new(),
        position: None,
        repeat: Repeat::Off,
        shuffled: false,
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn random_greeting_preserves_the_curated_session_snapshot_and_origin() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/first.flac', 'First', 'Artist', 120000, 0);
             INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (8, '/music/second.flac', 'Second', 'Artist', 120000, 0);
             INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (9, '/music/greeting.flac', 'Greeting', 'Artist', 120000, 0);",
        )
        .unwrap();
    let controller = controller_with_db(test_root.path(), conn, Box::new(SilentPlayback));
    controller.set_random_start_chooser_for_test(|_| Ok(vec![9, 8, 7]));
    let curated = QueueSnapshot {
        ids: vec![7, 8],
        order: vec![1, 0],
        position: Some(1),
        repeat: Repeat::All,
        shuffled: true,
    };
    let origin = super::play_origin::PlayOrigin {
        place: reprise_core::browser::BrowserPlace::from(
            reprise_core::view_source::ViewSource::Playlist(42),
        ),
        label: "Road trip".into(),
    };

    controller.restore_session_queue(
        curated.clone(),
        UpNextQueue::default(),
        None,
        Some(origin.clone()),
    );

    assert_eq!(controller.session_queue_snapshot(), curated);
    assert_eq!(controller.current_play_origin(), Some(origin));
    assert_eq!(controller.pending_random_start_track_id(), Some(9));
    assert_eq!(controller.bar.title_label.text(), "Greeting");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn first_play_starts_the_track_shown_by_the_random_greeting() {
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
    let played_paths = Rc::new(RefCell::new(Vec::new()));
    let controller = controller_with_db(
        test_root.path(),
        conn,
        Box::new(RecordingPlayback {
            played_paths: played_paths.clone(),
        }),
    );
    controller.set_random_start_chooser_for_test(|_| Ok(vec![9, 7]));

    controller.restore_session_queue(
        QueueSnapshot {
            ids: vec![7],
            order: vec![0],
            position: Some(0),
            repeat: Repeat::All,
            shuffled: true,
        },
        UpNextQueue::default(),
        None,
        None,
    );
    assert_eq!(controller.bar.title_label.text(), "Greeting");

    controller.toggle_pause();

    assert_eq!(played_paths.borrow().as_slice(), ["/music/greeting.flac"]);
    let started = controller.session_queue_snapshot();
    assert_eq!(started.ids, vec![9, 7]);
    assert_eq!(started.repeat, Repeat::All);
    assert!(started.shuffled);
    assert_eq!(controller.pending_random_start_track_id(), None);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mpris_play_starts_the_greeting_instead_of_the_restored_track() {
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
    let played_paths = Rc::new(RefCell::new(Vec::new()));
    let controller = controller_with_db(
        test_root.path(),
        conn,
        Box::new(RecordingPlayback {
            played_paths: played_paths.clone(),
        }),
    );
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

    controller.handle_mpris_command(MprisCommand::Play);

    assert_eq!(played_paths.borrow().as_slice(), ["/music/greeting.flac"]);
    assert_eq!(controller.session_queue_snapshot().ids, vec![9, 7]);
    assert_eq!(controller.pending_random_start_track_id(), None);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mpris_play_leaves_a_surviving_restored_queue_without_a_current_item_untouched() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/survivor.flac', 'Survivor', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    let played_paths = Rc::new(RefCell::new(Vec::new()));
    let controller = controller_with_db(
        test_root.path(),
        conn,
        Box::new(RecordingPlayback {
            played_paths: played_paths.clone(),
        }),
    );
    controller.set_random_start_chooser_for_test(|_| Err(rusqlite::Error::InvalidQuery));
    controller.restore_session_queue(
        QueueSnapshot {
            ids: vec![7, 8],
            order: vec![0, 1],
            position: Some(1),
            repeat: Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        None,
        None,
    );
    assert_eq!(
        controller
            .stopped_play_target()
            .and_then(|target| target.item()),
        None
    );
    assert!(controller.has_playable_item());

    controller.handle_mpris_command(MprisCommand::Play);

    assert!(played_paths.borrow().is_empty());
    assert_eq!(controller.session_queue_snapshot().ids, vec![7]);
    assert_eq!(controller.session_queue_snapshot().position, None);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn startup_with_a_non_empty_library_shows_a_track_and_stays_stopped() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/greeting.flac', 'Greeting', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    let controller = controller_with_db(test_root.path(), conn, Box::new(SilentPlayback));

    controller.restore_session_queue(empty_snapshot(), UpNextQueue::default(), None, None);

    assert!(controller.session_queue_snapshot().ids.is_empty());
    assert_eq!(controller.pending_random_start_track_id(), Some(7));
    assert_eq!(controller.bar.title_label.text(), "Greeting");
    assert!(controller.bar.widget().is_sensitive());
    assert_eq!(controller.current_play_origin(), None);
    assert!(
        controller.restored_placement_intact.get(),
        "START-4 placed the greeting; its Play path bypasses this inert one-shot"
    );
    assert_eq!(
        controller.session_playback_status(),
        MprisPlaybackStatus::Stopped
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn refreshing_library_availability_keeps_an_armed_greeting_playable() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/greeting.flac', 'Greeting', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    let controller = controller_with_db(test_root.path(), conn, Box::new(SilentPlayback));
    controller.set_random_start_chooser_for_test(|_| Ok(vec![7]));
    controller.restore_session_queue(empty_snapshot(), UpNextQueue::default(), None, None);

    controller.refresh_library_availability();

    assert_eq!(controller.pending_random_start_track_id(), Some(7));
    assert!(controller.bar.prev_button.is_sensitive());
    assert!(controller.bar.next_button.is_sensitive());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn empty_random_library_keeps_a_retained_restored_queue_playable() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks
                 (id, path, title, artist, duration_ms, added_at, missing_since, missing_reason)
             VALUES
                 (7, '/media/offline/restored.flac', 'Restored', 'Artist', 120000, 0,
                  1, 'unmounted');",
        )
        .unwrap();
    let controller = controller_with_db(test_root.path(), conn, Box::new(SilentPlayback));

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

    assert_eq!(controller.session_queue_snapshot().ids, vec![7]);
    assert_eq!(controller.pending_random_start_track_id(), None);
    assert!(!controller.library_has_tracks.get());
    assert!(controller.bar.widget().is_sensitive());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn startup_twice_uses_the_two_seeded_greetings() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/first.flac', 'First greeting', 'Artist', 120000, 0);
             INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (8, '/music/second.flac', 'Second greeting', 'Artist', 120000, 0);",
        )
        .unwrap();
    let controller = controller_with_db(test_root.path(), conn, Box::new(SilentPlayback));
    let mut greetings = std::collections::VecDeque::from([vec![7, 8], vec![8, 7]]);
    controller.set_random_start_chooser_for_test(move |_| Ok(greetings.pop_front().unwrap()));
    let mut restored = empty_snapshot();
    restored.repeat = Repeat::All;
    restored.shuffled = true;
    let mut pending = UpNextQueue::default();
    pending.append(&[QueueItem::Track(8)]);

    controller.restore_session_queue(restored.clone(), pending.clone(), None, None);
    let first = controller.session_queue_snapshot();
    assert!(first.ids.is_empty());
    assert_eq!(first.repeat, Repeat::All);
    assert!(first.shuffled);
    assert_eq!(controller.session_up_next_snapshot().0.ids(), &[8]);
    assert_eq!(controller.pending_random_start_track_id(), Some(7));
    assert_eq!(controller.bar.title_label.text(), "First greeting");

    controller.restore_session_queue(restored, pending, None, None);
    let second = controller.session_queue_snapshot();
    assert!(second.ids.is_empty());
    assert_eq!(second.repeat, Repeat::All);
    assert!(second.shuffled);
    assert_eq!(controller.session_up_next_snapshot().0.ids(), &[8]);
    assert_eq!(controller.pending_random_start_track_id(), Some(8));
    assert_eq!(controller.bar.title_label.text(), "Second greeting");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn startup_with_an_empty_library_clears_the_bar_and_disables_transport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/greeting.flac', 'Greeting', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    let controller = controller_with_db(test_root.path(), conn, Box::new(SilentPlayback));
    let mut greetings = std::collections::VecDeque::from([vec![7], Vec::new()]);
    controller.set_random_start_chooser_for_test(move |_| Ok(greetings.pop_front().unwrap()));

    controller.restore_session_queue(empty_snapshot(), UpNextQueue::default(), None, None);
    assert_eq!(controller.bar.title_label.text(), "Greeting");

    crate::test_db::connection(&controller.conn)
        .execute("DELETE FROM tracks WHERE id = 7", [])
        .unwrap();
    controller.restore_session_queue(empty_snapshot(), UpNextQueue::default(), None, None);

    assert!(controller.session_queue_snapshot().ids.is_empty());
    assert!(controller.now_playing.borrow().is_none());
    assert_eq!(controller.pending_random_start_track_id(), None);
    assert!(!controller.library_has_tracks.get());
    assert_eq!(controller.current_play_origin(), None);
    assert!(!controller.restored_placement_intact.get());
    assert!(!controller.bar.widget().is_sensitive());
    assert_eq!(
        controller.session_playback_status(),
        MprisPlaybackStatus::Stopped
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn chooser_failure_keeps_the_restored_queue_current_track_and_transport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/restored.flac', 'Restored', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    let controller = controller_with_db(test_root.path(), conn, Box::new(SilentPlayback));
    controller.set_random_start_chooser_for_test(|_| Err(rusqlite::Error::InvalidQuery));
    let restored = QueueSnapshot {
        ids: vec![7],
        order: vec![0],
        position: Some(0),
        repeat: Repeat::One,
        shuffled: false,
    };

    controller.restore_session_queue(
        restored.clone(),
        UpNextQueue::default(),
        None,
        Some(super::play_origin::PlayOrigin::library()),
    );

    assert_eq!(controller.session_queue_snapshot(), restored);
    assert_eq!(controller.pending_random_start_track_id(), None);
    assert_eq!(controller.bar.title_label.text(), "Restored");
    assert!(controller.bar.widget().is_sensitive());
    assert!(controller.restored_placement_intact.get());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn next_discards_the_greeting_and_advances_the_restored_queue() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/first.flac', 'First', 'Artist', 120000, 0);
             INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (8, '/music/second.flac', 'Second', 'Artist', 120000, 0);
             INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (9, '/music/greeting.flac', 'Greeting', 'Artist', 120000, 0);",
        )
        .unwrap();
    let played_paths = Rc::new(RefCell::new(Vec::new()));
    let controller = controller_with_db(
        test_root.path(),
        conn,
        Box::new(RecordingPlayback {
            played_paths: played_paths.clone(),
        }),
    );
    controller.set_random_start_chooser_for_test(|_| Ok(vec![9, 8, 7]));
    controller.restore_session_queue(
        QueueSnapshot {
            ids: vec![7, 8],
            order: vec![0, 1],
            position: Some(0),
            repeat: Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        None,
        None,
    );

    controller.next();

    assert_eq!(controller.pending_random_start_track_id(), None);
    assert_eq!(played_paths.borrow().as_slice(), ["/music/second.flac"]);
    assert_eq!(controller.bar.title_label.text(), "Second");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn previous_discards_the_greeting_and_restores_the_persisted_current_track() {
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
    let played_paths = Rc::new(RefCell::new(Vec::new()));
    let controller = controller_with_db(
        test_root.path(),
        conn,
        Box::new(RecordingPlayback {
            played_paths: played_paths.clone(),
        }),
    );
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

    controller.previous();

    assert_eq!(controller.pending_random_start_track_id(), None);
    assert!(played_paths.borrow().is_empty());
    assert_eq!(controller.bar.title_label.text(), "Restored");
    assert!(controller.restored_placement_intact.get());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn dismissing_the_greeting_resyncs_transport_and_notifies_the_restored_track() {
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
    let notified = Rc::new(RefCell::new(Vec::new()));
    let notified_for_callback = notified.clone();
    controller.add_on_current_track_changed(move |track_id, _, _| {
        notified_for_callback.borrow_mut().push(track_id);
    });
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
    controller.notify_restored_current_track();

    assert!(controller.dismiss_random_start_greeting());

    assert_eq!(notified.borrow().as_slice(), [9, 7]);
    assert!(controller.bar.prev_button.is_sensitive());
    assert!(controller.bar.next_button.is_sensitive());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn dismissing_a_greeting_without_a_restored_current_disables_queue_navigation() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (9, '/music/greeting.flac', 'Greeting', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    let controller = controller_with_db(test_root.path(), conn, Box::new(SilentPlayback));
    controller.set_random_start_chooser_for_test(|_| Ok(vec![9]));
    controller.restore_session_queue(empty_snapshot(), UpNextQueue::default(), None, None);
    assert!(controller.bar.prev_button.is_sensitive());
    assert!(controller.bar.next_button.is_sensitive());

    assert!(controller.dismiss_random_start_greeting());

    assert!(!controller.bar.prev_button.is_sensitive());
    assert!(!controller.bar.next_button.is_sensitive());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pending_play_next_item_wins_over_the_random_greeting() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/context.flac', 'Context', 'Artist', 120000, 0);
             INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (8, '/music/play-next.flac', 'Play Next', 'Artist', 120000, 0);",
        )
        .unwrap();
    let controller = controller_with_db(test_root.path(), conn, Box::new(SilentPlayback));
    controller.set_random_start_chooser_for_test(|_| {
        panic!("a pending Play Next item must bypass random startup selection")
    });

    controller.restore_session_queue(
        QueueSnapshot {
            ids: vec![7],
            order: vec![0],
            position: Some(0),
            repeat: Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        Some(QueueItem::Track(8)),
        None,
    );

    assert_eq!(controller.current_up_next.get(), Some(QueueItem::Track(8)));
    assert_eq!(controller.session_queue_snapshot().ids, vec![7]);
    assert_eq!(controller.bar.title_label.text(), "Play Next");
    assert!(controller.restored_placement_intact.get());
    assert_eq!(controller.pending_random_start_track_id(), None);
    assert_eq!(
        controller.session_playback_status(),
        MprisPlaybackStatus::Stopped
    );
}
