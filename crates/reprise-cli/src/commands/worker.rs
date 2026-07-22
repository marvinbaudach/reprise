//! `jobs work` — the standalone worker host (cargo feature `worker`).
//!
//! Claims queued AI jobs through the package-D `ai_jobs` facade
//! (lease + heartbeat + reclaim, all atomic in core), runs the
//! `StemSeparationBackend` one job at a time, writes each render into the
//! staging store, and completes the job through
//! [`ai_promotion::complete_render`]: the owner-guarded `mark_done` plus, when a
//! library root is configured, honoring the job's persisted save-intent by
//! promoting the fresh render into the library (Beschluss 15) — so a `--save`
//! create needs no manual save step. With no library root nothing can be filed,
//! so renders are simply left staged. Cancellation is honored between chunks;
//! SIGINT/SIGTERM stop the loop cleanly, leaving any in-flight job for another
//! worker to reclaim after its lease expires.
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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use clap::Args;
use reprise_core::ai_jobs::{self, ClaimedJob};
use reprise_core::ai_promotion::{self, CompletionOutcome, PromotionConfig};
use reprise_core::ai_staging::StagingStore;
use reprise_core::library::settings;
use reprise_core::queries;
use reprise_core::stem_separation::{
    FakeStemBackend, ProgressPermille, StemError, StemSeparationBackend, PROGRESS_COMPLETE,
};
use rusqlite::Connection;
use serde_json::json;

use crate::clock::now_unix;
use crate::commands::instrumental::map_promotion_error;
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
    /// `--once`). Must be at least 1 to avoid a busy loop.
    #[arg(long, value_name = "SECS", default_value_t = 2, value_parser = clap::value_parser!(u64).range(1..=86_400))]
    pub poll_interval: u64,
    /// Lease length for a claimed job, in seconds. A crashed worker's job is
    /// reclaimable by another worker after this elapses. Must be at least 1 — a
    /// zero/negative lease would make every claim instantly reclaimable and
    /// defeat the leasing model.
    #[arg(long, value_name = "SECS", default_value_t = 60, value_parser = clap::value_parser!(i64).range(1..=86_400))]
    pub lease: i64,
    /// Use the in-core Fake backend instead of the real (not-yet-wired)
    /// reprise-stems backend. Test/diagnostic aid.
    #[arg(long, hide = true)]
    pub fake_backend: bool,
    /// Simulate this many milliseconds of render time per job before the fake
    /// backend runs, making concurrent contention and mid-render reclaim
    /// deterministic. Test/diagnostic aid; only meaningful with
    /// `--fake-backend`.
    #[arg(long, hide = true, value_name = "MS", default_value_t = 0, value_parser = clap::value_parser!(u64).range(0..=600_000))]
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

/// The per-run invariants every claimed job shares: where renders are staged,
/// where (if anywhere) promotions are filed, the backend, this worker's token,
/// the CLI args, and the shutdown flag. Bundled so the claim/process functions
/// keep a small signature.
struct WorkerCtx<'a> {
    store: &'a StagingStore,
    config: Option<&'a PromotionConfig>,
    backend: &'a dyn StemSeparationBackend,
    worker: i64,
    args: &'a WorkerArgs,
    shutdown: &'a AtomicBool,
}

