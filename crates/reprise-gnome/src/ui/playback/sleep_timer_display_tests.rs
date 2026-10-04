use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent,
};
use reprise_view::sleep_timer::SleepTimer;

use super::play_origin::PlayOrigin;
use super::sleep_timer_hooks::SleepTimerBinding;
use super::test_support::controller_with_db;

#[derive(Default)]
struct Calls {
    pauses: Cell<u32>,
    volumes: RefCell<Vec<f64>>,
}

struct TestPlayback(Rc<Calls>);

impl PlaybackBackend for TestPlayback {
    fn play(&self, _: &str) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        self.0.pauses.set(self.0.pauses.get() + 1);
        Ok(PlaybackState::Paused)
    }

    fn seek_to(&self, _: i64) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn set_volume(&self, volume: f64) {
        self.0.volumes.borrow_mut().push(volume);
    }

    fn set_audio_effects(&self, _: AudioEffects) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn set_next(&self, _: Option<&str>) {}

    fn set_transition(&self, _: reprise_core::library::settings::TrackTransition, _: u8) {}
}

fn insert_track(db: &reprise_core::db::Db, id: i64, path: &str) {
    crate::test_db::connection(db)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (?1, ?2, ?3, 'Artist', 60000, 0)",
            rusqlite::params![id, path, format!("Track {id}")],
        )
        .unwrap();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_sleep_1_track_finished_pauses_without_advancing() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let db = Rc::new(crate::test_db::open().unwrap());
    insert_track(&db, 7, "/music/first.flac");
    insert_track(&db, 8, "/music/second.flac");
    let calls = Rc::new(Calls::default());
    let controller = controller_with_db(
        Path::new(test_root.path()),
        db,
        Box::new(TestPlayback(calls.clone())),
    );
    controller.play_from_view(vec![7, 8], 0, PlayOrigin::library());
    let timer = Rc::new(RefCell::new(SleepTimer::off()));
    let binding = SleepTimerBinding::new(timer);
    controller.install_sleep_timer(&binding);
    assert!(controller.arm_sleep_timer_end_of_track(&binding));

    controller.apply_event(PlayerEvent::TrackFinished);

    assert_eq!(calls.pauses.get(), 1);
    assert_eq!(controller.current_track.get().map(|(id, _)| id), Some(7));
    assert_eq!(calls.volumes.borrow().last().copied(), Some(1.0));
}
