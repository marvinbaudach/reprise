//! Display-backed startup restoration regressions.

use std::rc::Rc;

use gtk4::prelude::WidgetExt;
use reprise_core::media_integration::MprisPlaybackStatus;
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

    assert_eq!(controller.session_queue_snapshot().ids, vec![7]);
    assert_eq!(controller.bar.title_label.text(), "Greeting");
    assert!(controller.bar.widget().is_sensitive());
    assert_eq!(
        controller.current_play_origin(),
        Some(super::play_origin::PlayOrigin::library())
    );
    assert!(!controller.restored_placement_intact.get());
    assert_eq!(
        controller.session_playback_status(),
        MprisPlaybackStatus::Stopped
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn startup_twice_uses_the_two_seeded_greeting_snapshots() {
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
    assert_eq!(first.ids, vec![7, 8]);
    assert_eq!(first.repeat, Repeat::All);
    assert!(first.shuffled);
    assert_eq!(controller.session_up_next_snapshot().0.ids(), &[8]);
    assert_eq!(controller.bar.title_label.text(), "First greeting");

    controller.restore_session_queue(restored, pending, None, None);
    let second = controller.session_queue_snapshot();
    assert_eq!(second.ids, vec![8, 7]);
    assert_eq!(second.repeat, Repeat::All);
    assert!(second.shuffled);
    assert_eq!(controller.session_up_next_snapshot().0.ids(), &[8]);
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
    assert!(!controller.restored_placement_intact.get());
    assert_eq!(
        controller.session_playback_status(),
        MprisPlaybackStatus::Stopped
    );
}
