//! The app-hosted instrumental worker (plan §2.4/2) — one job at a time, its
//! own connection, the claim/lease/heartbeat/progress/finish protocol driven
//! through the `ai_jobs` core facade, and a `StemSeparationBackend` (the Fake
//! in this package; the real backend in P3b).
//!
//! The spike's ~6 GB memory peak forces **exactly one job at a time**, so this
//! is a single worker thread, not a pool — the same shape as
//! `ui::scan::audio_analysis_runtime`. The render *logic* is factored into the
//! pure, synchronous [`run_next_job`]/[`run_claimed_job`] so a headless test
//! drives a full claim→progress→done / cancel / fail roundtrip with an injected
//! clock and no threads or sleeps; [`InstrumentalWorker`] only wraps that in the
//! thread + condvar + coalesced progress channel.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use reprise_core::ai_jobs::{self, ClaimedJob, HeartbeatOutcome};
use reprise_core::ai_staging::StagingStore;
use reprise_core::stem_separation::{StemError, StemSeparationBackend};
use rusqlite::Connection;

use super::SourceResolver;

/// Lease length for a claimed job. Generous: a real render can run minutes on
/// slow hardware (plan §2.4/6), so the lease must outlast the gap between two
/// heartbeats by a wide margin. A worker heartbeats between chunks, well within
/// this, and a crashed worker's job becomes reclaimable once it elapses.
pub(in crate::ui) const LEASE_SECS: i64 = 120;

/// Floor between two `progress_permille` writes (plan §2.2: ≤ 2 writes/s). A
/// real backend reports at chunk boundaries; this keeps a chatty one from
/// hammering the row. Monotonic so it is immune to wall-clock jumps.
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(250);

const WORKER_THREAD_NAME: &str = "reprise-instrumental-worker";

/// How a single job's run ended — the terminal DB state is the truth, this is
/// the worker's view of what it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum JobRunOutcome {
    /// Rendered and marked `done` (staging render written).
    Completed,
    /// A requested cancel was acked (`cancelled`); no output left behind.
    Cancelled,
    /// The backend failed (`failed`); no output.
    Failed,
    /// The lease was lost mid-run (another worker owns it now) — left as-is for
    /// that worker, nothing was marked.
    Abandoned,
}

/// The result of running one job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct JobRun {
    pub job_id: i64,
    pub outcome: JobRunOutcome,
}

/// A diagnostic `error_kind` for a failed render — stored on the job row, not
/// user-facing.
fn error_kind(error: &StemError) -> &'static str {
    match error {
        StemError::Cancelled => "cancelled",
        StemError::SourceUnreadable(_) => "source-unreadable",
        StemError::Io(_) => "io",
        StemError::Backend(_) => "backend",
    }
}

/// Claims the next runnable job for `worker_id` and runs it to completion,
/// returning `None` when the queue holds nothing runnable. Pure and synchronous
/// — the thread loop calls it repeatedly; tests call it directly.
#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn run_next_job(
    conn: &Connection,
    backend: &dyn StemSeparationBackend,
    staging: &StagingStore,
    resolve: &SourceResolver,
    worker_id: i64,
    lease_secs: i64,
    clock: &dyn Fn() -> i64,
    on_progress: &mut dyn FnMut(),
) -> Option<JobRun> {
    let claimed = match ai_jobs::claim_next(conn, worker_id, clock(), lease_secs) {
        Ok(claimed) => claimed?,
        Err(error) => {
            tracing::error!(%error, "instrumental worker: claim failed");
            return None;
        }
    };
    Some(run_claimed_job(
        conn,
        backend,
        staging,
        resolve,
        worker_id,
        &claimed,
        lease_secs,
        clock,
        on_progress,
    ))
}

