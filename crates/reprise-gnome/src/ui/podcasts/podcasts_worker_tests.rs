use super::*;
use reprise_core::podcasts::download_state::{DownloadProgress, DownloadState};
use reprise_core::podcasts::feed::ParsedEpisode;
use reprise_core::podcasts::pipeline::FeedFetcher;
use reprise_core::podcasts::store::{self, NewSubscription};

struct ProgressFeed {
    fail: bool,
}

impl FeedFetcher for ProgressFeed {
    fn fetch(
        &self,
        _: &podcasts::SubscriptionRow,
    ) -> Result<podcasts::http::Response, podcasts::PodcastError> {
        unreachable!()
    }

    fn download(&self, _: &str, _: &std::path::Path) -> Result<(), podcasts::PodcastError> {
        unreachable!()
    }

    fn download_with_progress(
        &self,
        _: &str,
        destination: &std::path::Path,
        on_progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), podcasts::PodcastError> {
        std::fs::write(destination, b"0123456789").unwrap();
        on_progress(DownloadProgress {
            received_bytes: 8,
            total_bytes: None,
        });
        on_progress(DownloadProgress {
            received_bytes: 4,
            total_bytes: Some(10),
        });
        on_progress(DownloadProgress {
            received_bytes: 10,
            total_bytes: None,
        });
        if self.fail {
            Err(podcasts::PodcastError::Transport("offline".to_owned()))
        } else {
            Ok(())
        }
    }
}

/// `POD-7`: the worker has no download executor of its own any
/// more — every `pod_7_*`/`net_1a_*` test below drives
/// `reprise_core::podcasts::pipeline::download_episode` directly, the exact
/// function `PodcastsOperation::Download` calls in production, so a
/// regression in the shared path is caught here rather than only in a
/// GNOME-only copy that could drift from it.
fn run_download(
    conn: &reprise_core::db::Db,
    episode_id: i64,
    download_root: &std::path::Path,
    feed_fetcher: &dyn podcasts::pipeline::FeedFetcher,
    youtube_fetcher: &dyn podcasts::pipeline::YoutubeFetcher,
    emit: &mut dyn FnMut(DownloadState),
) {
    let _ = podcasts::pipeline::download_episode(
        conn,
        feed_fetcher,
        youtube_fetcher,
        download_root,
        episode_id,
        emit,
    );
}

