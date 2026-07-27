//! Long-lived podcast refresh and download worker.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::podcasts;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PodcastsOperation {
    Refresh { force: bool },
    Download { episode_id: i64 },
}

pub(in crate::ui) const fn request_generation(current: u64, operation: PodcastsOperation) -> u64 {
    match operation {
        PodcastsOperation::Refresh { .. } => current.wrapping_add(1),
        PodcastsOperation::Download { .. } => current,
    }
}

#[derive(Debug)]
pub(in crate::ui) struct PodcastsRequest {
    pub generation: u64,
    pub operation: PodcastsOperation,
    pub response: PodcastsResponseChannel,
}

#[derive(Debug)]
pub(in crate::ui) struct PodcastsResponse {
    pub generation: u64,
    pub result: Result<PodcastsWorkerResult, String>,
}

#[derive(Debug)]
pub(in crate::ui) struct PodcastsResponseChannel {
    sender: async_channel::Sender<PodcastsResponse>,
    stale: async_channel::Receiver<PodcastsResponse>,
}

pub(in crate::ui) fn podcasts_response_channel() -> (
    PodcastsResponseChannel,
    async_channel::Receiver<PodcastsResponse>,
) {
    let (sender, receiver) = async_channel::bounded(1);
    (
        PodcastsResponseChannel {
            sender,
            stale: receiver.clone(),
        },
        receiver,
    )
}

impl PodcastsResponseChannel {
    fn publish_latest(&self, response: PodcastsResponse) {
        match self.sender.try_send(response) {
            Ok(()) => {}
            Err(async_channel::TrySendError::Full(response)) => {
                let _ = self.stale.try_recv();
                if let Err(error) = self.sender.try_send(response) {
                    tracing::debug!(%error, "podcast worker response receiver is unavailable");
                }
            }
            Err(async_channel::TrySendError::Closed(_)) => {}
        }
    }

    fn publish_terminal(&self, response: PodcastsResponse) {
        if let Err(error) = self.sender.send_blocking(response) {
            tracing::debug!(%error, "podcast worker response receiver is unavailable");
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PodcastsWorkerResult {
    Refreshed(podcasts::pipeline::RefreshSummary),
    DownloadState {
        episode_id: i64,
        state: podcasts::download_state::DownloadState,
    },
}

type OnEnabled = Rc<dyn Fn(bool)>;

pub(in crate::ui) struct PodcastsRuntime {
    pub enabled: Rc<Cell<bool>>,
    worker: async_channel::Sender<PodcastsRequest>,
    subscribers: RefCell<Vec<OnEnabled>>,
}

impl PodcastsRuntime {
    pub(in crate::ui) fn setup(conn: &rusqlite::Connection) -> Rc<Self> {
        let enabled =
            reprise_core::modules::is_enabled(conn, &reprise_core::modules::PODCASTS_MODULE)
                .unwrap_or(false);
        Rc::new(Self {
            enabled: Rc::new(Cell::new(enabled)),
            worker: spawn(database_path(conn)),
            subscribers: RefCell::new(Vec::new()),
        })
    }

    pub(in crate::ui) fn set_enabled(
        &self,
        conn: &rusqlite::Connection,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(conn, &reprise_core::modules::PODCASTS_MODULE, enabled)?;
        if self.enabled.replace(enabled) != enabled {
            for callback in self.subscribers.borrow().iter() {
                callback(enabled);
            }
        }
        Ok(())
    }

    pub(in crate::ui) fn subscribe_enabled(&self, callback: impl Fn(bool) + 'static) {
        let callback: OnEnabled = Rc::new(callback);
        callback(self.enabled.get());
        self.subscribers.borrow_mut().push(callback);
    }

    pub(in crate::ui) fn request(&self, request: PodcastsRequest) -> bool {
        if !self.enabled.get() {
            return false;
        }
        match self.worker.try_send(request) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "could not queue podcast work");
                false
            }
        }
    }

    pub(in crate::ui) fn automatic_refresh_allowed(
        &self,
        subscription_count: usize,
        metered: bool,
        due: bool,
    ) -> bool {
        automatic_refresh_allowed(self.enabled.get(), subscription_count, metered, due)
    }
}

pub(in crate::ui) fn automatic_refresh_allowed(
    enabled: bool,
    subscription_count: usize,
    metered: bool,
    due: bool,
) -> bool {
    enabled && subscription_count > 0 && !metered && due
}

