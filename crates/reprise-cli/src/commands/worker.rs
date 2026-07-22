//! `jobs work` — the standalone worker host (cargo feature `worker`).
//!
//! Claims queued AI jobs through the package-D `ai_jobs` facade
//! (lease + heartbeat + reclaim, all atomic in core), runs the
//! `StemSeparationBackend` one job at a time, writes each render into the
//! staging store, and marks the job `done` (staged, awaiting the save
//! decision). Cancellation is honored between chunks; SIGINT/SIGTERM stop the
//! loop cleanly, leaving any in-flight job for another worker to reclaim after
//! its lease expires.
//!
//! This module is the ONLY thing that pulls `reprise-stems` into the CLI (the
//! removable ML backend), so the default build stays core-only. The real
//! backend is constructed at startup here; tests drive the in-core
//! [`FakeStemBackend`] via `--fake-backend`.

// The removable ML backend crate. Package G will replace `select_backend`'s
// "not wired yet" arm with a real `reprise_stems::…` construction; referencing
// it here keeps the (feature-gated) dependency edge explicit and mechanical.
use reprise_stems as _;

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use clap::Args;
use reprise_core::ai_jobs::{self, ClaimedJob};
use reprise_core::ai_staging::StagingStore;
use reprise_core::queries;
use reprise_core::stem_separation::{
    FakeStemBackend, ProgressPermille, StemError, StemSeparationBackend, PROGRESS_COMPLETE,
};
use rusqlite::Connection;
use serde_json::json;

use crate::clock::now_unix;
use crate::error::CliError;
use crate::output::print_json;
use crate::retry::{rusqlite_is_busy, with_retry};
use crate::staging;

/// Runs a mutating `ai_jobs` facade call under the shared busy-retry policy.
/// Concurrent workers contend on the single WAL writer slot and can even see a
/// `SQLITE_BUSY_SNAPSHOT` when a peer commits between a claim's read and write;
/// retrying with a fresh transaction (and a fresh `now`) is exactly the right
/// response, so no legitimate claim/heartbeat/transition is lost to contention.
fn retrying<T>(op: impl FnMut() -> Result<T, rusqlite::Error>) -> Result<T, CliError> {
    with_retry(op, rusqlite_is_busy).map_err(CliError::from)
}

/// Minimum spacing between in-place progress writes (plan 2.2: ≤ 2 writes/s).
const PROGRESS_WRITE_INTERVAL: Duration = Duration::from_millis(500);
/// Slice length for interruptible sleeps (empty-queue poll, simulated render),
/// so SIGINT/SIGTERM is honored within a slice rather than after a whole poll
/// interval.
const SLEEP_SLICE: Duration = Duration::from_millis(50);

/// Arguments for `jobs work`.
#[derive(Args, Debug)]
pub struct WorkerArgs {
    /// Process every runnable job then exit, instead of polling for more.
    #[arg(long)]
    pub once: bool,
    /// Stop after processing this many jobs (0 = unlimited).
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub max_jobs: u64,
    /// Seconds to sleep between polls when the queue is empty (ignored with
    /// `--once`).
    #[arg(long, value_name = "SECS", default_value_t = 2)]
    pub poll_interval: u64,
    /// Lease length for a claimed job, in seconds. A crashed worker's job is
    /// reclaimable by another worker after this elapses.
    #[arg(long, value_name = "SECS", default_value_t = 60)]
    pub lease: i64,
    /// Use the in-core Fake backend instead of the real (not-yet-wired)
    /// reprise-stems backend. Test/diagnostic aid.
    #[arg(long, hide = true)]
    pub fake_backend: bool,
    /// Simulate this many milliseconds of render time per job before the fake
    /// backend runs, making concurrent contention and mid-render reclaim
    /// deterministic. Test/diagnostic aid; only meaningful with
    /// `--fake-backend`.
    #[arg(long, hide = true, value_name = "MS", default_value_t = 0)]
    pub simulate_render_ms: u64,
}

