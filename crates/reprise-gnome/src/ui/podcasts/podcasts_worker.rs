//! Long-lived podcast refresh and download worker.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::podcasts;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PodcastsOperation {
    Refresh {
        policy: podcasts::refresh::RefreshPolicy,
        kind: Option<podcasts::PodcastKind>,
    },
    LoadMore {
        subscription_id: i64,
        end: usize,
    },
    Download {
        episode_id: i64,
    },
    /// Brings every subscription up to its `keep_downloaded` target after a
    /// refresh, without making the refresh wait for a potentially large
    /// first-run backlog.
    FillDownloads,
}

pub(in crate::ui) const fn request_generation(current: u64, operation: PodcastsOperation) -> u64 {
    match operation {
        PodcastsOperation::Refresh { .. } | PodcastsOperation::LoadMore { .. } => {
            current.wrapping_add(1)
        }
        // Neither is allowed to cancel a refresh/load-more already in
        // flight, and both are themselves allowed to keep running alongside
        // one — same non-cancelling treatment `Download` already has.
        PodcastsOperation::Download { .. } | PodcastsOperation::FillDownloads => current,
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
    Filled(podcasts::fill_downloads::FillSummary),
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

fn any_source_dispatchable(conn: &Db) -> bool {
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
    pub(in crate::ui) fn setup(conn: &Db) -> Rc<Self> {
        let worker = spawn(conn.path());
        Rc::new(Self {
            enabled: Rc::new(Cell::new(any_source_dispatchable(conn))),
            worker,
            subscribers: RefCell::new(Vec::new()),
        })
    }

    fn set_module_enabled(
        &self,
        conn: &Db,
        module: &'static reprise_core::modules::ModuleDescriptor,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        reprise_core::modules::set_enabled(conn, module, enabled)?;
        self.recompute_enabled(conn);
        Ok(())
    }

    pub(in crate::ui) fn set_podcasts_enabled(
        &self,
        conn: &Db,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        self.set_module_enabled(conn, &reprise_core::modules::PODCASTS_MODULE, enabled)
    }

    pub(in crate::ui) fn set_youtube_enabled(
        &self,
        conn: &Db,
        enabled: bool,
    ) -> Result<(), rusqlite::Error> {
        self.set_module_enabled(conn, &reprise_core::modules::YOUTUBE_MODULE, enabled)
    }

    /// Re-derives `enabled` from persisted state and notifies subscribers on
    /// change. Called after either source module toggles, and after the
    /// global online-sources gate toggles (from the Online sources page).
    pub(in crate::ui) fn recompute_enabled(&self, conn: &Db) {
        let enabled = any_source_dispatchable(conn);
        if self.enabled.replace(enabled) != enabled {
            let subscribers = self.subscribers.borrow().clone();
            for callback in subscribers {
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

fn spawn(database_path: Option<PathBuf>) -> async_channel::Sender<PodcastsRequest> {
    let (sender, receiver) = async_channel::unbounded::<PodcastsRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-podcasts".into())
        .spawn(move || {
            let connection = database_path
                .as_deref()
                .map(|path| reprise_core::db::Db::open_migrated(Some(path)));
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
    connection: Option<&Result<Db, reprise_core::db::DbError>>,
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
        PodcastsOperation::Refresh { policy, kind } => {
            let result = podcasts::config::load(conn)
                .map_err(|error| error.to_string())
                .and_then(|config| {
                    let ytdlp =
                        super::metadata_ytdlp(config.ytdlp_path.as_deref(), config.youtube_browser);
                    podcasts::pipeline::refresh(
                        conn,
                        &podcasts::pipeline::HttpFeedFetcher,
                        &ytdlp,
                        chrono::Utc::now().timestamp(),
                        podcasts::refresh::RefreshRequest { policy, kind },
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
                    let ytdlp =
                        super::metadata_ytdlp(config.ytdlp_path.as_deref(), config.youtube_browser);
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
            download_episode(conn, request, episode_id);
        }
        PodcastsOperation::FillDownloads => {
            let result = podcasts::config::load(conn)
                .map_err(|error| error.to_string())
                .and_then(|config| {
                    let ytdlp = podcasts::ytdlp::YtDlp::discover_with_browser(
                        config.ytdlp_path.as_deref(),
                        config.youtube_browser,
                    );
                    podcasts::fill_downloads::fill_downloads(
                        conn,
                        &podcasts::pipeline::HttpFeedFetcher,
                        &ytdlp,
                        &podcasts::downloads::default_download_root(),
                        &mut |episode_id, state| {
                            send_response(
                                request,
                                Ok(PodcastsWorkerResult::DownloadState { episode_id, state }),
                            );
                        },
                    )
                    .map(PodcastsWorkerResult::Filled)
                    .map_err(|error| error.to_string())
                });
            send_response(request, result);
        }
    }
}

fn send_response(request: &PodcastsRequest, result: Result<PodcastsWorkerResult, String>) {
    let terminal = match &result {
        Err(_)
        | Ok(
            PodcastsWorkerResult::Refreshed(_)
            | PodcastsWorkerResult::LoadedMore { .. }
            | PodcastsWorkerResult::Filled(_),
        ) => true,
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

/// `POD-7`: the worker's only download executor is
/// `reprise_core::podcasts::pipeline::download_episode` — the same body the
/// fill-up and MCP's `music_manage_episodes` already call. There is no second
/// episode lookup, `NET-1a` check, `.part` handling, or progress emission
/// here; this just wires up the fetchers and forwards progress/terminal states
/// onto the response channel.
fn download_episode(conn: &Db, request: &PodcastsRequest, episode_id: i64) {
    let config = match podcasts::config::load(conn) {
        Ok(config) => config,
        Err(error) => {
            send_response(request, Err(error.to_string()));
            return;
        }
    };
    let ytdlp = podcasts::ytdlp::YtDlp::discover_with_browser(
        config.ytdlp_path.as_deref(),
        config.youtube_browser,
    );
    let download_root = podcasts::downloads::default_download_root();
    let result = podcasts::pipeline::download_episode(
        conn,
        &podcasts::pipeline::HttpFeedFetcher,
        &ytdlp,
        &download_root,
        episode_id,
        &mut |state| {
            send_response(
                request,
                Ok(PodcastsWorkerResult::DownloadState { episode_id, state }),
            );
        },
    );
    // Losing the download claim is normal: another caller owns an active
    // download, so keep the row in progress. Other errors remain terminal.
    if let Err(error) = result {
        if let Some(state) = download_error_state(&error) {
            send_response(
                request,
                Ok(PodcastsWorkerResult::DownloadState { episode_id, state }),
            );
        } else {
            send_response(request, Err(error.to_string()));
        }
    }
}

fn download_error_state(
    error: &podcasts::pipeline::PipelineError,
) -> Option<podcasts::download_state::DownloadState> {
    matches!(
        error,
        podcasts::pipeline::PipelineError::DownloadAlreadyRunning
    )
    .then_some(podcasts::download_state::DownloadState::Downloading {
        received_bytes: 0,
        total_bytes: None,
    })
}

#[cfg(test)]
#[path = "podcasts_worker_tests.rs"]
mod tests;
