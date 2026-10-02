//! Long-lived podcast refresh and download worker.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use reprise_core::db::Db;
use reprise_core::podcasts;

#[path = "podcasts_worker_lanes.rs"]
mod lanes;
use lanes::{lane_for, spawn_lane, LaneExecutor, PodcastsLane};

#[derive(Clone, Debug)]
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
    SyncSubscription {
        subscription_id: i64,
        abort: podcasts::pipeline::SyncAbort,
    },
    /// Brings every subscription up to its `keep_downloaded` target after a
    /// refresh, without making the refresh wait for a potentially large
    /// first-run backlog.
    FillDownloads,
}

pub(in crate::ui) const fn request_generation(current: u64, operation: &PodcastsOperation) -> u64 {
    match operation {
        PodcastsOperation::Refresh { .. } | PodcastsOperation::LoadMore { .. } => {
            current.wrapping_add(1)
        }
        // Neither is allowed to cancel a refresh/load-more already in
        // flight, and both are themselves allowed to keep running alongside
        // one — same non-cancelling treatment `Download` already has.
        PodcastsOperation::Download { .. }
        | PodcastsOperation::SyncSubscription { .. }
        | PodcastsOperation::FillDownloads => current,
    }
}

#[derive(Debug)]
pub(in crate::ui) struct PodcastsRequest {
    pub generation: u64,
    pub operation: PodcastsOperation,
    pub response: PodcastsResponseChannel,
}

/// A request together with the instant it entered the worker's channel.
/// Logged when its lane dequeues it so a slow refresh can be told apart from
/// a refresh that queued behind other feed work.
pub(super) struct QueuedRequest {
    request: PodcastsRequest,
    queued_at: Instant,
}

#[derive(Debug)]
pub(in crate::ui) struct PodcastsResponse {
    pub generation: u64,
    pub result: Result<PodcastsWorkerResult, String>,
}

#[derive(Debug)]
pub(in crate::ui) struct PodcastsResponseChannel {
    sender: async_channel::Sender<PodcastsResponse>,
}

pub(in crate::ui) fn podcasts_response_channel() -> (
    PodcastsResponseChannel,
    async_channel::Receiver<PodcastsResponse>,
) {
    let (sender, receiver) = async_channel::bounded(1);
    (PodcastsResponseChannel { sender }, receiver)
}

impl PodcastsResponseChannel {
    fn publish_latest(&self, response: PodcastsResponse) {
        if let Err(error) = self.sender.force_send(response) {
            tracing::debug!(%error, "podcast worker response receiver is unavailable");
        }
    }

    fn publish_terminal(&self, response: PodcastsResponse) {
        if let Err(error) = self.sender.force_send(response) {
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
    SyncProgress {
        subscription_id: i64,
        progress: podcasts::pipeline::SyncProgress,
    },
    Filled(podcasts::fill_downloads::FillSummary),
}

type OnEnabled = Rc<dyn Fn(bool)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FillRequestState {
    running: bool,
    pending: bool,
}

impl FillRequestState {
    fn request(&mut self) -> bool {
        if self.running {
            self.pending = true;
            return false;
        }
        self.running = true;
        true
    }

    fn complete(&mut self) -> bool {
        self.running = false;
        std::mem::take(&mut self.pending)
    }

    fn cancel(&mut self) {
        self.running = false;
        self.pending = false;
    }
}

/// Issue #96 / `NET-1a`: `enabled` is true when *either* Podcasts (RSS) or
/// YouTube is network-allowed (its own module AND the global online-sources
/// gate) — "Podcasts off + YouTube on" must still dispatch work. Which
/// subscriptions actually get fetched is then decided per-kind, deeper in
/// `podcasts::pipeline`, which is the one authority for that gate.
pub(in crate::ui) struct PodcastsRuntime {
    pub enabled: Rc<Cell<bool>>,
    feeds: async_channel::Sender<QueuedRequest>,
    downloads: async_channel::Sender<QueuedRequest>,
    subscribers: RefCell<Vec<OnEnabled>>,
    fill_request: Cell<FillRequestState>,
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
        let executor: LaneExecutor = Arc::new(process_request);
        let database_path = conn.path();
        Rc::new(Self {
            enabled: Rc::new(Cell::new(any_source_dispatchable(conn))),
            feeds: spawn_lane(
                PodcastsLane::Feeds,
                database_path.clone(),
                Arc::clone(&executor),
            ),
            downloads: spawn_lane(PodcastsLane::Downloads, database_path, executor),
            subscribers: RefCell::new(Vec::new()),
            fill_request: Cell::new(FillRequestState::default()),
        })
    }

    #[cfg(test)]
    pub(super) fn new_for_test(enabled: bool, executor: LaneExecutor) -> Rc<Self> {
        Rc::new(Self {
            enabled: Rc::new(Cell::new(enabled)),
            feeds: spawn_lane(PodcastsLane::Feeds, None, Arc::clone(&executor)),
            downloads: spawn_lane(PodcastsLane::Downloads, None, executor),
            subscribers: RefCell::new(Vec::new()),
            fill_request: Cell::new(FillRequestState::default()),
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
        let queued = QueuedRequest {
            request,
            queued_at: Instant::now(),
        };
        let sender = match lane_for(&queued.request.operation) {
            PodcastsLane::Feeds => &self.feeds,
            PodcastsLane::Downloads => &self.downloads,
        };
        match sender.try_send(queued) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "could not queue podcast work");
                false
            }
        }
    }

