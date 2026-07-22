//! Headless roundtrip tests for the instrumental worker's render logic, driven
//! through the pure `run_next_job`/`run_claimed_job` with the deterministic
//! `FakeStemBackend` — no threads, no sleeps, an injected fixed clock.

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;

use reprise_core::ai_jobs::{self, JobState};
use reprise_core::ai_promotion::PromotionConfig;
use reprise_core::ai_staging::StagingStore;
use reprise_core::stem_separation::{
    FakeStemBackend, StemError, StemSeparationBackend, PROGRESS_COMPLETE,
};
use rusqlite::Connection;

use super::*;
use crate::ui::instrumental::SourceResolver;

const WORKER: i64 = 7;
const NOW: i64 = 1_000;

fn clock() -> i64 {
    NOW
}

/// A migrated in-memory DB plus a source track whose file exists on disk, and a
/// staging store under the same temp dir. Returns everything the worker needs.
struct Harness {
    _dir: tempfile::TempDir,
    conn: Connection,
    staging: StagingStore,
    resolve: SourceResolver,
    model_id: String,
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.flac");
    std::fs::write(&source, b"fake source audio").unwrap();

    let conn = Connection::open_in_memory().unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, added_at) \
         VALUES (1, ?1, 'Song', 'Artist', 'Album', 0)",
        [source.to_string_lossy()],
    )
    .unwrap();

    let staging = StagingStore::new(dir.path().join("staging"));
    let resolve: SourceResolver = Arc::new(move |_conn: &Connection, id: i64| -> Option<PathBuf> {
        (id == 1).then(|| source.clone())
    });
    Harness {
        _dir: dir,
        conn,
        staging,
        resolve,
        model_id: FakeStemBackend::new().model_id(),
    }
}

fn enqueue(h: &Harness) -> i64 {
    ai_jobs::enqueue_instrumental(&h.conn, &h.staging, 1, &h.model_id, NOW)
        .unwrap()
        .job_id()
}

/// A backend that panics mid-render — a stand-in for a decoder crashing on a
/// crafted source (the reprise-stems `i % 0` class of bug).
struct PanickingBackend;

impl StemSeparationBackend for PanickingBackend {
    fn separate_instrumental(
        &self,
        _source: &std::path::Path,
        _output: &std::path::Path,
        _progress: &mut dyn FnMut(u16),
        _cancel: &dyn Fn() -> bool,
    ) -> Result<(), StemError> {
        panic!("simulated backend crash");
    }

    fn model_id(&self) -> String {
        FakeStemBackend::new().model_id()
    }
}

#[test]
fn worker_completes_a_queued_job_into_staging() {
    let mut h = harness();
    let job_id = enqueue(&h);
    let mut ticks = 0;

    let run = run_next_job(
        &mut h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &h.resolve,
        None,
        WORKER,
        LEASE_SECS,
        &clock,
        &mut || ticks += 1,
    )
    .expect("a queued job is claimed and run");

    assert_eq!(run.job_id, job_id);
    assert_eq!(run.outcome, JobRunOutcome::Completed);
    let job = ai_jobs::get_job(&h.conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, JobState::Done);
    assert_eq!(job.progress_permille, PROGRESS_COMPLETE);
    assert_eq!(job.result_track_id, None, "still staged, not yet promoted");
    assert!(h.staging.exists(job_id), "a completed run leaves a render");
    assert!(ticks >= 1, "a completed run ticks the UI at least once");
}

#[test]
fn worker_returns_none_when_the_queue_is_empty() {
    let mut h = harness();
    let run = run_next_job(
        &mut h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &h.resolve,
        None,
        WORKER,
        LEASE_SECS,
        &clock,
        &mut || {},
    );
    assert!(run.is_none(), "nothing runnable yields no run");
}

#[test]
fn worker_cancels_a_running_job_and_writes_no_output() {
    let mut h = harness();
    let job_id = enqueue(&h);
    // Claim it (queued -> running), then flag a cancel on the running job.
    let claimed = ai_jobs::claim_next(&h.conn, WORKER, NOW, LEASE_SECS)
        .unwrap()
        .expect("the queued job is claimable");
    assert!(matches!(
        ai_jobs::request_cancel(&h.conn, job_id, NOW).unwrap(),
        ai_jobs::CancelOutcome::CancelRequested
    ));

    let run = run_claimed_job(
        &mut h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &h.resolve,
        None,
        WORKER,
        &claimed,
        LEASE_SECS,
        &clock,
        &mut || {},
    );

    assert_eq!(run.outcome, JobRunOutcome::Cancelled);
    let job = ai_jobs::get_job(&h.conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, JobState::Cancelled);
    assert!(
        !h.staging.exists(job_id),
        "a cancelled run leaves no render"
    );
}

#[test]
fn worker_marks_a_backend_failure_without_output() {
    let mut h = harness();
    let job_id = enqueue(&h);

    let run = run_next_job(
        &mut h.conn,
        &FakeStemBackend::new().failing_at(1),
        &h.staging,
        &h.resolve,
        None,
        WORKER,
        LEASE_SECS,
        &clock,
        &mut || {},
    )
    .unwrap();

    assert_eq!(run.outcome, JobRunOutcome::Failed);
    let job = ai_jobs::get_job(&h.conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, JobState::Failed);
    assert!(
        job.error_kind.is_some(),
        "a failure records a diagnostic kind"
    );
    assert!(!h.staging.exists(job_id), "a failed run leaves no render");
}

