use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use reprise_core::db::Db;
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::feed::{ParsedEpisode, ParsedFeed};
use reprise_core::podcasts::pipeline::{FeedFetcher, YoutubeFetcher};
use reprise_core::podcasts::store::{self, NewSubscription};
use reprise_core::podcasts::{PodcastError, PodcastKind, SubscriptionRow};

use super::download_episode_for_playback;

#[derive(Default)]
struct DownloadGate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

impl DownloadGate {
    fn wait_until_started(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !state.0 {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn release(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.1 = true;
        self.changed.notify_all();
    }
}

struct BlockingYoutube {
    gate: Arc<DownloadGate>,
}

impl YoutubeFetcher for BlockingYoutube {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        unreachable!("the fixture downloads an existing episode")
    }

    fn download(&self, _: &str, destination: &Path) -> Result<(), PodcastError> {
        let mut state = self
            .gate
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.0 = true;
        self.gate.changed.notify_all();
        while !state.1 {
            state = self
                .gate
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        std::fs::write(destination, b"audio").map_err(|error| PodcastError::Body(error.to_string()))
    }
}

struct NeverFeed;

impl FeedFetcher for NeverFeed {
    fn fetch(
        &self,
        _: &SubscriptionRow,
    ) -> Result<reprise_core::podcasts::http::Response, PodcastError> {
        unreachable!("the fixture downloads an existing YouTube episode")
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        unreachable!("the fixture downloads a YouTube episode")
    }
}

struct NeverYoutube;

impl YoutubeFetcher for NeverYoutube {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        unreachable!("a playback waiter must not execute a second download")
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        unreachable!("a playback waiter must not execute a second download")
    }
}

fn file_backed_youtube_episode() -> (tempfile::TempDir, Db, Db, PathBuf, i64) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");
    let leader_db = Db::open_migrated(Some(&database_path)).unwrap();
    reprise_core::online_sources::set_enabled(&leader_db, true).unwrap();
    reprise_core::modules::set_enabled(&leader_db, &reprise_core::modules::YOUTUBE_MODULE, true)
        .unwrap();
    let subscription_id = store::add_or_restore(
        &leader_db,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/channel/UCwaiting".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: true,
        },
        1,
    )
    .unwrap();
    let inserted_id = store::upsert_episode(
        &leader_db,
        subscription_id,
        &ParsedEpisode {
            guid: "waiting-video".to_owned(),
            title: "Waiting video".to_owned(),
            image_url: None,
            audio_url: "https://www.youtube.com/watch?v=waiting".to_owned(),
            page_url: None,
            published_at: Some(1),
            duration_secs: None,
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id;
    static NEXT_EPISODE_ID: AtomicI64 = AtomicI64::new(9_100_000);
    let episode_id = NEXT_EPISODE_ID.fetch_add(1, Ordering::Relaxed);
    crate::test_db::connection(&leader_db)
        .execute(
            "UPDATE podcast_episodes SET id = ?2 WHERE id = ?1",
            rusqlite::params![inserted_id, episode_id],
        )
        .unwrap();
    let playback_db = Db::open_ready(&database_path).unwrap();
    let download_root = directory.path().join("downloads");
    (directory, leader_db, playback_db, download_root, episode_id)
}

#[test]
fn playback_joins_a_fill_download_and_receives_its_file() {
    let (_directory, leader_db, playback_db, download_root, episode_id) =
        file_backed_youtube_episode();
    let gate = Arc::new(DownloadGate::default());
    let leader_gate = Arc::clone(&gate);
    let leader_root = download_root.clone();
    let leader = std::thread::spawn(move || {
        reprise_core::podcasts::pipeline::download_episode(
            &leader_db,
            &NeverFeed,
            &BlockingYoutube { gate: leader_gate },
            &leader_root,
            episode_id,
            &mut |_| {},
        )
        .unwrap()
    });
    gate.wait_until_started();

    let (progress_sender, progress_receiver) = std::sync::mpsc::sync_channel(8);
    let playback_root = download_root;
    let playback = std::thread::spawn(move || {
        download_episode_for_playback(
            &playback_db,
            &NeverFeed,
            &NeverYoutube,
            &playback_root,
            episode_id,
            &mut |state| progress_sender.send(state).unwrap(),
        )
    });
    progress_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("playback must join and replay the active download's progress");
    gate.release();

    assert!(matches!(
        leader.join().unwrap(),
        DownloadState::Downloaded { .. }
    ));
    assert!(matches!(
        playback.join().unwrap().unwrap(),
        DownloadState::Downloaded { .. }
    ));
    assert!(store::episode(
        &Db::open_ready(&_directory.path().join("reprise.db")).unwrap(),
        episode_id
    )
    .unwrap()
    .unwrap()
    .downloaded_path
    .is_some());
}