/// Why a job's render stopped early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopReason {
    /// A user cancel request was observed (ack it: job -> cancelled).
    Cancel,
    /// The process is shutting down (leave the job running for reclaim).
    Shutdown,
    /// The lease was lost to another worker (abandon; the new owner has it).
    LostLease,
}

/// What one claimed job resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobOutcome {
    Done,
    Failed,
    Cancelled,
    /// Left running (shutdown or lost lease) — reclaimable by another worker.
    Abandoned,
}

/// Running tally of processed jobs, reported when the loop exits.
#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    done: u64,
    failed: u64,
    cancelled: u64,
    abandoned: u64,
}

impl Tally {
    fn record(&mut self, outcome: JobOutcome) {
        match outcome {
            JobOutcome::Done => self.done += 1,
            JobOutcome::Failed => self.failed += 1,
            JobOutcome::Cancelled => self.cancelled += 1,
            JobOutcome::Abandoned => self.abandoned += 1,
        }
    }

    fn total(self) -> u64 {
        self.done + self.failed + self.cancelled + self.abandoned
    }
}

/// Runs the worker host. `conn` is this process's own connection (WAL lets many
/// coexist); the backend is chosen once at startup.
pub fn run(
    conn: &Connection,
    staging_dir: Option<&PathBuf>,
    args: &WorkerArgs,
    json_output: bool,
) -> Result<(), CliError> {
    let backend = select_backend(args)?;
    let store = staging::resolve(staging_dir);
    store
        .ensure_dir()
        .map_err(|error| CliError::Database(format!("cannot create staging dir: {error}")))?;
    let worker = worker_token();
    let shutdown = install_signal_flag()?;

    let mut tally = Tally::default();
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        let claimed = retrying(|| ai_jobs::claim_next(conn, worker, now_unix(), args.lease))?;
        match claimed {
            Some(job) => {
                let outcome = process_job(
                    conn,
                    &store,
                    backend.as_ref(),
                    &job,
                    worker,
                    args,
                    &shutdown,
                )?;
                tally.record(outcome);
                if args.max_jobs != 0 && tally.total() >= args.max_jobs {
                    break;
                }
            }
            None => {
                if args.once || shutdown.load(Ordering::Relaxed) {
                    break;
                }
                // Interruptible so SIGINT is honored within a slice, not after a
                // whole poll interval.
                sleep_unless_shutdown(Duration::from_secs(args.poll_interval), &shutdown);
            }
        }
    }

    report(tally, json_output);
    Ok(())
}

/// Chooses the stem-separation backend. Production would build the real
/// reprise-stems backend here; it is still a stub (package G), so the honest
/// answer without `--fake-backend` is a clear "not wired yet" — never a silent
/// fake render in production.
fn select_backend(args: &WorkerArgs) -> Result<Box<dyn StemSeparationBackend + Send>, CliError> {
    if args.fake_backend {
        return Ok(Box::new(FakeStemBackend::new()));
    }
    Err(CliError::Unavailable(
        "no stem-separation backend is built in yet — package G (reprise-stems) is still a stub. \
         Re-run with --fake-backend to exercise the worker, or wait for the real backend."
            .to_string(),
    ))
}

/// Claims-scoped mutable state shared between the progress sink and the cancel
/// probe (interior mutability so both closures can borrow it at once).
struct RunState<'a> {
    conn: &'a Connection,
    job_id: i64,
    worker: i64,
    lease: i64,
    shutdown: &'a AtomicBool,
    stop: Cell<Option<StopReason>>,
    last_write: Cell<Option<Instant>>,
    infra_error: Cell<Option<String>>,
}

