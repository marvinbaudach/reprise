//! Long-lived podcast refresh and download worker.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::podcasts;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PodcastsOperation {
    Refresh { force: bool },
    LoadMore { subscription_id: i64, end: usize },
    Download { episode_id: i64 },
}

pub(in crate::ui) const fn request_generation(current: u64, operation: PodcastsOperation) -> u64 {
    match operation {
        PodcastsOperation::Refresh { .. } | PodcastsOperation::LoadMore { .. } => {
            current.wrapping_add(1)
        }
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
    LoadedMore {
        subscription_id: i64,
        end: usize,
    },
    DownloadState {
        episode_id: i64,
        state: podcasts::download_state::DownloadState,
    },
}

type OnEnabled = Rc<dyn Fn(bool)>;

/// Issue #96 / `NET-1a`: `enabled` is true when *either* Podcasts (RSS) or
/// YouTube is network-allowed (its own module AND the global online-sources
/// gate) — "Podcasts off + YouTube on" must still dispatch work. Which
/// subscriptions actually get fetched is then decided per-kind, deeper in
/// `podcasts::pipeline`, which is the one authority for that gate.
pub(in crate::ui) struct PodcastsRuntime {
    pub enabled: Rc<Cell<bool>>,
    worker: async_channel::Sender<PodcastsRequest>,
    subscribers: RefCell<Vec<OnEnabled>>,
}

fn any_source_dispatchable(conn: &rusqlite::Connection) -> bool {
    reprise_core::podcasts::config::source_network_allowed(
        conn,
        reprise_core::podcasts::PodcastKind::Rss,
    )
    .unwrap_or(false)
        || reprise_core::podcasts::config::source_network_allowed(
            conn,
            reprise_core::podcasts::PodcastKind::Youtube,
        )
        .unwrap_or(false)
}

impl PodcastsRuntime {
    pub(in crate::ui) fn setup(conn: &rusqlite::Connection) -> Rc<Self> {
        Rc::new(Self {
            enabled: Rc::new(Cell::new(any_source_dispatchable(conn))),
            worker: spawn(database_path(conn)),
            subscribers: RefCell::new(Vec::new()),
        })
    }

    fn set_module_enabled(
        &self,
        conn: &rusqlite::Connection,
        module: &'static reprise_core::modules::ModuleDescriptor,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(conn, module, enabled)?;
        self.recompute_enabled(conn);
        Ok(())
    }

    pub(in crate::ui) fn set_podcasts_enabled(
        &self,
        conn: &rusqlite::Connection,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        self.set_module_enabled(conn, &reprise_core::modules::PODCASTS_MODULE, enabled)
    }

    pub(in crate::ui) fn set_youtube_enabled(
        &self,
        conn: &rusqlite::Connection,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        self.set_module_enabled(conn, &reprise_core::modules::YOUTUBE_MODULE, enabled)
    }

    /// Re-derives `enabled` from persisted state and notifies subscribers on
    /// change. Called after either source module toggles, and after the
    /// global online-sources gate toggles (from the Online sources page).
    pub(in crate::ui) fn recompute_enabled(&self, conn: &rusqlite::Connection) {
        let enabled = any_source_dispatchable(conn);
        if self.enabled.replace(enabled) != enabled {
            for callback in self.subscribers.borrow().iter() {
                callback(enabled);
            }
        }
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
        PodcastsOperation::LoadMore {
            subscription_id,
            end,
        } => {
            let result = podcasts::config::load(conn)
                .map_err(|error| error.to_string())
                .and_then(|config| {
                    let ytdlp = podcasts::ytdlp::YtDlp::discover(config.ytdlp_path.as_deref());
                    podcasts::pipeline::load_more_youtube(
                        conn,
                        &ytdlp,
                        subscription_id,
                        end,
                        chrono::Utc::now().timestamp(),
                    )
                    .map(|_| PodcastsWorkerResult::LoadedMore {
                        subscription_id,
                        end,
                    })
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
        Err(_)
        | Ok(PodcastsWorkerResult::Refreshed(_) | PodcastsWorkerResult::LoadedMore { .. }) => true,
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
    // NET-1a: a download is a network entry point too — gate it per the
    // episode's own source kind, not the blanket dispatch check above.
    if !podcasts::config::source_network_allowed(conn, episode.kind)
        .map_err(|error| error.to_string())?
    {
        return Err("this source is disabled".to_owned());
    }
    let subscription = podcasts::store::subscription(conn, episode.subscription_id)
        .map_err(|error| error.to_string())?
        .filter(|subscription| subscription.removed_at.is_none())
        .ok_or_else(|| "podcast subscription no longer exists".to_owned())?;
    let extension = download_extension(episode.kind, &episode.audio_url);
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

fn download_extension(kind: podcasts::PodcastKind, audio_url: &str) -> &'static str {
    podcasts::downloads::extension_for(kind, audio_url)
}

#[cfg(test)]
#[path = "podcasts_worker_tests.rs"]
mod tests;