    pub(in crate::ui) fn begin_fill_request(&self) -> bool {
        let mut state = self.fill_request.get();
        let begin = state.request();
        self.fill_request.set(state);
        begin
    }

    pub(in crate::ui) fn finish_fill_request(&self) -> bool {
        let mut state = self.fill_request.get();
        let replay = state.complete();
        self.fill_request.set(state);
        replay
    }

    pub(in crate::ui) fn cancel_fill_request(&self) {
        let mut state = self.fill_request.get();
        state.cancel();
        self.fill_request.set(state);
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DownloadJobCounts {
    downloaded: usize,
    failed: usize,
}

impl DownloadJobCounts {
    const fn episodes(self) -> usize {
        self.downloaded + self.failed
    }

    fn record(&mut self, state: &podcasts::download_state::DownloadState) {
        let next = count_download_states(std::slice::from_ref(state));
        self.downloaded += next.downloaded;
        self.failed += next.failed;
    }
}

fn count_download_states(states: &[podcasts::download_state::DownloadState]) -> DownloadJobCounts {
    let mut counts = DownloadJobCounts::default();
    for state in states {
        match state {
            podcasts::download_state::DownloadState::Downloaded { .. } => counts.downloaded += 1,
            podcasts::download_state::DownloadState::Failed { .. } => counts.failed += 1,
            podcasts::download_state::DownloadState::NotDownloaded
            | podcasts::download_state::DownloadState::Queued
            | podcasts::download_state::DownloadState::Downloading { .. }
            | podcasts::download_state::DownloadState::Missing => {}
        }
    }
    counts
}

#[derive(Clone, Copy)]
enum DownloadJobOutcome {
    Ok,
    Error,
    AlreadyRunning,
}

impl DownloadJobOutcome {
    const fn name(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::AlreadyRunning => "already_running",
        }
    }
}

fn log_download_job(
    request: &PodcastsRequest,
    started_at: Instant,
    counts: DownloadJobCounts,
    outcome: DownloadJobOutcome,
) {
    tracing::info!(
        lane = PodcastsLane::Downloads.name(),
        operation = ?request.operation,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        episodes = counts.episodes(),
        downloaded = counts.downloaded,
        failed = counts.failed,
        outcome = outcome.name(),
        "podcast download job finished"
    );
}

fn process_request(
    connection: Option<&Result<Db, reprise_core::db::DbError>>,
    queued: &QueuedRequest,
) {
    let request = &queued.request;
    let started_at = Instant::now();
    // Each lane owns and reuses one connection, preserving refresh's
    // connection-address-keyed retry state while downloads run independently.
    let Some(Ok(conn)) = connection else {
        let error = connection
            .and_then(|result| result.as_ref().err())
            .map_or_else(
                || "the active database has no persistent path".to_owned(),
                ToString::to_string,
            );
        send_response(request, Err(error));
        if lane_for(&request.operation) == PodcastsLane::Downloads {
            log_download_job(
                request,
                started_at,
                DownloadJobCounts::default(),
                DownloadJobOutcome::Error,
            );
        }
        return;
    };
    match &request.operation {
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
                        podcasts::refresh::RefreshRequest {
                            policy: *policy,
                            kind: *kind,
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
                    let ytdlp =
                        super::metadata_ytdlp(config.ytdlp_path.as_deref(), config.youtube_browser);
                    podcasts::pipeline::load_more_youtube(
                        conn,
                        &ytdlp,
                        *subscription_id,
                        *end,
                        chrono::Utc::now().timestamp(),
                    )
                    .map(|_| PodcastsWorkerResult::LoadedMore {
                        subscription_id: *subscription_id,
                        end: *end,
                    })
                    .map_err(|error| error.to_string())
                });
            send_response(request, result);
        }
        PodcastsOperation::Download { episode_id } => {
            let (counts, outcome) = download_episode(conn, request, *episode_id);
            log_download_job(request, started_at, counts, outcome);
        }
        PodcastsOperation::SyncSubscription {
            subscription_id,
            abort,
        } => {
            let config = match podcasts::config::load(conn) {
                Ok(config) => config,
                Err(error) => {
                    send_response(request, Err(error.to_string()));
                    return;
                }
            };
            let ytdlp = super::metadata_ytdlp(config.ytdlp_path.as_deref(), config.youtube_browser);
            let result = podcasts::pipeline::sync_subscription(
                conn,
                &podcasts::pipeline::HttpFeedFetcher,
                &ytdlp,
                chrono::Utc::now().timestamp(),
                *subscription_id,
                abort,
                &mut |progress| {
                    send_response(
                        request,
                        Ok(PodcastsWorkerResult::SyncProgress {
                            subscription_id: *subscription_id,
                            progress,
                        }),
                    );
                },
            );
            if let Err(error) = result {
                if !abort.is_cancelled() {
                    tracing::debug!(%error, subscription_id, "podcast subscription sync ended with an error");
                }
            }
        }
        PodcastsOperation::FillDownloads => {
            let mut counts = DownloadJobCounts::default();
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
                            counts.record(&state);
                            send_response(
                                request,
                                Ok(PodcastsWorkerResult::DownloadState { episode_id, state }),
                            );
                        },
                    )
                    .map(PodcastsWorkerResult::Filled)
                    .map_err(|error| error.to_string())
                });
            let (counts, outcome) = match &result {
                Ok(PodcastsWorkerResult::Filled(summary)) => (
                    DownloadJobCounts {
                        downloaded: summary.downloaded,
                        failed: summary.failed,
                    },
                    DownloadJobOutcome::Ok,
                ),
                Ok(_) => unreachable!("fill result has one terminal variant"),
                Err(_) => (counts, DownloadJobOutcome::Error),
            };
            send_response(request, result);
            log_download_job(request, started_at, counts, outcome);
        }
    }
}