#[test]
fn worker_fails_a_job_whose_source_cannot_be_resolved() {
    let mut h = harness();
    let job_id = enqueue(&h);
    // A resolver that never finds the path (the P3b gap in production).
    let resolve: SourceResolver = Arc::new(|_conn: &Connection, _id: i64| None);

    let run = run_next_job(
        &mut h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &resolve,
        None,
        WORKER,
        LEASE_SECS,
        &clock,
        &mut || {},
    )
    .unwrap();

    assert_eq!(run.outcome, JobRunOutcome::Failed);
    let job = ai_jobs::get_job(&h.conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, JobState::Failed);
    assert_eq!(job.error_kind.as_deref(), Some("source-unavailable"));
    assert!(!h.staging.exists(job_id));
}

#[test]
fn worker_progress_writes_are_throttled_but_completion_is_exact() {
    // The Fake fires ten rapid progress steps; the 250 ms floor collapses the
    // intermediate writes to at most one, yet mark_done still lands an exact
    // 1000. Driving run_claimed_job directly isolates the render's own ticks
    // (throttled progress + the final done) from run_next_job's start/terminal
    // ticks.
    let mut h = harness();
    let job_id = enqueue(&h);
    let claimed = ai_jobs::claim_next(&h.conn, WORKER, NOW, LEASE_SECS)
        .unwrap()
        .unwrap();
    let ticks = Cell::new(0u32);

    let mut on_tick = || ticks.set(ticks.get() + 1);
    run_claimed_job(
        &mut h.conn,
        &FakeStemBackend::new().with_steps(10),
        &h.staging,
        &h.resolve,
        None,
        WORKER,
        &claimed,
        LEASE_SECS,
        &clock,
        &mut on_tick,
    );

    // Ten rapid steps, at most one throttled progress write plus the done tick.
    assert!(
        ticks.get() <= 2,
        "progress writes are throttled: {}",
        ticks.get()
    );
    assert_eq!(
        ai_jobs::get_job(&h.conn, job_id)
            .unwrap()
            .unwrap()
            .progress_permille,
        PROGRESS_COMPLETE
    );
}

#[test]
fn worker_publishes_the_render_to_its_canonical_path_and_leaves_no_temp() {
    // A completed run must render into a claim-scoped temp and then publish it
    // to the canonical staging path, leaving no `.partial` temp behind.
    let mut h = harness();
    let job_id = enqueue(&h);

    let run = run_next_job(
        &mut h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &h.resolve,
        None,
        WORKER,
        LEASE_SECS,
        &clock,
        &mut || {},
    )
    .unwrap();

    assert_eq!(run.outcome, JobRunOutcome::Completed);
    assert!(
        h.staging.exists(job_id),
        "the render is published to its canonical staging path"
    );
    assert!(
        !h.staging.temp_path_for_job(job_id, WORKER).exists(),
        "the claim-scoped temp render is consumed by the publish"
    );
}

#[test]
fn worker_defers_a_failed_auto_promotion_and_keeps_the_render() {
    // With a library root configured and the job carrying save-intent, the
    // app-hosted worker promotes on completion (the smoke path). When the
    // promotion itself fails, it must degrade gracefully: the job stays done +
    // unsaved with its render kept in staging and the error noted, so a manual
    // save can retry — never a lost render or a stuck job.
    let mut h = harness();
    let job_id =
        ai_jobs::enqueue_instrumental_batch(&h.conn, &h.staging, &[1], &h.model_id, true, NOW)
            .unwrap()
            .jobs[0]
            .job_id();
    let library = tempfile::tempdir().unwrap();
    let config = PromotionConfig::new(library.path());
    // Block the promotion destination with a directory so the copy fails
    // deterministically (the harness track is "Song 1" by "Artist 1").
    let blocked = config
        .instrumentals_root()
        .join("Artist 1")
        .join("Song 1 (Instrumental).flac");
    std::fs::create_dir_all(&blocked).unwrap();

    let run = run_next_job(
        &mut h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &h.resolve,
        Some(&config),
        WORKER,
        LEASE_SECS,
        &clock,
        &mut || {},
    )
    .unwrap();

    assert_eq!(run.outcome, JobRunOutcome::Completed);
    let job = ai_jobs::get_job(&h.conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, JobState::Done);
    assert!(
        job.result_track_id.is_none(),
        "a failed auto-promotion leaves the job unsaved"
    );
    assert!(job.error_kind.is_some(), "the deferred promotion is noted");
    assert!(
        h.staging.exists(job_id),
        "the render is kept in staging for a manual retry"
    );
}

#[test]
fn worker_survives_a_panicking_backend_and_continues_with_the_next_job() {
    // A crafted source that makes the backend panic must not take the worker
    // thread down: the job fails cleanly (error_kind "backend"), and the worker
    // goes straight on to render the next job.
    let mut h = harness();
    let job1 = enqueue(&h);

    let run1 = run_next_job(
        &mut h.conn,
        &PanickingBackend,
        &h.staging,
        &h.resolve,
        None,
        WORKER,
        LEASE_SECS,
        &clock,
        &mut || {},
    )
    .unwrap();

    assert_eq!(run1.outcome, JobRunOutcome::Failed);
    let failed = ai_jobs::get_job(&h.conn, job1).unwrap().unwrap();
    assert_eq!(failed.state, JobState::Failed);
    assert_eq!(failed.error_kind.as_deref(), Some("backend"));
    assert!(
        !h.staging.exists(job1),
        "a panicked render leaves no output"
    );

    // The worker is unharmed: the next job renders to completion.
    let job2 = enqueue(&h);
    let run2 = run_next_job(
        &mut h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &h.resolve,
        None,
        WORKER,
        LEASE_SECS,
        &clock,
        &mut || {},
    )
    .unwrap();

    assert_eq!(run2.outcome, JobRunOutcome::Completed);
    assert!(h.staging.exists(job2));
}