/// Runs one already-claimed (`running`) job: resolves the source, renders into
/// the staging store, and marks the terminal state through `ai_jobs`. Split out
/// so a test can claim a job, request its cancel, then run it — proving the
/// running→cancelled path deterministically.
#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn run_claimed_job(
    conn: &Connection,
    backend: &dyn StemSeparationBackend,
    staging: &StagingStore,
    resolve: &SourceResolver,
    worker_id: i64,
    job: &ClaimedJob,
    lease_secs: i64,
    clock: &dyn Fn() -> i64,
    on_progress: &mut dyn FnMut(),
) -> JobRun {
    let job_id = job.id;
    let fail = |kind: &str| {
        if let Err(error) = ai_jobs::mark_failed(conn, job_id, worker_id, kind, clock()) {
            tracing::error!(%error, job_id, "instrumental worker: mark_failed failed");
        }
        JobRun {
            job_id,
            outcome: JobRunOutcome::Failed,
        }
    };

    let Some(source) = job.source_track_id.and_then(|id| resolve(conn, id)) else {
        tracing::warn!(job_id, "instrumental worker: source path unavailable");
        return fail("source-unavailable");
    };
    if let Err(error) = staging.ensure_dir() {
        tracing::error!(%error, job_id, "instrumental worker: could not create staging dir");
        return fail("io");
    }
    let output = staging.path_for_job(job_id);

    // The progress/cancel closures borrow `conn` and `on_progress`; scoping
    // them to this block releases those borrows before the terminal arms below
    // tick the UI one final time.
    let result = {
        // Progress: throttled DB write + a coalesced UI tick (plan §2.2).
        let last_write = std::cell::Cell::new(Option::<Instant>::None);
        let mut progress = |permille: u16| {
            let now = Instant::now();
            let due = last_write
                .get()
                .is_none_or(|last| now.duration_since(last) >= PROGRESS_MIN_INTERVAL);
            if !due {
                return;
            }
            last_write.set(Some(now));
            if let Err(error) = ai_jobs::set_progress(conn, job_id, worker_id, permille) {
                tracing::warn!(%error, job_id, "instrumental worker: set_progress failed");
            }
            on_progress();
        };
        // Cancel probe: heartbeat between chunks refreshes the lease and reports
        // a pending cancel; losing the lease also stops the run.
        let cancel = || {
            let outcome = ai_jobs::heartbeat(conn, job_id, worker_id, clock(), lease_secs)
                .unwrap_or(HeartbeatOutcome {
                    still_owner: false,
                    cancel_requested: false,
                });
            outcome.cancel_requested || !outcome.still_owner
        };
        backend.separate_instrumental(&source, &output, &mut progress, &cancel)
    };
    match result {
        Ok(()) => {
            match ai_jobs::mark_done(conn, job_id, worker_id, clock()) {
                Ok(true) => on_progress(),
                Ok(false) => tracing::warn!(job_id, "instrumental worker: done lost ownership"),
                Err(error) => tracing::error!(%error, job_id, "instrumental worker: mark_done"),
            }
            JobRun {
                job_id,
                outcome: JobRunOutcome::Completed,
            }
        }
        Err(StemError::Cancelled) => {
            // A cancelled run left no output. mark_cancelled is gated on a real
            // pending cancel; if it did not apply, the lease was lost instead —
            // leave the row for the reclaiming worker.
            match ai_jobs::mark_cancelled(conn, job_id, worker_id, clock()) {
                Ok(true) => {
                    on_progress();
                    JobRun {
                        job_id,
                        outcome: JobRunOutcome::Cancelled,
                    }
                }
                Ok(false) => JobRun {
                    job_id,
                    outcome: JobRunOutcome::Abandoned,
                },
                Err(error) => {
                    tracing::error!(%error, job_id, "instrumental worker: mark_cancelled");
                    JobRun {
                        job_id,
                        outcome: JobRunOutcome::Abandoned,
                    }
                }
            }
        }
        Err(error) => {
            // A partial render is never valid output — drop it.
            let _ = staging.discard(job_id);
            fail(error_kind(&error))
        }
    }
}

// --- Threaded runtime -------------------------------------------------------

struct LoopState {
    revision: u64,
    shutdown: bool,
}

struct SharedState {
    state: Mutex<LoopState>,
    changed: Condvar,
    progress_tx: async_channel::Sender<()>,
    stopping: AtomicBool,
}

struct Inner {
    shared: Arc<SharedState>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        shutdown(&self.shared, &self.worker);
    }
}

/// The window-owned handle to the instrumental worker thread. Cheap to clone;
/// dropping the last clone joins the thread.
#[derive(Clone)]
pub(in crate::ui) struct InstrumentalWorker {
    inner: Arc<Inner>,
    progress_rx: async_channel::Receiver<()>,
}