impl RunState<'_> {
    /// Called after each rendered chunk: heartbeat (extend lease, read cancel),
    /// then write throttled progress.
    fn on_progress(&self, permille: ProgressPermille) {
        let beat = with_retry(
            || ai_jobs::heartbeat(self.conn, self.job_id, self.worker, now_unix(), self.lease),
            rusqlite_is_busy,
        );
        match beat {
            Ok(outcome) => {
                if !outcome.still_owner {
                    self.stop.set(Some(StopReason::LostLease));
                } else if outcome.cancel_requested {
                    self.stop.set(Some(StopReason::Cancel));
                }
            }
            Err(error) => {
                self.infra_error.set(Some(error.to_string()));
                self.stop.set(Some(StopReason::LostLease));
            }
        }
        if self.should_write_progress(permille) {
            let written = with_retry(
                || ai_jobs::set_progress(self.conn, self.job_id, self.worker, permille),
                rusqlite_is_busy,
            );
            if let Err(error) = written {
                self.infra_error.set(Some(error.to_string()));
            }
            self.last_write.set(Some(Instant::now()));
        }
    }

    /// Rate-limits progress writes, but always lets the final 100% through.
    fn should_write_progress(&self, permille: ProgressPermille) -> bool {
        if permille >= PROGRESS_COMPLETE {
            return true;
        }
        match self.last_write.get() {
            Some(at) => at.elapsed() >= PROGRESS_WRITE_INTERVAL,
            None => true,
        }
    }

    /// Cancel probe: shut down, cancel requested, or lease lost.
    fn should_stop(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed) || self.stop.get().is_some()
    }
}

/// Renders one claimed job. Terminal DB transitions go through the owner-guarded
/// `ai_jobs` facades; a genuine infrastructure failure is returned as a
/// [`CliError`] (stopping the worker), while a failed render is recorded on the
/// job and reported as [`JobOutcome::Failed`].
fn process_job(
    conn: &Connection,
    store: &StagingStore,
    backend: &dyn StemSeparationBackend,
    job: &ClaimedJob,
    worker: i64,
    args: &WorkerArgs,
    shutdown: &AtomicBool,
) -> Result<JobOutcome, CliError> {
    let Some(source_path) = resolve_source(conn, job)? else {
        retrying(|| {
            ai_jobs::mark_failed(conn, job.id, worker, "source_track_missing", now_unix())
        })?;
        return Ok(JobOutcome::Failed);
    };
    let output_path = store.path_for_job(job.id);

    // Optional simulated occupancy (test aid): hold the claim without
    // heartbeating, so a short lease can expire and be reclaimed elsewhere.
    if let Some(reason) = simulate_occupancy(args, shutdown) {
        return abandon_or_cancel(conn, job.id, worker, reason);
    }

    let state = RunState {
        conn,
        job_id: job.id,
        worker,
        lease: args.lease,
        shutdown,
        stop: Cell::new(None),
        last_write: Cell::new(None),
        infra_error: Cell::new(None),
    };
    let result = backend.separate_instrumental(
        &source_path,
        &output_path,
        &mut |permille| state.on_progress(permille),
        &|| state.should_stop(),
    );

    if let Some(error) = state.infra_error.take() {
        return Err(CliError::Database(error));
    }
    match result {
        Ok(()) => {
            // The owner-guarded facade returns `false` if we lost the lease
            // mid-render (another worker reclaimed): then this is not our job to
            // count as done — report it abandoned instead of double-counting.
            let marked = retrying(|| ai_jobs::mark_done(conn, job.id, worker, now_unix()))?;
            Ok(if marked {
                JobOutcome::Done
            } else {
                JobOutcome::Abandoned
            })
        }
        Err(StemError::Cancelled) => {
            let reason = state.stop.get().unwrap_or(StopReason::Shutdown);
            abandon_or_cancel(conn, job.id, worker, reason)
        }
        Err(other) => {
            let kind = error_kind(&other);
            let marked = retrying(|| ai_jobs::mark_failed(conn, job.id, worker, kind, now_unix()))?;
            Ok(if marked {
                JobOutcome::Failed
            } else {
                JobOutcome::Abandoned
            })
        }
    }
}