fn episode(conn: &reprise_core::db::Db) -> i64 {
    // These tests exercise download-progress plumbing, not the NET-1a
    // gate itself (see the dedicated `net_1a_*` test below).
    reprise_core::modules::set_enabled(conn, &reprise_core::modules::PODCASTS_MODULE, true)
        .unwrap();
    let subscription_id = store::add_or_restore(
        conn,
        &NewSubscription {
            kind: podcasts::PodcastKind::Rss,
            feed_url: "https://example.test/feed".to_owned(),
            title: "Show".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    store::upsert_episode(
        conn,
        subscription_id,
        &ParsedEpisode {
            guid: "episode".to_owned(),
            title: "Episode".to_owned(),
            image_url: None,
            audio_url: "https://example.test/episode.mp3".to_owned(),
            page_url: None,
            published_at: None,
            duration_secs: None,
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id
}

#[test]
fn automatic_refresh_requires_every_gate() {
    assert!(automatic_refresh_allowed(true, 1, false, true));
    assert!(!automatic_refresh_allowed(false, 1, false, true));
    assert!(!automatic_refresh_allowed(true, 0, false, true));
    assert!(!automatic_refresh_allowed(true, 1, true, true));
    assert!(!automatic_refresh_allowed(true, 1, false, false));
}

#[test]
fn plane_6b_youtube_downloads_use_the_opus_extension() {
    assert_eq!(
        podcasts::downloads::extension_for(podcasts::PodcastKind::Youtube, "ignored"),
        "opus"
    );
    assert_eq!(
        podcasts::downloads::extension_for(
            podcasts::PodcastKind::Rss,
            "https://example.test/episode.mp3"
        ),
        "mp3"
    );
}

#[test]
fn pod_7_download_request_does_not_invalidate_an_in_flight_refresh() {
    assert_eq!(
        request_generation(9, PodcastsOperation::Download { episode_id: 4 }),
        9
    );
    assert_eq!(
        request_generation(
            9,
            PodcastsOperation::Refresh {
                policy: podcasts::refresh::RefreshPolicy::Force,
                kind: None,
            },
        ),
        10
    );
    assert_eq!(
        request_generation(
            9,
            PodcastsOperation::LoadMore {
                subscription_id: 7,
                end: 40,
            },
        ),
        10
    );
}

#[test]
fn a_fill_downloads_request_does_not_cancel_a_running_refresh() {
    let current = 7;
    assert_eq!(
        request_generation(current, PodcastsOperation::FillDownloads),
        current
    );
}

#[test]
fn a_row_download_that_loses_to_playback_stays_in_progress() {
    assert_eq!(
        download_error_state(&podcasts::pipeline::PipelineError::DownloadAlreadyRunning),
        Some(DownloadState::Downloading {
            received_bytes: 0,
            total_bytes: None,
        })
    );
    assert_eq!(
        download_error_state(&podcasts::pipeline::PipelineError::EpisodeNotFound),
        None
    );
}

#[test]
fn both_podcast_views_share_one_catalogue_fill_slot() {
    let conn = crate::test_db::open().unwrap();
    let runtime = PodcastsRuntime::setup(&conn);
    let rss_view_runtime = Rc::clone(&runtime);
    let youtube_view_runtime = Rc::clone(&runtime);

    assert!(rss_view_runtime.begin_fill_request());
    assert!(!youtube_view_runtime.begin_fill_request());
    assert!(rss_view_runtime.finish_fill_request());
}

#[test]
fn a_fill_requested_while_running_is_replayed_after_completion() {
    let mut state = FillRequestState::default();

    assert!(state.request());
    assert!(!state.request());
    assert!(state.complete());
    assert!(state.request());
}

#[test]
fn pod_7_download_worker_emits_ordered_monotone_states_and_persists_after_publish() {
    let conn = crate::test_db::open().unwrap();
    let episode_id = episode(&conn);
    let directory = tempfile::tempdir().unwrap();
    let mut states = Vec::new();

    run_download(
        &conn,
        episode_id,
        directory.path(),
        &ProgressFeed { fail: false },
        &NeverYoutube,
        &mut |state| states.push(state),
    );

    assert_eq!(
        states,
        [
            DownloadState::Queued,
            DownloadState::Downloading {
                received_bytes: 0,
                total_bytes: None,
            },
            DownloadState::Downloading {
                received_bytes: 8,
                total_bytes: None,
            },
            DownloadState::Downloading {
                received_bytes: 8,
                total_bytes: Some(10),
            },
            DownloadState::Downloading {
                received_bytes: 10,
                total_bytes: Some(10),
            },
            DownloadState::Downloaded { bytes: 10 },
        ]
    );
    let row = store::episode(&conn, episode_id).unwrap().unwrap();
    assert_eq!(row.downloaded_bytes, Some(10));
    assert!(row
        .downloaded_path
        .is_some_and(|path| !path.ends_with(".part")));
}

#[test]
fn pod_7_failed_worker_download_emits_failed_and_removes_partial() {
    let conn = crate::test_db::open().unwrap();
    let episode_id = episode(&conn);
    let directory = tempfile::tempdir().unwrap();
    let mut states = Vec::new();

    run_download(
        &conn,
        episode_id,
        directory.path(),
        &ProgressFeed { fail: true },
        &NeverYoutube,
        &mut |state| states.push(state),
    );

    // `POD-13`: the raw `PodcastError::Transport("offline")` payload must
    // never reach `DownloadState::Failed` — only its classified reason,
    // identical to what `pipeline::download_episode` and
    // `source_actions::podcast_source_error` report for the same failure
    // kind.
    assert!(matches!(
        states.last(),
        Some(DownloadState::Failed { message }) if message == "podcast source could not be reached"
    ));
    assert!(store::episode(&conn, episode_id)
        .unwrap()
        .unwrap()
        .downloaded_path
        .is_none());
    assert!(walk_files(directory.path()).is_empty());
}

/// A `FeedFetcher` whose provider error carries exactly what `POD-13`
/// forbids: a signed URL with a query string, a credential-looking token,
/// and an absolute local filesystem path.
struct LeakingFeed;

const LEAKING_PROVIDER_MESSAGE: &str = "GET https://cdn.example.test/ep.mp3\
    ?sig=abc123&token=SECRET-TOKEN failed while writing \
    /home/user/.local/share/reprise/podcasts/leak.mp3";

impl FeedFetcher for LeakingFeed {
    fn fetch(
        &self,
        _: &podcasts::SubscriptionRow,
    ) -> Result<podcasts::http::Response, podcasts::PodcastError> {
        unreachable!()
    }

    fn download(&self, _: &str, _: &std::path::Path) -> Result<(), podcasts::PodcastError> {
        Err(podcasts::PodcastError::Transport(
            LEAKING_PROVIDER_MESSAGE.to_owned(),
        ))
    }
}

#[test]
fn pod_7_response_channel_coalesces_progress_but_never_drops_terminal_state() {
    let (response, receiver) = podcasts_response_channel();
    let progress = |received_bytes| PodcastsResponse {
        generation: 7,
        result: Ok(PodcastsWorkerResult::DownloadState {
            episode_id: 4,
            state: DownloadState::Downloading {
                received_bytes,
                total_bytes: Some(30),
            },
        }),
    };
    response.publish_latest(progress(10));
    response.publish_latest(progress(20));
    let latest = receiver.try_recv().unwrap();
    assert!(matches!(
        latest.result,
        Ok(PodcastsWorkerResult::DownloadState {
            state: DownloadState::Downloading {
                received_bytes: 20,
                ..
            },
            ..
        })
    ));

    response.publish_terminal(PodcastsResponse {
        generation: 7,
        result: Ok(PodcastsWorkerResult::DownloadState {
            episode_id: 4,
            state: DownloadState::Failed {
                message: "offline".into(),
            },
        }),
    });
    assert!(matches!(
        receiver.try_recv().unwrap().result.unwrap(),
        PodcastsWorkerResult::DownloadState {
            episode_id: 4,
            state: DownloadState::Failed { ref message },
        } if message == "offline"
    ));
}

#[test]
fn pod_7_episode_removed_during_download_leaves_no_persisted_or_orphaned_file() {
    let conn = crate::test_db::open().unwrap();
    let episode_id = episode(&conn);
    let directory = tempfile::tempdir().unwrap();
    let feed = RemovingFeed {
        conn: &conn,
        episode_id,
    };
    let mut states = Vec::new();

    run_download(
        &conn,
        episode_id,
        directory.path(),
        &feed,
        &NeverYoutube,
        &mut |state| states.push(state),
    );

    assert!(matches!(
        states.last(),
        Some(DownloadState::Failed { message })
            if message == "podcast episode no longer exists"
    ));
    assert!(store::episode(&conn, episode_id).unwrap().is_none());
    assert!(walk_files(directory.path()).is_empty());
}

struct RemovingFeed<'a> {
    conn: &'a reprise_core::db::Db,
    episode_id: i64,
}

impl FeedFetcher for RemovingFeed<'_> {
    fn fetch(
        &self,
        _: &podcasts::SubscriptionRow,
    ) -> Result<podcasts::http::Response, podcasts::PodcastError> {
        unreachable!()
    }

    fn download(
        &self,
        _: &str,
        destination: &std::path::Path,
    ) -> Result<(), podcasts::PodcastError> {
        std::fs::write(destination, b"complete").unwrap();
        store::tombstone_episode(self.conn, self.episode_id, 2).unwrap();
        Ok(())
    }
}

#[derive(Default)]
struct NeverYoutube;

impl podcasts::pipeline::YoutubeFetcher for NeverYoutube {
    fn list(
        &self,
        _: &str,
        _: usize,
    ) -> Result<podcasts::feed::ParsedFeed, podcasts::PodcastError> {
        unreachable!()
    }

    fn download(&self, _: &str, _: &std::path::Path) -> Result<(), podcasts::PodcastError> {
        unreachable!()
    }
}

fn walk_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .flat_map(|entry| {
            if entry.path().is_dir() {
                walk_files(&entry.path())
            } else {
                vec![entry.path()]
            }
        })
        .collect()
}

/// Issue #96: Podcasts off + YouTube on must dispatch work — the runtime's
/// `enabled` flag is an OR of the two source modules, not a single flag
/// shared between them.
#[test]
fn issue_96_podcasts_off_youtube_on_still_dispatches() {
    let conn = crate::test_db::open().unwrap();
    let runtime = PodcastsRuntime::setup(&conn);
    assert!(!runtime.enabled.get(), "both sources start off by default");

    runtime.set_youtube_enabled(&conn, true).unwrap();
    assert!(
        runtime.enabled.get(),
        "YouTube on alone must dispatch, even with Podcasts off"
    );
    assert!(
        !reprise_core::modules::is_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE).unwrap()
    );

    runtime.set_youtube_enabled(&conn, false).unwrap();
    assert!(!runtime.enabled.get());

    runtime.set_podcasts_enabled(&conn, true).unwrap();
    assert!(
        runtime.enabled.get(),
        "Podcasts on alone must dispatch, even with YouTube off"
    );
}