fn send_response(request: &PodcastsRequest, result: Result<PodcastsWorkerResult, String>) {
    let terminal = result.as_ref().map_or(true, worker_result_is_terminal);
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

fn worker_result_is_terminal(result: &PodcastsWorkerResult) -> bool {
    match result {
        PodcastsWorkerResult::Refreshed(_)
        | PodcastsWorkerResult::LoadedMore { .. }
        | PodcastsWorkerResult::Filled(_) => true,
        PodcastsWorkerResult::DownloadState { state, .. } => matches!(
            state,
            podcasts::download_state::DownloadState::Downloaded { .. }
                | podcasts::download_state::DownloadState::Failed { .. }
        ),
        PodcastsWorkerResult::SyncProgress { progress, .. } => matches!(
            progress,
            podcasts::pipeline::SyncProgress::Done(_) | podcasts::pipeline::SyncProgress::Failed(_)
        ),
    }
}

/// `POD-7`: the worker's only download executor is
/// `reprise_core::podcasts::pipeline::download_episode` — the same body the
/// fill-up and MCP's `music_manage_episodes` already call. There is no second
/// episode lookup, `NET-1a` check, `.part` handling, or progress emission
/// here; this just wires up the fetchers and forwards progress/terminal states
/// onto the response channel.
fn download_episode(
    conn: &Db,
    request: &PodcastsRequest,
    episode_id: i64,
) -> (DownloadJobCounts, DownloadJobOutcome) {
    let config = match podcasts::config::load(conn) {
        Ok(config) => config,
        Err(error) => {
            send_response(request, Err(error.to_string()));
            return (DownloadJobCounts::default(), DownloadJobOutcome::Error);
        }
    };
    let ytdlp = podcasts::ytdlp::YtDlp::discover_with_browser(
        config.ytdlp_path.as_deref(),
        config.youtube_browser,
    );
    let download_root = podcasts::downloads::default_download_root();
    let mut counts = DownloadJobCounts::default();
    let result = podcasts::pipeline::download_episode(
        conn,
        &podcasts::pipeline::HttpFeedFetcher,
        &ytdlp,
        &download_root,
        episode_id,
        &mut |state| {
            counts.record(&state);
            send_response(
                request,
                Ok(PodcastsWorkerResult::DownloadState { episode_id, state }),
            );
        },
    );
    // Losing the download claim is normal: another caller owns an active
    // download, so keep the row in progress. Other errors remain terminal.
    match result {
        Ok(_) => (counts, DownloadJobOutcome::Ok),
        Err(error) => {
            if let Some(state) = download_error_state(&error) {
                send_response(
                    request,
                    Ok(PodcastsWorkerResult::DownloadState { episode_id, state }),
                );
                (counts, DownloadJobOutcome::AlreadyRunning)
            } else {
                send_response(request, Err(error.to_string()));
                (counts, DownloadJobOutcome::Error)
            }
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

#[cfg(test)]
#[path = "podcasts_worker_lane_tests.rs"]
mod lane_tests;
