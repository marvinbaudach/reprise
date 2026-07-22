//! Headless roundtrip tests for the instrumental worker's render logic, driven
//! through the pure `run_next_job`/`run_claimed_job` with the deterministic
//! `FakeStemBackend` — no threads, no sleeps, an injected fixed clock.

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;

use reprise_core::ai_jobs::{self, JobState};
use reprise_core::ai_staging::StagingStore;
use reprise_core::stem_separation::{FakeStemBackend, PROGRESS_COMPLETE};
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

#[test]
fn worker_completes_a_queued_job_into_staging() {
    let h = harness();
    let job_id = enqueue(&h);
    let mut ticks = 0;

    let run = run_next_job(
        &h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &h.resolve,
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
    let h = harness();
    let run = run_next_job(
        &h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &h.resolve,
        WORKER,
        LEASE_SECS,
        &clock,
        &mut || {},
    );
    assert!(run.is_none(), "nothing runnable yields no run");
}

#[test]
fn worker_cancels_a_running_job_and_writes_no_output() {
    let h = harness();
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
        &h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &h.resolve,
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
    let h = harness();
    let job_id = enqueue(&h);

    let run = run_next_job(
        &h.conn,
        &FakeStemBackend::new().failing_at(1),
        &h.staging,
        &h.resolve,
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
    let h = harness();
    let job_id = enqueue(&h);
    // A resolver that never finds the path (the P3b gap in production).
    let resolve: SourceResolver = Arc::new(|_conn: &Connection, _id: i64| None);

    let run = run_next_job(
        &h.conn,
        &FakeStemBackend::new(),
        &h.staging,
        &resolve,
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
    let h = harness();
    let job_id = enqueue(&h);
    let claimed = ai_jobs::claim_next(&h.conn, WORKER, NOW, LEASE_SECS)
        .unwrap()
        .unwrap();
    let ticks = Cell::new(0u32);

    let mut on_tick = || ticks.set(ticks.get() + 1);
    run_claimed_job(
        &h.conn,
        &FakeStemBackend::new().with_steps(10),
        &h.staging,
        &h.resolve,
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