/// A user cancel is acked (`-> cancelled`); a shutdown or lost lease leaves the
/// job `running` for another worker to reclaim after the lease expires.
fn abandon_or_cancel(
    conn: &Connection,
    job_id: i64,
    worker: i64,
    reason: StopReason,
) -> Result<JobOutcome, CliError> {
    match reason {
        StopReason::Cancel => {
            let marked = retrying(|| ai_jobs::mark_cancelled(conn, job_id, worker, now_unix()))?;
            Ok(if marked {
                JobOutcome::Cancelled
            } else {
                JobOutcome::Abandoned
            })
        }
        StopReason::Shutdown | StopReason::LostLease => Ok(JobOutcome::Abandoned),
    }
}

/// Resolves the claimed job's source track to an on-disk path, or `None` when
/// the job has no source or the track row is gone.
fn resolve_source(conn: &Connection, job: &ClaimedJob) -> Result<Option<PathBuf>, CliError> {
    let Some(source_track_id) = job.source_track_id else {
        return Ok(None);
    };
    let summary = queries::query_track_summary(conn, source_track_id)?;
    Ok(summary.map(|summary| PathBuf::from(summary.path)))
}

/// Sleeps the configured simulated-render time in short slices, honoring
/// shutdown. Returns `Some(Shutdown)` if interrupted, else `None`.
fn simulate_occupancy(args: &WorkerArgs, shutdown: &AtomicBool) -> Option<StopReason> {
    if args.simulate_render_ms == 0 {
        return None;
    }
    if sleep_unless_shutdown(Duration::from_millis(args.simulate_render_ms), shutdown) {
        Some(StopReason::Shutdown)
    } else {
        None
    }
}

/// Sleeps up to `total` in [`SLEEP_SLICE`] steps, returning early the moment the
/// shutdown flag is set. Returns whether a shutdown cut the sleep short — so a
/// blocked worker reacts to SIGINT/SIGTERM within a slice, not after the whole
/// duration (`std::thread::sleep` is not interrupted by a signal).
fn sleep_unless_shutdown(total: Duration, shutdown: &AtomicBool) -> bool {
    let deadline = Instant::now() + total;
    loop {
        if shutdown.load(Ordering::Relaxed) {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        sleep(remaining.min(SLEEP_SLICE));
    }
}

/// The diagnostic `error_kind` recorded for a failed render.
fn error_kind(error: &StemError) -> &'static str {
    match error {
        StemError::Cancelled => "cancelled",
        StemError::SourceUnreadable(_) => "source_unreadable",
        StemError::Io(_) => "io",
        StemError::Backend(_) => "backend",
    }
}

/// A random 64-bit worker token (the `claimed_by` value), so two worker
/// processes never share an identity. `fastrand` is already in the tree via
/// core; a fresh `SystemTime`-seeded value is plenty for a per-process id.
fn worker_token() -> i64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    // Mix the pid in so two workers started in the same nanosecond still differ.
    let pid = std::process::id();
    i64::from(pid).wrapping_mul(1_000_000_007) ^ i64::from(nanos)
}

/// Installs a SIGINT/SIGTERM flag for clean shutdown. Failure to install is a
/// hard error rather than a silently un-interruptible worker.
fn install_signal_flag() -> Result<Arc<AtomicBool>, CliError> {
    let flag = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(signal, Arc::clone(&flag)).map_err(|error| {
            CliError::Database(format!("cannot install signal handler: {error}"))
        })?;
    }
    Ok(flag)
}

fn report(tally: Tally, json_output: bool) {
    if json_output {
        print_json(&json!({
            "processed": tally.total(),
            "done": tally.done,
            "failed": tally.failed,
            "cancelled": tally.cancelled,
            "abandoned": tally.abandoned,
        }));
    } else {
        println!(
            "worker stopped: {} processed (done {}, failed {}, cancelled {}, abandoned {})",
            tally.total(),
            tally.done,
            tally.failed,
            tally.cancelled,
            tally.abandoned,
        );
    }
}