impl InstrumentalWorker {
    /// Spawns the worker thread. It opens its **own** migrated connection to
    /// `db_path` (never the UI connection — `rusqlite::Connection` is not
    /// `Send`), then idles on a condvar until [`wake`](Self::wake) or shutdown.
    pub(in crate::ui) fn new(
        db_path: PathBuf,
        backend: Box<dyn StemSeparationBackend + Send>,
        staging: StagingStore,
        resolve: SourceResolver,
        worker_id: i64,
    ) -> Self {
        let (progress_tx, progress_rx) = async_channel::bounded::<()>(1);
        let shared = Arc::new(SharedState {
            state: Mutex::new(LoopState {
                revision: 0,
                shutdown: false,
            }),
            changed: Condvar::new(),
            progress_tx,
            stopping: AtomicBool::new(false),
        });
        let worker = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name(WORKER_THREAD_NAME.into())
                .spawn(move || {
                    // The thread owns the backend for its whole life; worker_loop
                    // only ever borrows it.
                    worker_loop(
                        &db_path,
                        backend.as_ref(),
                        &staging,
                        &resolve,
                        worker_id,
                        &shared,
                    );
                })
                .ok()
        };
        if worker.is_none() {
            tracing::error!("instrumental worker: could not spawn worker thread");
        }
        Self {
            inner: Arc::new(Inner {
                shared,
                worker: Mutex::new(worker),
            }),
            progress_rx,
        }
    }

    /// Nudges the worker to re-poll the queue — call after enqueuing jobs so a
    /// newly queued render starts without waiting for the next event.
    // Consumed by the "Create instrumental" enqueue path in a later package-F commit.
    #[allow(dead_code)]
    pub(in crate::ui) fn wake(&self) {
        let mut state = self.inner.shared.state.lock().unwrap();
        state.revision = state.revision.wrapping_add(1);
        self.inner.shared.changed.notify_all();
    }

    /// A coalesced "refresh" stream: one tick per progress/lifecycle write
    /// (bounded(1), so a slow UI collapses a burst into one). The conversion
    /// view drains it via `glib::spawn_future_local` and re-reads the job rows
    /// — progress is not a change_log event, so this is how the bar stays live.
    pub(in crate::ui) fn progress_receiver(&self) -> async_channel::Receiver<()> {
        self.progress_rx.clone()
    }

    /// Stops the worker thread and joins it. Idempotent; also runs on drop and
    /// should be called from `window.connect_close_request`.
    pub(in crate::ui) fn shutdown(&self) {
        shutdown(&self.inner.shared, &self.inner.worker);
    }
}

fn shutdown(shared: &Arc<SharedState>, worker: &Mutex<Option<JoinHandle<()>>>) {
    shared.stopping.store(true, Ordering::SeqCst);
    {
        let mut state = shared.state.lock().unwrap();
        state.shutdown = true;
        shared.changed.notify_all();
    }
    if let Some(handle) = worker.lock().unwrap().take() {
        if handle.join().is_err() {
            tracing::error!("instrumental worker: worker thread panicked");
        }
    }
}

fn worker_loop(
    db_path: &std::path::Path,
    backend: &dyn StemSeparationBackend,
    staging: &StagingStore,
    resolve: &SourceResolver,
    worker_id: i64,
    shared: &Arc<SharedState>,
) {
    let conn = match reprise_core::db::open_migrated(Some(db_path)) {
        Ok(conn) => conn,
        Err(error) => {
            tracing::error!(%error, "instrumental worker: could not open database");
            return;
        }
    };
    let clock = super::now_unix;
    let mut handled = 0u64;
    loop {
        // Drain every runnable job, one at a time.
        loop {
            if shared.stopping.load(Ordering::SeqCst) {
                return;
            }
            let mut tick = || {
                let _ = shared.progress_tx.try_send(());
            };
            match run_next_job(
                &conn, backend, staging, resolve, worker_id, LEASE_SECS, &clock, &mut tick,
            ) {
                Some(_) => continue,
                None => break,
            }
        }
        // Idle until woken (new work) or told to stop.
        let mut state = shared.state.lock().unwrap();
        while state.revision == handled && !state.shutdown {
            state = shared.changed.wait(state).unwrap();
        }
        if state.shutdown {
            return;
        }
        handled = state.revision;
    }
}

#[cfg(test)]
#[path = "worker_host_tests.rs"]
mod tests;