/// `NET-1a`: toggling the global online-sources gate must be reflected by
/// `recompute_enabled`, the hook the Online sources page's global master
/// switch calls after persisting the gate.
#[test]
fn net_1a_recompute_enabled_reflects_the_global_gate() {
    let conn = crate::test_db::open().unwrap();
    let runtime = PodcastsRuntime::setup(&conn);
    runtime.set_podcasts_enabled(&conn, true).unwrap();
    assert!(runtime.enabled.get());

    reprise_core::online_sources::set_enabled(&conn, false).unwrap();
    runtime.recompute_enabled(&conn);
    assert!(
        !runtime.enabled.get(),
        "global gate off must disable dispatch even with Podcasts on"
    );

    reprise_core::online_sources::set_enabled(&conn, true).unwrap();
    runtime.recompute_enabled(&conn);
    assert!(runtime.enabled.get());
}

/// `NET-1a`: a download is gated per the episode's own source kind, so a
/// YouTube episode cannot be downloaded while only Podcasts (RSS) is on.
#[test]
fn net_1a_download_is_blocked_when_its_source_kind_is_disabled() {
    let conn = crate::test_db::open().unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE, true)
        .unwrap();
    // YouTube is deliberately left disabled.
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: podcasts::PodcastKind::Youtube,
            feed_url: "https://www.youtube.com/channel/UCabc".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let episode_id = store::upsert_episode(
        &conn,
        subscription_id,
        &ParsedEpisode {
            guid: "video".to_owned(),
            title: "Video".to_owned(),
            image_url: None,
            audio_url: "https://www.youtube.com/watch?v=video".to_owned(),
            page_url: None,
            published_at: None,
            duration_secs: None,
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id;
    let directory = tempfile::tempdir().unwrap();
    let mut states = Vec::new();

    run_download(
        &conn,
        episode_id,
        directory.path(),
        &ProgressFeed { fail: false },
        &NeverYoutube,
        &mut |state| states.push(state),
    );

    assert!(matches!(
        states.last(),
        Some(DownloadState::Failed { message }) if message == "this source is disabled"
    ));
    assert!(walk_files(directory.path()).is_empty());
}

/// Came in with the dev merge. `subscribe_enabled` notifies its subscribers
/// while holding the subscriber list, so a callback that subscribes again
/// re-enters it; this is the regression test for that borrow.
#[test]
fn enabled_subscriber_can_register_another_subscriber_during_notification() {
    let conn = crate::test_db::open().unwrap();
    let runtime = PodcastsRuntime::setup(&conn);
    let primary_calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let secondary_calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let runtime_for_callback = runtime.clone();
    let primary_calls_for_callback = primary_calls.clone();
    let secondary_calls_for_callback = secondary_calls.clone();
    runtime.subscribe_enabled(move |enabled| {
        primary_calls_for_callback.borrow_mut().push(enabled);
        if enabled {
            let calls = secondary_calls_for_callback.clone();
            runtime_for_callback.subscribe_enabled(move |enabled| {
                calls.borrow_mut().push(enabled);
            });
        }
    });

    runtime
        .set_module_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE, true)
        .unwrap();

    assert_eq!(primary_calls.borrow().as_slice(), [false, true]);
    assert_eq!(secondary_calls.borrow().as_slice(), [true]);
}
