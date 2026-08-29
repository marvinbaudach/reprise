use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::WidgetExt;
use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent,
};
use reprise_core::queue::{QueueSnapshot, Repeat};
use reprise_core::up_next::{QueueItem, UpNextQueue};
use reprise_core::waveform::{RenderDataBackend, WaveformBackend, WaveformError};

use super::player_controller::{PlayerController, PlayerControllerBackends};
use crate::ui::scrobble_runtime::ScrobbleRuntime;

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

struct TestWaveform;

impl WaveformBackend for TestWaveform {
    fn extract_peaks(&self, _: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
        Ok(vec![0; buckets])
    }
}

impl RenderDataBackend for TestWaveform {}

fn controller(calls: Rc<PlaybackCalls>, test_root: &Path) -> Rc<PlayerController> {
    controller_with_db(calls, test_root, Rc::new(crate::test_db::open().unwrap()))
}

fn controller_with_db(
    calls: Rc<PlaybackCalls>,
    test_root: &Path,
    conn: Rc<reprise_core::db::Db>,
) -> Rc<PlayerController> {
    let app = libadwaita::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.SeekStartTest")
        .build();
    let (_event_sender, playback_events) = async_channel::unbounded::<PlayerEvent>();
    let listenbrainz = ScrobbleRuntime::new(
        test_root.join("listenbrainz.db"),
        reprise_core::scrobbling::ScrobbleProvider::ListenBrainz,
        "ListenBrainz",
    );
    let lastfm = ScrobbleRuntime::new(
        test_root.join("lastfm.db"),
        reprise_core::scrobbling::ScrobbleProvider::LastFm,
        "Last.fm",
    );
    PlayerController::new(
        conn,
        crate::ui::cover_download_worker::setup_for_test(),
        listenbrainz,
        lastfm,
        PlayerControllerBackends {
            playback: Box::new(TestPlayback { calls }),
            playback_events,
            media: reprise_core::media_integration::MediaIntegrationHandles::inert(),
            waveform: Arc::new(TestWaveform),
        },
        &app,
    )
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stopped_restored_track_starts_at_the_clicked_waveform_position() {
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

    assert_eq!(
        calls.played_paths.borrow().as_slice(),
        ["/music/restored.flac"]
    );
    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn restored_episode_starts_at_the_clicked_position_not_its_saved_resume() {
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

    assert_eq!(
        calls.played_uris.borrow().as_slice(),
        ["https://podcast.test/episode.mp3"]
    );
    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000]);
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
        calls.clone(),
        test_root.path(),
        Rc::new(reprise_core::db::Db::open_in_memory().unwrap()),
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
fn stopped_queued_episode_does_not_drop_the_clicked_position() {
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

    assert_eq!(
        calls.played_uris.borrow().as_slice(),
        ["https://podcast.test/queued.mp3"]
    );
    assert_eq!(calls.sought_positions.borrow().as_slice(), [30_000]);
}