/// Runs the worker host. `conn` is this process's own connection (WAL lets many
/// coexist); the backend is chosen once at startup. A configured library root
/// lets the worker honor a job's persisted save-intent by promoting the render
/// on completion; with no root, renders are left staged (nothing to file).
pub fn run(
    conn: &mut Connection,
    staging_dir: Option<&PathBuf>,
    args: &WorkerArgs,
    json_output: bool,
) -> Result<(), CliError> {
    let backend = select_backend(args)?;
    let store = staging::resolve(staging_dir);
    store
        .ensure_dir()
        .map_err(|error| CliError::Database(format!("cannot create staging dir: {error}")))?;
    sweep_staging_orphans(&store, conn);
    let worker = worker_token();
    let shutdown = install_signal_flag()?;
    let config = library_promotion_config(conn)?;

    let ctx = WorkerCtx {
        store: &store,
        config: config.as_ref(),
        backend: backend.as_ref(),
        worker,
        args,
        shutdown: &shutdown,
    };

    // Run the loop with its own tally, then ALWAYS report — even when a mid-run
    // infrastructure error aborts the loop, so a `--json` caller still gets a
    // summary of the work already done and automation never sees empty output.
    let mut tally = Tally::default();
    let outcome = work_loop(conn, &ctx, &mut tally);
    report(tally, json_output, outcome.is_err());
    outcome
}

/// Builds the promotion target from the configured library root, or `None` when
/// none is set — then the worker leaves every finished render staged, since
/// there is nowhere to file a promotion regardless of a job's save-intent.
fn library_promotion_config(conn: &Connection) -> Result<Option<PromotionConfig>, CliError> {
    Ok(settings::get_library_root(conn)?.map(PromotionConfig::new))
}

/// Removes resurrectable staging orphans (a saved/cancelled/vanished job's
/// leftover render) before the worker starts. Best-effort: a sweep failure is a
/// non-fatal housekeeping miss reported on stderr, never a reason to refuse to
/// work. Diagnostics go to stderr so `--json` stdout stays a clean summary.
fn sweep_staging_orphans(store: &StagingStore, conn: &Connection) {
    match store.sweep_orphans(conn) {
        Ok(removed) if !removed.is_empty() => {
            eprintln!("worker: swept {} staging orphan(s)", removed.len());
        }
        Ok(_) => {}
        Err(error) => eprintln!("worker: staging orphan sweep failed: {error}"),
    }
}

