//! Serial, cancellable library-wide lyrics cache population.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use gtk4::glib;
use reprise_core::db::Db;
use reprise_core::lyrics::{LookupOptions, LyricsError, LyricsHit, LyricsQuery, NeedsFetch};
use reprise_core::queries::TrackSummary;

use super::cover_download_batch::{BatchState as CoverBatchState, CoverDownloadBatch};
use super::scan_flow::ScanCancellation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum LyricsBatchState {
    Idle,
    Running,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct LyricsBatchProgress {
    pub(in crate::ui) state: LyricsBatchState,
    pub(in crate::ui) checked: usize,
    pub(in crate::ui) total: usize,
    pub(in crate::ui) downloaded: usize,
    pub(in crate::ui) unavailable: usize,
}

impl LyricsBatchProgress {
    fn idle() -> Self {
        Self {
            state: LyricsBatchState::Idle,
            checked: 0,
            total: 0,
            downloaded: 0,
            unavailable: 0,
        }
    }

    fn running(total: usize) -> Self {
        Self {
            state: if total == 0 {
                LyricsBatchState::Complete
            } else {
                LyricsBatchState::Running
            },
            total,
            ..Self::idle()
        }
    }

    fn advance(mut self, outcome: BatchItemOutcome) -> Self {
        self.checked = self.checked.saturating_add(1).min(self.total);
        match outcome {
            BatchItemOutcome::Skipped | BatchItemOutcome::Failed => {}
            BatchItemOutcome::Downloaded => self.downloaded += 1,
            BatchItemOutcome::Unavailable => self.unavailable += 1,
        }
        if self.checked == self.total {
            self.state = LyricsBatchState::Complete;
        }
        self
    }

    fn fail(mut self) -> Self {
        self.state = LyricsBatchState::Failed;
        self
    }

    pub(in crate::ui) fn fraction(self) -> f64 {
        if self.total == 0 {
            return f64::from(self.state == LyricsBatchState::Complete);
        }
        self.checked as f64 / self.total as f64
    }
}

#[derive(Clone, Copy)]
enum BatchItemOutcome {
    Skipped,
    Downloaded,
    Unavailable,
    Failed,
}

#[derive(Clone)]
struct BatchTrack {
    query: LyricsQuery,
    path: PathBuf,
}

impl From<TrackSummary> for BatchTrack {
    fn from(summary: TrackSummary) -> Self {
        Self {
            query: LyricsQuery {
                title: summary.title,
                artist: summary.artist,
                album: summary.album,
                duration_ms: summary.duration_ms,
            },
            path: summary.path.into(),
        }
    }
}

type LocalLookup = Arc<dyn Fn(&Path) -> bool + Send + Sync>;
type NeedsLookup = Arc<dyn Fn(&LyricsQuery) -> NeedsFetch + Send + Sync>;
type OnlineLookup =
    Arc<dyn Fn(&LyricsQuery, &Path) -> Result<LyricsHit, LyricsError> + Send + Sync>;
type AllBreakersOpen = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone)]
struct WorkerServices {
    local: LocalLookup,
    needs: NeedsLookup,
    online: OnlineLookup,
    all_breakers_open: AllBreakersOpen,
}

struct WorkerRequest {
    generation: u64,
    generation_source: Arc<AtomicU64>,
    cancellation: ScanCancellation,
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
        Self::spawn(WorkerServices {
            local: Arc::new(|path| reprise_core::lyrics::local_hit(path).is_some()),
            needs: Arc::new(reprise_core::lyrics::needs_fetch),
            online: Arc::new(|query, path| {
                reprise_core::lyrics::load_or_fetch_with_options(
                    query,
                    Some(path),
                    LookupOptions::default(),
                )
            }),
            all_breakers_open: Arc::new(reprise_core::lyrics::all_network_breakers_open),
        })
    }

    fn spawn(services: WorkerServices) -> Self {
        let (sender, receiver) = async_channel::unbounded();
        let result = std::thread::Builder::new()
            .name("reprise-lyrics-batch".into())
            .spawn(move || {
                while let Ok(request) = receiver.recv_blocking() {
                    run_request(&request, &services);
                }
            });
        if let Err(error) = result {
            tracing::warn!(%error, "could not start lyrics batch worker");
        }
        Self { sender }
    }
}

type ProgressCallback = Rc<dyn Fn(LyricsBatchProgress)>;

