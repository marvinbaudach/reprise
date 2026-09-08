use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use gtk4::prelude::WidgetExt;
use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent,
};
use reprise_core::queue::{QueueSnapshot, Repeat};
use reprise_core::up_next::{QueueItem, UpNextQueue};

use super::player_controller::PlayerController;
use super::test_support::controller_with_db;

#[derive(Default)]
struct PlaybackCalls {
    played_paths: RefCell<Vec<String>>,
    played_uris: RefCell<Vec<String>>,
    sought_positions: RefCell<Vec<i64>>,
    failed_seeks_remaining: Cell<usize>,
}

struct TestPlayback {
    calls: Rc<PlaybackCalls>,
}

impl PlaybackBackend for TestPlayback {
    fn play(&self, path: &str) -> Result<(), PlaybackError> {
        self.calls.played_paths.borrow_mut().push(path.to_owned());
        Ok(())
    }

    fn play_uri(&self, uri: &str) -> Result<(), PlaybackError> {
        self.calls.played_uris.borrow_mut().push(uri.to_owned());
        Ok(())
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        Ok(PlaybackState::Paused)
    }

    fn seek_to(&self, position_ms: i64) -> Result<(), PlaybackError> {
        self.calls.sought_positions.borrow_mut().push(position_ms);
        let failed_seeks_remaining = self.calls.failed_seeks_remaining.get();
        if failed_seeks_remaining > 0 {
            self.calls
                .failed_seeks_remaining
                .set(failed_seeks_remaining - 1);
            return Err(PlaybackError::Backend("pipeline is not ready".into()));
        }
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

fn controller(calls: Rc<PlaybackCalls>, test_root: &Path) -> Rc<PlayerController> {
    controller_with_db(
        test_root,
        Rc::new(crate::test_db::open().unwrap()),
        Box::new(TestPlayback { calls }),
    )
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stopped_restored_track_marks_the_clicked_waveform_position_until_play() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    let controller = controller(calls.clone(), test_root.path());
    crate::test_db::connection(&controller.conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/restored.flac', 'Restored', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    controller.set_random_start_chooser_for_test(|_| Ok(vec![7]));
    controller.restore_session_queue(
        QueueSnapshot {
            position: Some(0),
            ids: vec![7],
            order: vec![0],
            repeat: Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        None,
        None,
    );

    assert!(
        controller.bar.waveform.widget().is_sensitive(),
        "restore must seed the duration after applying Stopped"
    );

    controller.seek_or_start(30_000);

    assert!(calls.played_paths.borrow().is_empty());
    assert!(calls.sought_positions.borrow().is_empty());

    controller.toggle_pause();

    assert_eq!(
        calls.played_paths.borrow().as_slice(),
        ["/music/restored.flac"]
    );
    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stopped_greeting_scrub_applies_to_the_greeting_when_play_starts() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    let controller = controller(calls.clone(), test_root.path());
    crate::test_db::connection(&controller.conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/restored.flac', 'Restored', 'Artist', 120000, 0);
             INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (9, '/music/greeting.flac', 'Greeting', 'Artist', 120000, 0);",
        )
        .unwrap();
    controller.set_random_start_chooser_for_test(|_| Ok(vec![9, 7]));
    controller.restore_session_queue(
        QueueSnapshot {
            position: Some(0),
            ids: vec![7],
            order: vec![0],
            repeat: Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        None,
        None,
    );

    controller.seek_or_start(30_000);
    controller.toggle_pause();

    assert_eq!(
        calls.played_paths.borrow().as_slice(),
        ["/music/greeting.flac"]
    );
    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stopped_track_mark_applies_when_the_same_track_starts_directly() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    let controller = controller(calls.clone(), test_root.path());
    crate::test_db::connection(&controller.conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/marked.flac', 'Marked', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    controller.current_up_next.set(Some(QueueItem::Track(7)));

    controller.seek_or_start(30_000);

    assert!(calls.played_paths.borrow().is_empty());
    assert!(calls.sought_positions.borrow().is_empty());

    controller.play_track_id(7);

    assert_eq!(
        calls.played_paths.borrow().as_slice(),
        ["/music/marked.flac"]
    );
    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stopped_track_mark_cannot_survive_a_different_direct_start() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    let controller = controller(calls.clone(), test_root.path());
    crate::test_db::connection(&controller.conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/marked.flac', 'Marked', 'Artist', 120000, 0);
             INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (8, '/music/other.flac', 'Other', 'Artist', 120000, 0);",
        )
        .unwrap();
    controller.current_up_next.set(Some(QueueItem::Track(7)));

    controller.seek_or_start(30_000);

    assert!(calls.played_paths.borrow().is_empty());
    assert!(calls.sought_positions.borrow().is_empty());

    controller.play_track_id(8);

    assert_eq!(
        calls.played_paths.borrow().as_slice(),
        ["/music/other.flac"]
    );
    assert!(calls.sought_positions.borrow().is_empty());

    controller.reset_to_stopped();
    controller.toggle_pause();

    assert_eq!(
        calls.played_paths.borrow().as_slice(),
        ["/music/other.flac", "/music/marked.flac"]
    );
    assert!(calls.sought_positions.borrow().is_empty());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn repeat_one_restart_does_not_apply_a_stopped_track_mark() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    let controller = controller(calls.clone(), test_root.path());
    crate::test_db::connection(&controller.conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/marked.flac', 'Marked', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    controller.set_random_start_chooser_for_test(|_| Ok(vec![7]));
    controller.restore_session_queue(
        QueueSnapshot {
            position: Some(0),
            ids: vec![7],
            order: vec![0],
            repeat: Repeat::One,
            shuffled: false,
        },
        UpNextQueue::default(),
        None,
        None,
    );

    controller.seek_or_start(30_000);

    assert!(calls.played_paths.borrow().is_empty());
    assert!(calls.sought_positions.borrow().is_empty());

    controller.apply_event(PlayerEvent::TrackFinished);

    assert_eq!(
        calls.played_paths.borrow().as_slice(),
        ["/music/marked.flac"]
    );
    assert!(
        calls.sought_positions.borrow().is_empty(),
        "a repeat-one restart must start from the beginning"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn restored_episode_marks_the_clicked_position_until_play() {
    use reprise_core::library::session::{SessionEpisode, SessionEpisodeOrigin};
    use reprise_core::podcasts::feed::ParsedEpisode;
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    let controller = controller(calls.clone(), test_root.path());
    let subscription_id = store::add_or_restore(
        &controller.conn,
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
    let episode_id = store::upsert_episode(
        &controller.conn,
        subscription_id,
        &ParsedEpisode {
            guid: "episode-1".into(),
            title: "Restored episode".into(),
            image_url: None,
            audio_url: "https://podcast.test/episode.mp3".into(),
            page_url: None,
            published_at: Some(1),
            duration_secs: Some(3_600),
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id;
    store::save_position(&controller.conn, episode_id, 22_000).unwrap();
    assert!(controller.restore_session_episode(Some(&SessionEpisode {
        episode_id,
        origin: SessionEpisodeOrigin::Direct,
        neighbour_episode_ids: vec![episode_id],
    })));

    controller.seek_or_start(30_000);

    assert!(calls.played_uris.borrow().is_empty());
    assert!(calls.sought_positions.borrow().is_empty());

    controller.toggle_pause();

    assert_eq!(
        calls.played_uris.borrow().as_slice(),
        ["https://podcast.test/episode.mp3"]
    );
    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn restoring_an_episode_discards_an_armed_random_greeting() {
    use reprise_core::library::session::{SessionEpisode, SessionEpisodeOrigin};
    use reprise_core::podcasts::feed::ParsedEpisode;
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    let controller = controller(calls, test_root.path());
    crate::test_db::connection(&controller.conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (9, '/music/greeting.flac', 'Greeting', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    controller.set_random_start_chooser_for_test(|_| Ok(vec![9]));
    controller.restore_session_queue(
        QueueSnapshot {
            position: None,
            ids: Vec::new(),
            order: Vec::new(),
            repeat: Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        None,
        None,
    );
    assert_eq!(controller.pending_random_start_track_id(), Some(9));

    let subscription_id = store::add_or_restore(
        &controller.conn,
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
    let episode_id = store::upsert_episode(
        &controller.conn,
        subscription_id,
        &ParsedEpisode {
            guid: "episode-greeting".into(),
            title: "Restored episode".into(),
            image_url: None,
            audio_url: "https://podcast.test/episode.mp3".into(),
            page_url: None,
            published_at: Some(1),
            duration_secs: Some(3_600),
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id;

    assert!(controller.restore_session_episode(Some(&SessionEpisode {
        episode_id,
        origin: SessionEpisodeOrigin::Direct,
        neighbour_episode_ids: vec![episode_id],
    })));

    assert_eq!(controller.pending_random_start_track_id(), None);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stopped_restored_track_retries_the_clicked_position_once_after_preroll() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    calls.failed_seeks_remaining.set(1);
    let controller = controller(calls.clone(), test_root.path());
    crate::test_db::connection(&controller.conn)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/restored.flac', 'Restored', 'Artist', 120000, 0)",
            [],
        )
        .unwrap();
    controller.set_random_start_chooser_for_test(|_| Ok(vec![7]));
    controller.restore_session_queue(
        QueueSnapshot {
            position: Some(0),
            ids: vec![7],
            order: vec![0],
            repeat: Repeat::Off,
            shuffled: false,
        },
        UpNextQueue::default(),
        None,
        None,
    );

    controller.seek_or_start(30_000);
    assert!(calls.played_paths.borrow().is_empty());
    assert!(calls.sought_positions.borrow().is_empty());

    controller.toggle_pause();
    controller.apply_event(PlayerEvent::Position {
        position_ms: 0,
        duration_ms: 120_000,
    });
    controller.apply_event(PlayerEvent::Position {
        position_ms: 500,
        duration_ms: 120_000,
    });

    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000, 30_000]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn restored_episode_retries_only_the_clicked_position_once_after_preroll() {
    use reprise_core::library::session::{SessionEpisode, SessionEpisodeOrigin};
    use reprise_core::podcasts::feed::ParsedEpisode;
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    calls.failed_seeks_remaining.set(1);
    let controller = controller(calls.clone(), test_root.path());
    let subscription_id = store::add_or_restore(
        &controller.conn,
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
    let episode_id = store::upsert_episode(
        &controller.conn,
        subscription_id,
        &ParsedEpisode {
            guid: "episode-retry".into(),
            title: "Restored episode".into(),
            image_url: None,
            audio_url: "https://podcast.test/episode.mp3".into(),
            page_url: None,
            published_at: Some(1),
            duration_secs: Some(3_600),
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id;
    store::save_position(&controller.conn, episode_id, 22_000).unwrap();
    assert!(controller.restore_session_episode(Some(&SessionEpisode {
        episode_id,
        origin: SessionEpisodeOrigin::Direct,
        neighbour_episode_ids: vec![episode_id],
    })));

    controller.seek_or_start(30_000);
    assert!(calls.played_uris.borrow().is_empty());
    assert!(calls.sought_positions.borrow().is_empty());

    controller.toggle_pause();
    controller.apply_event(PlayerEvent::Position {
        position_ms: 0,
        duration_ms: 3_600_000,
    });
    controller.apply_event(PlayerEvent::Position {
        position_ms: 500,
        duration_ms: 3_600_000,
    });

    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000, 30_000]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn restored_youtube_episode_keeps_the_clicked_position_until_delayed_source_start() {
    use reprise_core::library::session::{SessionEpisode, SessionEpisodeOrigin};
    use reprise_core::podcasts::feed::ParsedEpisode;
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    let controller = controller_with_db(
        test_root.path(),
        Rc::new(reprise_core::db::Db::open_in_memory().unwrap()),
        Box::new(TestPlayback {
            calls: calls.clone(),
        }),
    );
    let subscription_id = store::add_or_restore(
        &controller.conn,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://youtube.test/channel".into(),
            title: "Channel".into(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let episode_id = store::upsert_episode(
        &controller.conn,
        subscription_id,
        &ParsedEpisode {
            guid: "video-1".into(),
            title: "Restored video".into(),
            image_url: None,
            audio_url: "https://youtube.test/watch?v=1".into(),
            page_url: None,
            published_at: Some(1),
            duration_secs: Some(3_600),
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id;
    store::save_position(&controller.conn, episode_id, 22_000).unwrap();
    assert!(controller.restore_session_episode(Some(&SessionEpisode {
        episode_id,
        origin: SessionEpisodeOrigin::Direct,
        neighbour_episode_ids: vec![episode_id],
    })));

    controller.seek_or_start(30_000);
    assert!(calls.played_paths.borrow().is_empty());
    assert!(calls.sought_positions.borrow().is_empty());

    controller.toggle_pause();
    assert!(calls.played_paths.borrow().is_empty());
    assert!(calls.sought_positions.borrow().is_empty());

    let generation = controller.external.borrow().generation;
    controller
        .start_podcast_source(
            generation,
            episode_id,
            super::external_media::EpisodeSource::File("/downloads/video-1.opus".into()),
        )
        .unwrap();

    assert_eq!(
        calls.played_paths.borrow().as_slice(),
        ["/downloads/video-1.opus"]
    );
    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stopped_queued_episode_marks_the_clicked_position_until_play() {
    use reprise_core::podcasts::feed::ParsedEpisode;
    use reprise_core::podcasts::store::{self, NewSubscription};
    use reprise_core::podcasts::PodcastKind;

    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let test_root = tempfile::tempdir().unwrap();
    let calls = Rc::new(PlaybackCalls::default());
    let controller = controller(calls.clone(), test_root.path());
    let subscription_id = store::add_or_restore(
        &controller.conn,
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
    let episode_id = store::upsert_episode(
        &controller.conn,
        subscription_id,
        &ParsedEpisode {
            guid: "queued-episode".into(),
            title: "Queued episode".into(),
            image_url: None,
            audio_url: "https://podcast.test/queued.mp3".into(),
            page_url: None,
            published_at: Some(1),
            duration_secs: Some(3_600),
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id;
    controller
        .current_up_next
        .set(Some(QueueItem::Episode(episode_id)));

    controller.seek_or_start(30_000);

    assert!(calls.played_uris.borrow().is_empty());
    assert!(calls.sought_positions.borrow().is_empty());

    controller.toggle_pause();

    assert_eq!(
        calls.played_uris.borrow().as_slice(),
        ["https://podcast.test/queued.mp3"]
    );
    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000]);
}