/// The claim/process loop. Returns the first infrastructure error that aborts
/// it (individual failed renders are recorded on their jobs, not returned).
fn work_loop(
    conn: &mut Connection,
    ctx: &WorkerCtx<'_>,
    tally: &mut Tally,
) -> Result<(), CliError> {
    loop {
        if ctx.shutdown.load(Ordering::Relaxed) {
            return Ok(());
        }
        // The claim only needs `&Connection`; scope an immutable reborrow so
        // `process_job` can take `conn` mutably for the promotion path.
        let claimed = {
            let conn: &Connection = conn;
            retrying(|| ai_jobs::claim_next(conn, ctx.worker, now_unix(), ctx.args.lease))?
        };
        match claimed {
            Some(job) => {
                tally.record(process_job(conn, ctx, &job)?);
                if ctx.args.max_jobs != 0 && tally.total() >= ctx.args.max_jobs {
                    return Ok(());
                }
            }
            None => {
                if ctx.args.once || ctx.shutdown.load(Ordering::Relaxed) {
                    return Ok(());
                }
                // Interruptible so SIGINT is honored within a slice, not after a
                // whole poll interval.
                sleep_unless_shutdown(Duration::from_secs(ctx.args.poll_interval), ctx.shutdown);
            }
        }
    }
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
        // Skip the progress write once we already know to stop (lost lease,
        // cancel, or a heartbeat failure) — it would only waste a retry cycle
        // against an already-known-bad state.
        if self.stop.get().is_some() {
            return;
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
/// `ai_jobs`/`ai_promotion` facades; a genuine infrastructure failure is
/// returned as a [`CliError`] (stopping the worker), while a failed render is
/// recorded on the job and reported as [`JobOutcome::Failed`].
fn process_job(
    conn: &mut Connection,
    ctx: &WorkerCtx<'_>,
    job: &ClaimedJob,
) -> Result<JobOutcome, CliError> {
    let worker = ctx.worker;
    let Some(source_path) = resolve_source(conn, job)? else {
        retrying(|| {
            ai_jobs::mark_failed(conn, job.id, worker, "source_track_missing", now_unix())
        })?;
        return Ok(JobOutcome::Failed);
    };
    // Render into a claim-scoped temp file, never the shared canonical path:
    // the lease protects the DB row, but two workers can legitimately hold the
    // *same* `path_for_job(id)` (a straggler whose lease expired mid-render and
    // the reclaimer that finished it). `complete_render_with_publish` renames
    // temp -> canonical only after an owner-guarded `mark_done`, making the
    // winning owner the sole writer of the canonical file, so a straggler can
    // never clobber (or resurrect) the committed render.
    let temp_path = ctx.store.temp_path_for_job(job.id, worker);

    // Optional simulated occupancy (test aid): hold the claim without
    // heartbeating, so a short lease can expire and be reclaimed elsewhere.
    if let Some(reason) = simulate_occupancy(ctx.args, ctx.shutdown) {
        return abandon_or_cancel(conn, job.id, worker, reason);
    }

    // Render inside a scope so the immutable `&Connection` the heartbeat/progress
    // sink borrows is released before the completion below takes `conn` mutably
    // (promotion needs `&mut Connection`).
    let (result, infra_error, stop) = {
        let state = RunState {
            conn,
            job_id: job.id,
            worker,
            lease: ctx.args.lease,
            shutdown: ctx.shutdown,
            stop: Cell::new(None),
            last_write: Cell::new(None),
            infra_error: Cell::new(None),
        };
        let result = separate_catching_panics(
            ctx.backend,
            &source_path,
            &temp_path,
            &mut |permille| state.on_progress(permille),
            &|| state.should_stop(),
        );
        (result, state.infra_error.take(), state.stop.get())
    };

    if let Some(error) = infra_error {
        let _ = std::fs::remove_file(&temp_path);
        return Err(CliError::Database(error));
    }
    match result {
        Ok(()) => complete_owned_render(conn, ctx, job, &temp_path, stop),
        Err(StemError::Cancelled) => {
            let _ = std::fs::remove_file(&temp_path);
            let reason = stop.unwrap_or(StopReason::Shutdown);
            abandon_or_cancel(conn, job.id, worker, reason)
        }
        Err(other) => {
            // A backend error leaves no complete output, but drop any partial.
            let _ = std::fs::remove_file(&temp_path);
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

/// Completes an owned render through the core publish-safe path. The
/// owner-guarded `mark_done` inside [`ai_promotion::complete_render_with_publish`]
/// runs first, and only the winner then renames the claim-scoped temp onto the
/// canonical staging path and honors the job's persisted save-intent (promoting
/// into the library when a root is configured, else leaving it staged). A
/// straggler that lost its lease fails the guard, never touches the canonical
/// file, and has its temp deleted — no clobber, no resurrected orphan.
fn complete_owned_render(
    conn: &mut Connection,
    ctx: &WorkerCtx<'_>,
    job: &ClaimedJob,
    temp_path: &Path,
    stop: Option<StopReason>,
) -> Result<JobOutcome, CliError> {
    // If the final heartbeat already told us the lease was lost (or we are
    // shutting down), this job is no longer ours — drop the temp and let the
    // owner finish it. The core guard would catch it too; this just skips a
    // futile DB round-trip.
    if stop.is_some() {
        let _ = std::fs::remove_file(temp_path);
        return Ok(JobOutcome::Abandoned);
    }
    let outcome = ai_promotion::complete_render_with_publish(
        conn,
        ctx.store,
        ctx.config,
        job.id,
        ctx.worker,
        temp_path,
        now_unix(),
    )
    .map_err(|error| map_promotion_error(error, job.id))?;
    Ok(match outcome {
        // The lease lapsed before our guarded mark_done; the reclaiming worker
        // owns the DB row and the temp was dropped — report abandoned.
        CompletionOutcome::NotOwned => JobOutcome::Abandoned,
        CompletionOutcome::Staged
        | CompletionOutcome::Promoted(_)
        | CompletionOutcome::PromotionDeferred { .. } => JobOutcome::Done,
    })
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
/// the job has no source or the track row is gone. Uses the focused
/// [`queries::track_source_path`] facade — the same by-id path lookup the
/// app-hosted worker resolves through.
fn resolve_source(conn: &Connection, job: &ClaimedJob) -> Result<Option<PathBuf>, CliError> {
    let Some(source_track_id) = job.source_track_id else {
        return Ok(None);
    };
    Ok(queries::track_source_path(conn, source_track_id)?)
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
    // `checked_add` avoids the documented `Instant + Duration` overflow panic
    // for a pathologically large duration (arg ranges already prevent it; this
    // is defensive). A `None` deadline just keeps slicing until shutdown.
    let deadline = Instant::now().checked_add(total);
    loop {
        if shutdown.load(Ordering::Relaxed) {
            return true;
        }
        let remaining = match deadline {
            Some(deadline) => deadline.saturating_duration_since(Instant::now()),
            None => SLEEP_SLICE,
        };
        if remaining.is_zero() {
            return false;
        }
        sleep(remaining.min(SLEEP_SLICE));
    }
}

/// Runs the backend, catching a panic and mapping it to a normal backend
/// failure. A crafted or corrupt source can make a decoder panic (e.g. dividing
/// by a zero channel count); catching it here keeps one poisoned file from
/// taking the whole worker process down mid-queue — the job fails with
/// `error_kind` "backend" and the loop continues with the next job.
fn separate_catching_panics(
    backend: &dyn StemSeparationBackend,
    source: &Path,
    output: &Path,
    progress: &mut dyn FnMut(ProgressPermille),
    cancel: &dyn Fn() -> bool,
) -> Result<(), StemError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        backend.separate_instrumental(source, output, progress, cancel)
    }))
    .unwrap_or_else(|_| {
        Err(StemError::Backend(
            "backend panicked during separation".to_string(),
        ))
    })
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

/// Emits the run summary. `aborted` is true when an infrastructure error cut
/// the loop short, so automation can tell a clean stop from a partial run even
/// though the error text itself goes to stderr with a non-zero exit.
fn report(tally: Tally, json_output: bool, aborted: bool) {
    if json_output {
        print_json(&json!({
            "processed": tally.total(),
            "done": tally.done,
            "failed": tally.failed,
            "cancelled": tally.cancelled,
            "abandoned": tally.abandoned,
            "aborted": aborted,
        }));
    } else {
        let status = if aborted {
            "worker aborted"
        } else {
            "worker stopped"
        };
        println!(
            "{status}: {} processed (done {}, failed {}, cancelled {}, abandoned {})",
            tally.total(),
            tally.done,
            tally.failed,
            tally.cancelled,
            tally.abandoned,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A backend that panics mid-render — a stand-in for a decoder crashing on a
    /// crafted source (the reprise-stems `i % 0` class of bug).
    struct PanickingBackend;

    impl StemSeparationBackend for PanickingBackend {
        fn separate_instrumental(
            &self,
            _source: &Path,
            _output: &Path,
            _progress: &mut dyn FnMut(ProgressPermille),
            _cancel: &dyn Fn() -> bool,
        ) -> Result<(), StemError> {
            panic!("simulated backend crash");
        }

        fn model_id(&self) -> String {
            "panic@0".to_string()
        }
    }

    #[test]
    fn a_backend_panic_becomes_a_backend_failure_not_a_process_abort() {
        let result = separate_catching_panics(
            &PanickingBackend,
            Path::new("/nonexistent/source.flac"),
            Path::new("/nonexistent/out.flac"),
            &mut |_| {},
            &|| false,
        );
        let error = result.expect_err("a panicking backend yields an error, not a panic");
        assert!(matches!(error, StemError::Backend(_)));
        // Recorded on the job through the normal per-job failure path.
        assert_eq!(error_kind(&error), "backend");
    }
}
