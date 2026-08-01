//! GTK runtime adapter for library-wide lyrics cache population.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use gtk4::glib;
use reprise_core::db::Db;
pub(in crate::ui) use reprise_core::lyrics::{
    BatchProgress as LyricsBatchProgress, BatchState as LyricsBatchState,
};
use reprise_core::lyrics::{BatchRunStatus, BatchTrack};

use super::cover_download_batch::{
    BatchProgress as CoverBatchProgress, BatchState as CoverBatchState, CoverDownloadBatch,
};
use super::progress_subscribers::ProgressSubscribers;
use super::scan_flow::ScanCancellation;

struct WorkerRequest {
    generation: u64,
    generation_source: Arc<AtomicU64>,
    cancellation: ScanCancellation,
    enabled: Arc<AtomicBool>,
    tracks: Vec<BatchTrack>,
    events: async_channel::Sender<WorkerEvent>,
}

enum WorkerEvent {
    Progress(LyricsBatchProgress),
    Cancelled,
}

#[derive(Clone)]
struct LyricsBatchWorker {
    sender: async_channel::Sender<WorkerRequest>,
}

impl LyricsBatchWorker {
    fn production() -> Self {
        let (sender, receiver) = async_channel::unbounded::<WorkerRequest>();
        let result = std::thread::Builder::new()
            .name("reprise-lyrics-batch".into())
            .spawn(move || {
                while let Ok(request) = receiver.recv_blocking() {
                    let status = reprise_core::lyrics::run_batch(
                        &request.tracks,
                        || cancelled(&request),
                        || request.enabled.load(Ordering::Relaxed),
                        |progress| {
                            request
                                .events
                                .send_blocking(WorkerEvent::Progress(progress))
                                .is_ok()
                        },
                    );
                    if status == BatchRunStatus::Cancelled {
                        let _ = request.events.send_blocking(WorkerEvent::Cancelled);
                    }
                }
            });
        if let Err(error) = result {
            tracing::warn!(%error, "could not start lyrics batch worker");
        }
        Self { sender }
    }
}

pub(in crate::ui) struct LyricsBatch {
    conn: Rc<Db>,
    worker: LyricsBatchWorker,
    cancellation: ScanCancellation,
    enabled: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    progress: Cell<LyricsBatchProgress>,
    subscribers: ProgressSubscribers<LyricsBatchProgress>,
}

impl LyricsBatch {
    pub(in crate::ui) fn new(conn: &Rc<Db>) -> Rc<Self> {
        Rc::new(Self {
            conn: conn.clone(),
            worker: LyricsBatchWorker::production(),
            cancellation: ScanCancellation::default(),
            enabled: Arc::new(AtomicBool::new(network_allowed(conn))),
            generation: Arc::new(AtomicU64::new(0)),
            progress: Cell::new(LyricsBatchProgress::idle()),
            subscribers: ProgressSubscribers::default(),
        })
    }

    /// Re-publishes the live gate and starts only an off-to-on transition.
    pub(in crate::ui) fn recompute_enabled(self: &Rc<Self>) {
        if self.store_enabled() {
            self.start();
        }
    }

    fn store_enabled(&self) -> bool {
        let allowed = network_allowed(&self.conn);
        !self.enabled.swap(allowed, Ordering::Relaxed) && allowed
    }

    pub(in crate::ui) fn cancel(&self) {
        self.cancellation.request();
    }

    #[cfg(test)]
    pub(in crate::ui) fn is_cancel_requested(&self) -> bool {
        self.cancellation.is_requested()
    }

    pub(in crate::ui) fn subscribe_progress(
        &self,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn(LyricsBatchProgress) + 'static,
    ) {
        self.subscribers
            .subscribe(self.progress.get(), is_alive, callback);
    }

    pub(in crate::ui) fn start(self: &Rc<Self>) {
        self.store_enabled();
        if !self.enabled.load(Ordering::Relaxed) {
            self.set_progress(LyricsBatchProgress::idle());
            return;
        }
        let summaries = match reprise_core::queries::query_live_track_summaries(&self.conn) {
            Ok(summaries) => summaries,
            Err(error) => {
                tracing::warn!(%error, "could not query tracks for lyrics batch");
                self.set_progress(failed_progress(LyricsBatchProgress::idle()));
                return;
            }
        };
        self.cancellation.reset();
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let progress = LyricsBatchProgress::running(summaries.len());
        self.set_progress(progress);
        if progress.state == LyricsBatchState::Complete {
            return;
        }
        let (events, receiver) = async_channel::unbounded();
        if self
            .worker
            .sender
            .try_send(WorkerRequest {
                generation,
                generation_source: self.generation.clone(),
                cancellation: self.cancellation.clone(),
                enabled: self.enabled.clone(),
                tracks: summaries.into_iter().map(BatchTrack::from).collect(),
                events,
            })
            .is_err()
        {
            self.set_progress(failed_progress(progress));
            return;
        }
        let batch = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(event) = receiver.recv().await {
                let Some(batch) = batch.upgrade() else {
                    return;
                };
                if batch.generation.load(Ordering::Relaxed) != generation {
                    return;
                }
                match event {
                    WorkerEvent::Progress(progress) => batch.set_progress(progress),
                    WorkerEvent::Cancelled => {
                        batch.set_progress(LyricsBatchProgress::idle());
                        return;
                    }
                }
            }
        });
    }

    pub(in crate::ui) fn start_after_cover(self: &Rc<Self>, cover_batch: &Rc<CoverDownloadBatch>) {
        let lyrics_batch = self.clone();
        let armed = Rc::new(Cell::new(false));
        cover_batch.subscribe_progress(
            || true,
            start_after_cover_callback(armed.clone(), move || lyrics_batch.start()),
        );
        armed.set(true);
        cover_batch.start();
    }

    fn set_progress(&self, progress: LyricsBatchProgress) {
        self.progress.set(progress);
        self.subscribers.notify(progress);
    }

    #[cfg(test)]
    pub(in crate::ui) fn set_progress_for_test(&self, progress: LyricsBatchProgress) {
        self.set_progress(progress);
    }
}

pub(in crate::ui) fn start_after_cover_callback(
    armed: Rc<Cell<bool>>,
    start: impl Fn() + 'static,
) -> impl Fn(CoverBatchProgress) + 'static {
    move |progress| {
        if cover_batch_finished(armed.get(), progress.state) {
            start();
        }
    }
}

fn cover_batch_finished(armed: bool, state: CoverBatchState) -> bool {
    armed
        && matches!(
            state,
            CoverBatchState::Idle | CoverBatchState::Complete | CoverBatchState::Failed
        )
}

fn failed_progress(mut progress: LyricsBatchProgress) -> LyricsBatchProgress {
    progress.state = LyricsBatchState::Failed;
    progress
}

fn network_allowed(conn: &Db) -> bool {
    reprise_core::online_sources::network_allowed_or_off(
        conn,
        &reprise_core::modules::ONLINE_LYRICS_MODULE,
    )
}

fn cancelled(request: &WorkerRequest) -> bool {
    request.generation_source.load(Ordering::Relaxed) != request.generation
        || request.cancellation.is_requested()
}

#[cfg(test)]
#[path = "lyrics_batch_tests.rs"]
mod tests;
