use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use super::tests::{add_subscription, conn, feed_response, FakeFeed, FakeYoutube};
use super::*;
use crate::podcasts::feed::{ParsedEpisode, ParsedFeed};

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

struct BlockingFeed {
    gate: Arc<DownloadGate>,
    fail: bool,
}

impl FeedFetcher for BlockingFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<super::super::http::Response, PodcastError> {
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
        if self.fail {
            Err(PodcastError::Transport("fixture failure".to_owned()))
        } else {
            std::fs::write(destination, b"audio")
                .map_err(|error| PodcastError::Body(error.to_string()))
        }
    }
}

struct NeverYoutube;

impl YoutubeFetcher for NeverYoutube {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        unreachable!("the fixture is an RSS subscription")
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        unreachable!("the fixture is an RSS subscription")
    }
}

struct NeverFeed;

impl FeedFetcher for NeverFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<super::super::http::Response, PodcastError> {
        unreachable!("a waiter must not execute a second download")
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        unreachable!("a waiter must not execute a second download")
    }
}

fn file_backed_episode() -> (tempfile::TempDir, Db, Db, PathBuf, i64) {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");
    let winner_db = Db::open_migrated(Some(&database_path)).unwrap();
    crate::online_sources::set_enabled(&winner_db, true).unwrap();
    crate::modules::set_enabled(&winner_db, &crate::modules::PODCASTS_MODULE, true).unwrap();
    let subscription_id = add_subscription(
        winner_db.conn(),
        "https://example.test/concurrent-feed",
        false,
    );
    let inserted_episode_id = super::super::store::upsert_episode_in(
        winner_db.conn(),
        subscription_id,
        &ParsedEpisode {
            guid: "concurrent-episode".to_owned(),
            title: "Concurrent episode".to_owned(),
            image_url: None,
            audio_url: "https://example.test/concurrent.mp3".to_owned(),
            page_url: None,
            published_at: Some(1),
            duration_secs: None,
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id;
    static NEXT_EPISODE_ID: AtomicI64 = AtomicI64::new(9_000_000);
    let episode_id = NEXT_EPISODE_ID.fetch_add(1, Ordering::Relaxed);
    winner_db
        .conn()
        .execute(
            "UPDATE podcast_episodes SET id = ?2 WHERE id = ?1",
            rusqlite::params![inserted_episode_id, episode_id],
        )
        .unwrap();
    let follower_db = Db::open_ready(&database_path).unwrap();
    let download_root = directory.path().join("downloads");
    (directory, winner_db, follower_db, download_root, episode_id)
}

fn run_waiting_download(fail: bool) -> (DownloadState, DownloadState, Vec<DownloadState>) {
    let (_directory, winner_db, follower_db, download_root, episode_id) = file_backed_episode();
    let gate = Arc::new(DownloadGate::default());
    let winner_gate = Arc::clone(&gate);
    let winner_root = download_root.clone();
    let winner = std::thread::spawn(move || {
        download_episode(
            &winner_db,
            &BlockingFeed {
                gate: winner_gate,
                fail,
            },
            &NeverYoutube,
            &winner_root,
            episode_id,
            &mut |_| {},
        )
        .unwrap()
    });
    gate.wait_until_started();

    let (progress_sender, progress_receiver) = std::sync::mpsc::sync_channel(8);
    let follower_root = download_root;
    let follower = std::thread::spawn(move || {
        let mut states = Vec::new();
        let terminal = download_episode_waiting(
            &follower_db,
            &NeverFeed,
            &NeverYoutube,
            &follower_root,
            episode_id,
            &mut |state| {
                progress_sender.send(state.clone()).unwrap();
                states.push(state);
            },
        )
        .unwrap();
        (terminal, states)
    });
    progress_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("the waiter must receive in-flight progress before release");
    gate.release();

    let winner_terminal = winner.join().unwrap();
    let (follower_terminal, follower_states) = follower.join().unwrap();
    (winner_terminal, follower_terminal, follower_states)
}

#[test]
fn a_concurrent_download_of_the_same_episode_is_refused() {
    let db = conn();
    let root = tempfile::tempdir().unwrap();
    let subscription_id = add_subscription(db.conn(), "https://example.test/feed", false);
    let feed = FakeFeed {
        responses: RefCell::new(vec![Ok(feed_response("Show", 1, None))]),
        ..FakeFeed::default()
    };
    refresh_to_root(
        &db,
        &feed,
        &FakeYoutube,
        10,
        RefreshRequest::force(),
        root.path(),
    )
    .unwrap();
    let episode_id =
        super::super::query::episodes_for_subscription(&db, subscription_id).unwrap()[0].id;
    let super::super::download_claims::ClaimOutcome::Acquired(held) =
        super::super::download_claims::claim(episode_id)
    else {
        panic!("claim must be acquired");
    };

    let error = download_episode(
        &db,
        &feed,
        &FakeYoutube,
        root.path(),
        episode_id,
        &mut |_| {},
    )
    .expect_err("a claimed episode must not download twice");

    assert!(matches!(error, PipelineError::DownloadAlreadyRunning));
    drop(held);
}

#[test]
fn a_waiting_caller_receives_the_in_flight_downloads_terminal_state() {
    let (winner, follower, progress) = run_waiting_download(false);

    assert_eq!(follower, winner);
    assert!(matches!(follower, DownloadState::Downloaded { bytes: 5 }));
    assert_eq!(progress.last(), Some(&follower));
}

#[test]
fn a_waiting_caller_receives_the_in_flight_failure() {
    let (winner, follower, progress) = run_waiting_download(true);

    assert_eq!(follower, winner);
    assert!(matches!(follower, DownloadState::Failed { .. }));
    assert_eq!(progress.last(), Some(&follower));
}
