use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent,
};
use reprise_core::podcasts::feed::ParsedEpisode;
use reprise_core::podcasts::store::{self, NewSubscription};
use reprise_core::podcasts::PodcastKind;
use reprise_core::up_next::QueueItem;
use reprise_view::sleep_timer::SleepTimer;

use super::external_media_state::{EpisodeSource, ExternalMedia};
use super::play_origin::PlayOrigin;
use super::sleep_timer_hooks::SleepTimerBinding;
use super::test_support::controller_with_db;

#[derive(Default)]
struct Calls {
    pauses: Cell<u32>,
    plays: Cell<u32>,
    volumes: RefCell<Vec<f64>>,
}

struct TestPlayback(Rc<Calls>);

impl PlaybackBackend for TestPlayback {
    fn play(&self, _: &str) -> Result<(), PlaybackError> {
        self.0.plays.set(self.0.plays.get() + 1);
        Ok(())
    }

    fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
        self.0.plays.set(self.0.plays.get() + 1);
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

fn insert_episode(db: &reprise_core::db::Db) -> i64 {
    let subscription_id = store::add_or_restore(
        db,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://podcast.test/feed.xml".into(),
            title: "Show".into(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    store::upsert_episode(
        db,
        subscription_id,
        &ParsedEpisode {
            guid: "sleep-timer-episode".into(),
            title: "Sleep timer episode".into(),
            image_url: None,
            audio_url: "https://podcast.test/episode.mp3".into(),
            page_url: None,
            published_at: Some(1),
            duration_secs: Some(60),
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_18_track_finished_pauses_without_advancing() {
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

    controller.apply_event(PlayerEvent::Position {
        position_ms: 58_000,
        duration_ms: 60_000,
    });

    controller.apply_event(PlayerEvent::TrackFinished);

    assert_eq!(calls.pauses.get(), 1);
    assert_eq!(controller.current_track.get().map(|(id, _)| id), Some(7));
    let volumes = calls.volumes.borrow();
    assert!(volumes.iter().any(|volume| *volume < 1.0));
    assert_eq!(volumes.last().copied(), Some(1.0));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_18_gapless_handoff_advances_the_model_before_pausing() {
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

    controller.apply_event(PlayerEvent::AdvancedToNext);

    assert_eq!(controller.current_track.get().map(|(id, _)| id), Some(8));
    assert_eq!(calls.pauses.get(), 1);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_18_minute_fade_moves_then_restores_volume() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let db = Rc::new(crate::test_db::open().unwrap());
    insert_track(&db, 7, "/music/first.flac");
    let calls = Rc::new(Calls::default());
    let controller =
        controller_with_db(test_root.path(), db, Box::new(TestPlayback(calls.clone())));
    controller.play_from_view(vec![7], 0, PlayOrigin::library());
    let timer = Rc::new(RefCell::new(SleepTimer::off()));
    let binding = SleepTimerBinding::new(timer);
    controller.install_sleep_timer(&binding);
    controller.arm_sleep_timer_minutes(&binding, std::time::Duration::ZERO, 1);

    controller.sleep_timer_tick(&binding, std::time::Duration::from_millis(56_500), 0, 0);
    controller.sleep_timer_tick(&binding, std::time::Duration::from_secs(60), 0, 0);

    let volumes = calls.volumes.borrow();
    assert!(volumes.iter().any(|volume| *volume < 1.0));
    assert_eq!(volumes.last().copied(), Some(1.0));
    assert_eq!(calls.pauses.get(), 1);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_18_user_volume_change_during_fade_rebases_and_still_pauses() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let db = Rc::new(crate::test_db::open().unwrap());
    insert_track(&db, 7, "/music/first.flac");
    let calls = Rc::new(Calls::default());
    let controller =
        controller_with_db(test_root.path(), db, Box::new(TestPlayback(calls.clone())));
    controller.play_from_view(vec![7], 0, PlayOrigin::library());
    let timer = Rc::new(RefCell::new(SleepTimer::off()));
    let binding = SleepTimerBinding::new(timer.clone());
    controller.install_sleep_timer(&binding);
    controller.arm_sleep_timer_minutes(&binding, std::time::Duration::ZERO, 1);
    controller.sleep_timer_tick(&binding, std::time::Duration::from_millis(58_000), 0, 0);

    controller.volume.set(0.2);
    controller.sleep_timer_tick(&binding, std::time::Duration::from_millis(58_500), 0, 0);
    controller.sleep_timer_tick(&binding, std::time::Duration::from_secs(60), 0, 0);

    assert!(calls
        .volumes
        .borrow()
        .iter()
        .any(|volume| (*volume - 0.075).abs() < f64::EPSILON));
    assert_eq!(calls.volumes.borrow().last().copied(), Some(0.2));
    assert_eq!(calls.pauses.get(), 1);
    assert!(!timer.borrow().is_armed());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_18_cancel_mid_fade_restores_the_users_volume() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let db = Rc::new(crate::test_db::open().unwrap());
    insert_track(&db, 7, "/music/first.flac");
    let calls = Rc::new(Calls::default());
    let controller =
        controller_with_db(test_root.path(), db, Box::new(TestPlayback(calls.clone())));
    controller.play_from_view(vec![7], 0, PlayOrigin::library());
    let binding = SleepTimerBinding::new(Rc::new(RefCell::new(SleepTimer::off())));
    controller.install_sleep_timer(&binding);
    controller.arm_sleep_timer_minutes(&binding, std::time::Duration::ZERO, 1);
    controller.sleep_timer_tick(&binding, std::time::Duration::from_millis(58_000), 0, 0);

    controller.volume.set(0.2);
    controller.cancel_sleep_timer(&binding);

    assert_eq!(controller.volume.get(), 0.2);
    assert_eq!(calls.volumes.borrow().last().copied(), Some(0.2));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_18_manual_track_change_rearms_and_fades_the_new_track() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let db = Rc::new(crate::test_db::open().unwrap());
    insert_track(&db, 7, "/music/first.flac");
    insert_track(&db, 8, "/music/second.flac");
    let calls = Rc::new(Calls::default());
    let controller =
        controller_with_db(test_root.path(), db, Box::new(TestPlayback(calls.clone())));
    controller.play_from_view(vec![7, 8], 0, PlayOrigin::library());
    let timer = Rc::new(RefCell::new(SleepTimer::off()));
    let binding = SleepTimerBinding::new(timer.clone());
    controller.install_sleep_timer(&binding);
    assert!(controller.arm_sleep_timer_end_of_track(&binding));
    controller.sleep_timer_tick(&binding, std::time::Duration::ZERO, 58_000, 60_000);

    controller.play_from_view(vec![7, 8], 1, PlayOrigin::library());
    controller.sleep_timer_tick(&binding, std::time::Duration::ZERO, 56_000, 60_000);
    controller.sleep_timer_tick(&binding, std::time::Duration::ZERO, 58_000, 60_000);
    controller.apply_event(PlayerEvent::TrackFinished);

    assert_eq!(timer.borrow().armed_item(), None);
    assert_eq!(controller.current_track.get().map(|(id, _)| id), Some(8));
    assert_eq!(calls.pauses.get(), 1);
    assert_eq!(calls.volumes.borrow().last().copied(), Some(1.0));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_18_volume_change_pause_and_rearm_does_not_poison_the_next_fade() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let db = Rc::new(crate::test_db::open().unwrap());
    insert_track(&db, 7, "/music/first.flac");
    let calls = Rc::new(Calls::default());
    let controller =
        controller_with_db(test_root.path(), db, Box::new(TestPlayback(calls.clone())));
    controller.play_from_view(vec![7], 0, PlayOrigin::library());
    let binding = SleepTimerBinding::new(Rc::new(RefCell::new(SleepTimer::off())));
    controller.install_sleep_timer(&binding);
    controller.arm_sleep_timer_minutes(&binding, std::time::Duration::ZERO, 1);
    controller.sleep_timer_tick(&binding, std::time::Duration::from_millis(58_000), 0, 0);
    controller.volume.set(0.2);
    controller.sleep_timer_tick(&binding, std::time::Duration::from_secs(60), 0, 0);

    controller.apply_event(PlayerEvent::StateChanged(PlaybackState::Playing));
    controller.arm_sleep_timer_minutes(&binding, std::time::Duration::from_secs(60), 1);
    controller.sleep_timer_tick(&binding, std::time::Duration::from_millis(118_000), 0, 0);
    controller.sleep_timer_tick(&binding, std::time::Duration::from_secs(120), 0, 0);

    assert_eq!(calls.pauses.get(), 2);
    assert_eq!(controller.volume.get(), 0.2);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_18_episode_eos_completes_before_sleep_timer_feedback_and_play_restarts_it() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let db = Rc::new(crate::test_db::open().unwrap());
    let episode_id = insert_episode(&db);
    let calls = Rc::new(Calls::default());
    let controller =
        controller_with_db(test_root.path(), db, Box::new(TestPlayback(calls.clone())));
    controller
        .current_up_next
        .set(Some(QueueItem::Episode(episode_id)));
    controller
        .play_external(ExternalMedia::Podcast {
            episode_id,
            title: "Sleep timer episode".into(),
            show: "Show".into(),
            source: EpisodeSource::Url("https://podcast.test/episode.mp3".into()),
            resume_ms: 0,
            duration_ms: Some(60_000),
        })
        .unwrap();
    let timer = Rc::new(RefCell::new(SleepTimer::off()));
    let binding = SleepTimerBinding::new(timer.clone());
    controller.install_sleep_timer(&binding);
    assert!(controller.arm_sleep_timer_end_of_track(&binding));

    controller.apply_event(PlayerEvent::TrackFinished);

    assert_eq!(calls.pauses.get(), 0, "EOS must not checkpoint as Paused");
    assert!(controller.current_external_snapshot().is_none());
    assert!(!timer.borrow().is_armed());

    controller.toggle_pause();

    assert_eq!(
        calls.plays.get(),
        2,
        "Play restarts the finished queue item"
    );
    assert!(controller.current_external_snapshot().is_some());
}