pub(in crate::ui) struct LyricsBatch {
    conn: Rc<Db>,
    worker: LyricsBatchWorker,
    cancellation: ScanCancellation,
    generation: Arc<AtomicU64>,
    progress: Cell<LyricsBatchProgress>,
    subscribers: RefCell<Vec<ProgressCallback>>,
}

impl LyricsBatch {
    pub(in crate::ui) fn new(conn: &Rc<Db>, cancellation: ScanCancellation) -> Rc<Self> {
        Rc::new(Self {
            conn: conn.clone(),
            worker: LyricsBatchWorker::production(),
            cancellation,
            generation: Arc::new(AtomicU64::new(0)),
            progress: Cell::new(LyricsBatchProgress::idle()),
            subscribers: RefCell::new(Vec::new()),
        })
    }

    pub(in crate::ui) fn subscribe_progress(
        &self,
        callback: impl Fn(LyricsBatchProgress) + 'static,
    ) {
        let callback: ProgressCallback = Rc::new(callback);
        callback(self.progress.get());
        self.subscribers.borrow_mut().push(callback);
    }

    pub(in crate::ui) fn start(self: &Rc<Self>) {
        if !reprise_core::online_sources::network_allowed_or_off(
            &self.conn,
            &reprise_core::modules::ONLINE_LYRICS_MODULE,
        ) {
            self.set_progress(LyricsBatchProgress::idle());
            return;
        }
        let summaries = match reprise_core::queries::query_live_track_summaries(&self.conn) {
            Ok(summaries) => summaries,
            Err(error) => {
                tracing::warn!(%error, "could not query tracks for lyrics batch");
                self.set_progress(LyricsBatchProgress::idle().fail());
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
                tracks: summaries.into_iter().map(BatchTrack::from).collect(),
                events,
            })
            .is_err()
        {
            self.set_progress(progress.fail());
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
        cover_batch.subscribe_progress(|| true, {
            let armed = armed.clone();
            move |progress| {
                if cover_batch_finished(armed.get(), progress.state) {
                    lyrics_batch.start();
                }
            }
        });
        armed.set(true);
        cover_batch.start();
    }

    fn set_progress(&self, progress: LyricsBatchProgress) {
        self.progress.set(progress);
        let subscribers = self.subscribers.borrow().clone();
        for callback in subscribers {
            callback(progress);
        }
    }

    #[cfg(test)]
    pub(in crate::ui) fn set_progress_for_test(&self, progress: LyricsBatchProgress) {
        self.set_progress(progress);
    }
}

fn cover_batch_finished(armed: bool, state: CoverBatchState) -> bool {
    armed
        && matches!(
            state,
            CoverBatchState::Idle | CoverBatchState::Complete | CoverBatchState::Failed
        )
}

fn run_request(request: &WorkerRequest, services: &WorkerServices) {
    let mut progress = LyricsBatchProgress::running(request.tracks.len());
    for track in &request.tracks {
        if cancelled(request) {
            let _ = request.events.send_blocking(WorkerEvent::Cancelled);
            return;
        }
        let outcome = if (services.local)(&track.path)
            || (services.needs)(&track.query) == NeedsFetch::Skip
        {
            BatchItemOutcome::Skipped
        } else if (services.all_breakers_open)() {
            let _ = request
                .events
                .send_blocking(WorkerEvent::Progress(progress.fail()));
            return;
        } else {
            match (services.online)(&track.query, &track.path) {
                Ok(_) => BatchItemOutcome::Downloaded,
                Err(LyricsError::NotFound | LyricsError::MissingMetadata) => {
                    BatchItemOutcome::Unavailable
                }
                Err(_) => BatchItemOutcome::Failed,
            }
        };
        progress = progress.advance(outcome);
        if matches!(outcome, BatchItemOutcome::Failed) && (services.all_breakers_open)() {
            progress = progress.fail();
        }
        let terminal = progress.state != LyricsBatchState::Running;
        if request
            .events
            .send_blocking(WorkerEvent::Progress(progress))
            .is_err()
            || terminal
        {
            return;
        }
    }
}

fn cancelled(request: &WorkerRequest) -> bool {
    request.generation_source.load(Ordering::Relaxed) != request.generation
        || request.cancellation.is_requested()
}

#[cfg(test)]
#[path = "lyrics_batch_tests.rs"]
mod tests;