fn database_path(conn: &rusqlite::Connection) -> Option<PathBuf> {
    let mut statement = conn.prepare("PRAGMA database_list").ok()?;
    let mut rows = statement.query([]).ok()?;
    while let Some(row) = rows.next().ok()? {
        if row.get::<_, String>(1).ok()?.as_str() == "main" {
            let path = row.get::<_, String>(2).ok()?;
            return (!path.is_empty()).then(|| PathBuf::from(path));
        }
    }
    None
}

fn spawn(database_path: Option<PathBuf>) -> async_channel::Sender<PodcastsRequest> {
    let (sender, receiver) = async_channel::unbounded::<PodcastsRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-podcasts".into())
        .spawn(move || {
            let connection = database_path
                .as_deref()
                .map(|path| reprise_core::db::open_migrated(Some(path)));
            while let Ok(request) = receiver.recv_blocking() {
                process_request(connection.as_ref(), &request);
            }
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not start podcast worker");
    }
    sender
}

fn process_request(
    connection: Option<&Result<rusqlite::Connection, reprise_core::db::DbError>>,
    request: &PodcastsRequest,
) {
    let Some(Ok(conn)) = connection else {
        let error = connection
            .and_then(|result| result.as_ref().err())
            .map_or_else(
                || "the active database has no persistent path".to_owned(),
                ToString::to_string,
            );
        send_response(request, Err(error));
        return;
    };
    match request.operation {
        PodcastsOperation::Refresh { force } => {
            let result = podcasts::config::load(conn)
                .map_err(|error| error.to_string())
                .and_then(|config| {
                    let ytdlp = podcasts::ytdlp::YtDlp::discover(config.ytdlp_path.as_deref());
                    podcasts::pipeline::refresh_with_download_progress(
                        conn,
                        &podcasts::pipeline::HttpFeedFetcher,
                        &ytdlp,
                        chrono::Utc::now().timestamp(),
                        force,
                        &mut |episode_id, state| {
                            send_response(
                                request,
                                Ok(PodcastsWorkerResult::DownloadState { episode_id, state }),
                            );
                        },
                    )
                    .map(PodcastsWorkerResult::Refreshed)
                    .map_err(|error| error.to_string())
                });
            send_response(request, result);
        }
        PodcastsOperation::Download { episode_id } => {
            download_episode(conn, episode_id, &mut |state| {
                send_response(
                    request,
                    Ok(PodcastsWorkerResult::DownloadState { episode_id, state }),
                );
            });
        }
    }
}

fn send_response(request: &PodcastsRequest, result: Result<PodcastsWorkerResult, String>) {
    let terminal = match &result {
        Err(_) | Ok(PodcastsWorkerResult::Refreshed(_)) => true,
        Ok(PodcastsWorkerResult::DownloadState { state, .. }) => matches!(
            state,
            podcasts::download_state::DownloadState::Downloaded { .. }
                | podcasts::download_state::DownloadState::Failed { .. }
        ),
    };
    let response = PodcastsResponse {
        generation: request.generation,
        result,
    };
    if terminal {
        request.response.publish_terminal(response);
    } else {
        request.response.publish_latest(response);
    }
}

fn download_episode(
    conn: &rusqlite::Connection,
    episode_id: i64,
    emit: &mut dyn FnMut(podcasts::download_state::DownloadState),
) {
    let config = match podcasts::config::load(conn) {
        Ok(config) => config,
        Err(error) => {
            emit(podcasts::download_state::DownloadState::Queued);
            emit(podcasts::download_state::DownloadState::Downloading {
                received_bytes: 0,
                total_bytes: None,
            });
            emit(podcasts::download_state::DownloadState::Failed {
                message: error.to_string(),
            });
            return;
        }
    };
    let ytdlp = podcasts::ytdlp::YtDlp::discover(config.ytdlp_path.as_deref());
    download_episode_to_root(
        conn,
        episode_id,
        &podcasts::downloads::default_download_root(),
        &podcasts::pipeline::HttpFeedFetcher,
        &ytdlp,
        emit,
    );
}

fn download_episode_to_root(
    conn: &rusqlite::Connection,
    episode_id: i64,
    download_root: &std::path::Path,
    feed_fetcher: &dyn podcasts::pipeline::FeedFetcher,
    youtube_fetcher: &dyn podcasts::pipeline::YoutubeFetcher,
    emit: &mut dyn FnMut(podcasts::download_state::DownloadState),
) {
    use podcasts::download_state::DownloadState;

    emit(DownloadState::Queued);
    emit(DownloadState::Downloading {
        received_bytes: 0,
        total_bytes: None,
    });
    let result = try_download_episode_to_root(
        conn,
        episode_id,
        download_root,
        feed_fetcher,
        youtube_fetcher,
        emit,
    );
    match result {
        Ok(bytes) => emit(DownloadState::Downloaded { bytes }),
        Err(message) => emit(DownloadState::Failed { message }),
    }
}

fn try_download_episode_to_root(
    conn: &rusqlite::Connection,
    episode_id: i64,
    download_root: &std::path::Path,
    feed_fetcher: &dyn podcasts::pipeline::FeedFetcher,
    youtube_fetcher: &dyn podcasts::pipeline::YoutubeFetcher,
    emit: &mut dyn FnMut(podcasts::download_state::DownloadState),
) -> Result<u64, String> {
    let episode = podcasts::store::episode(conn, episode_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "podcast episode no longer exists".to_owned())?;
    let subscription = podcasts::store::subscription(conn, episode.subscription_id)
        .map_err(|error| error.to_string())?
        .filter(|subscription| subscription.removed_at.is_none())
        .ok_or_else(|| "podcast subscription no longer exists".to_owned())?;
    let extension = match episode.kind {
        podcasts::PodcastKind::Rss => podcasts::downloads::extension_from_url(&episode.audio_url),
        podcasts::PodcastKind::Youtube => "audio",
    };
    let destination = podcasts::downloads::download_path(
        download_root,
        &subscription.feed_url,
        &episode.guid,
        extension,
    );
    let mut state = podcasts::download_state::DownloadState::Downloading {
        received_bytes: 0,
        total_bytes: None,
    };
    let bytes = podcasts::downloads::download_atomically(&destination, |temporary| {
        let mut on_progress = |progress: podcasts::download_state::DownloadProgress| {
            state = podcasts::download_state::downloading(
                &state,
                progress.received_bytes,
                progress.total_bytes,
            );
            emit(state.clone());
        };
        match episode.kind {
            podcasts::PodcastKind::Rss => {
                feed_fetcher.download_with_progress(&episode.audio_url, temporary, &mut on_progress)
            }
            podcasts::PodcastKind::Youtube => youtube_fetcher.download_with_progress(
                &episode.audio_url,
                temporary,
                &mut on_progress,
            ),
        }
    })
    .map_err(|error| error.to_string())?;

    let destination_path = destination.to_str().ok_or_else(|| {
        let _ = std::fs::remove_file(&destination);
        "podcast download path is not valid UTF-8".to_owned()
    })?;
    let persisted =
        podcasts::downloads::persist_completed_if_active(conn, episode.id, destination_path, bytes)
            .map_err(|error| {
                let _ = std::fs::remove_file(&destination);
                error.to_string()
            })?;
    if !persisted {
        let _ = std::fs::remove_file(&destination);
        return Err("podcast episode no longer exists".to_owned());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
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

    fn episode(conn: &rusqlite::Connection) -> i64 {
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
    fn pod_7_download_request_does_not_invalidate_an_in_flight_refresh() {
        assert_eq!(
            request_generation(9, PodcastsOperation::Download { episode_id: 4 }),
            9
        );
        assert_eq!(
            request_generation(9, PodcastsOperation::Refresh { force: true }),
            10
        );
    }

    #[test]
    fn pod_7_download_worker_emits_ordered_monotone_states_and_persists_after_publish() {
        let conn = reprise_core::db::open_migrated(None).unwrap();
        let episode_id = episode(&conn);
        let directory = tempfile::tempdir().unwrap();
        let mut states = Vec::new();

        download_episode_to_root(
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
        let conn = reprise_core::db::open_migrated(None).unwrap();
        let episode_id = episode(&conn);
        let directory = tempfile::tempdir().unwrap();
        let mut states = Vec::new();

        download_episode_to_root(
            &conn,
            episode_id,
            directory.path(),
            &ProgressFeed { fail: true },
            &NeverYoutube,
            &mut |state| states.push(state),
        );

        assert!(matches!(
            states.last(),
            Some(DownloadState::Failed { message }) if message == "network request failed: offline"
        ));
        assert!(store::episode(&conn, episode_id)
            .unwrap()
            .unwrap()
            .downloaded_path
            .is_none());
        assert!(walk_files(directory.path()).is_empty());
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
        let conn = reprise_core::db::open_migrated(None).unwrap();
        let episode_id = episode(&conn);
        let directory = tempfile::tempdir().unwrap();
        let feed = RemovingFeed {
            conn: &conn,
            episode_id,
        };
        let mut states = Vec::new();

        download_episode_to_root(
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
        conn: &'a rusqlite::Connection,
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
}
