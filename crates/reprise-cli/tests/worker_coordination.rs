//! The critical worker-coordination proofs (plan risk list): two concurrent
//! workers never double-process a job, and a crashed worker's job is reclaimed
//! after its lease expires. Only compiled/run with `--features worker`.
#![cfg(feature = "worker")]

mod common;

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use common::{code, parse_json, Harness};

fn staging_dir(h: &Harness) -> PathBuf {
    h.dir.path().join("staging")
}

/// Seeds `n` tracks (with real files) and enqueues one staged job each.
fn enqueue_n(h: &Harness, staging: &Path, n: i64) -> Vec<i64> {
    for id in 1..=n {
        h.seed_track_with_file(id);
    }
    let ids: Vec<String> = (1..=n).map(|id| id.to_string()).collect();
    let mut args = vec![
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "create",
    ];
    for id in &ids {
        args.push(id);
    }
    args.push("--stage");
    let created = parse_json(&h.run(&args));
    created["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["job_id"].as_i64().unwrap())
        .collect()
}

/// THE flagship test: two `jobs work` processes against one temp DB must never
/// double-process a job. Atomic claims (core's conditional UPDATE inside a
/// transaction) guarantee exactly one claimer per job; the small simulated
/// render time forces genuine overlap so the workers really do contend.
#[test]
fn two_workers_never_double_process_a_job() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let job_ids = enqueue_n(&h, &staging, 8);

    // Run two independent worker processes concurrently, each draining once.
    let worker_args = [
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "work",
        "--once",
        "--fake-backend",
        "--simulate-render-ms",
        "60",
        "--json",
    ];
    let (out_a, out_b) = thread::scope(|scope| {
        let a = scope.spawn(|| h.run(&worker_args));
        let b = scope.spawn(|| h.run(&worker_args));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_eq!(code(&out_a), 0);
    assert_eq!(code(&out_b), 0);

    // 1. Between them the two workers processed every job exactly once — the sum
    //    of their own tallies is the job count, with no overlap.
    let processed_a = parse_json(&out_a)["processed"].as_i64().unwrap();
    let processed_b = parse_json(&out_b)["processed"].as_i64().unwrap();
    assert_eq!(
        processed_a + processed_b,
        8,
        "each job processed exactly once across both workers (a={processed_a}, b={processed_b})"
    );

    // 2. The change log shows exactly one `start` per job id — the end-to-end
    //    proof that no job was claimed twice.
    let mut starts: Vec<String> = h
        .ai_job_events()
        .into_iter()
        .filter(|(_, op)| op == "start")
        .map(|(job_id, _)| job_id)
        .collect();
    starts.sort();
    let mut expected: Vec<String> = job_ids.iter().map(i64::to_string).collect();
    expected.sort();
    assert_eq!(starts, expected, "each job started exactly once");

    // 3. Every job is done, with exactly one render on disk each.
    let store = reprise_core::ai_staging::StagingStore::new(&staging);
    for id in &job_ids {
        assert_eq!(h.ai_job_status(*id).as_deref(), Some("done"));
        assert!(store.exists(*id), "job {id} has exactly one render");
    }
}

/// A crashed worker leaves its job `running` with a lease that eventually
/// expires. This seeds exactly that leftover state (via the core facade with a
/// short, already-expired lease) and proves the real CLI worker reclaims and
/// finishes it — deterministic, no sleeps.
#[test]
fn a_stale_leased_job_is_reclaimed_and_finished() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let job_ids = enqueue_n(&h, &staging, 1);
    let job_id = job_ids[0];

    // Simulate a worker (token 999) that claimed the job at t=0 with a 1-second
    // lease, then died — the job is left `running` with a long-expired lease.
    {
        let conn = h.conn();
        let claimed = reprise_core::ai_jobs::claim_next(&conn, 999, 0, 1)
            .unwrap()
            .expect("seeded worker claims the job");
        assert_eq!(claimed.id, job_id);
        assert_eq!(h.ai_job_status(job_id).as_deref(), Some("running"));
    }

    // The real worker, running now, sees the expired lease and reclaims it.
    let out = h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "work",
        "--once",
        "--fake-backend",
    ]);
    assert_eq!(code(&out), 0);
    assert_eq!(parse_json(&out)["done"], 1, "the reclaimed job finished");
    assert_eq!(h.ai_job_status(job_id).as_deref(), Some("done"));
    let store = reprise_core::ai_staging::StagingStore::new(&staging);
    assert!(
        store.exists(job_id),
        "the reclaimed job produced its render"
    );
}

/// End-to-end kill: a real worker process claims a job and is SIGKILLed
/// mid-render (its lease deliberately short). After the lease expires a second
/// worker reclaims and finishes the job — no job is lost to a crash.
#[test]
fn a_killed_worker_is_reclaimed_by_another() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let job_ids = enqueue_n(&h, &staging, 1);
    let job_id = job_ids[0];

    // Worker A: a very long simulated render with a 1-second lease, so once we
    // kill it the lease expires almost immediately.
    let mut worker_a = h.spawn(&[
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "work",
        "--once",
        "--fake-backend",
        "--simulate-render-ms",
        "60000",
        "--lease",
        "1",
    ]);

    // Wait until A has actually claimed the job (poll its status).
    let claimed = wait_until(Duration::from_secs(10), || {
        h.ai_job_status(job_id).as_deref() == Some("running")
    });
    assert!(claimed, "worker A must claim the job before we kill it");

    // Kill A mid-render and reap it; its job stays `running` with a stale lease.
    worker_a.kill().expect("kill worker A");
    worker_a.wait().expect("reap worker A");

    // Poll worker B until the 1-second lease has expired and B reclaims the job.
    // Each B run is a cheap no-op until the lease lapses, so this is robust to
    // scheduling jitter without depending on a single fixed sleep.
    let reclaimed = wait_until(Duration::from_secs(10), || {
        h.run(&[
            "--staging-dir",
            staging.to_str().unwrap(),
            "jobs",
            "work",
            "--once",
            "--fake-backend",
        ]);
        h.ai_job_status(job_id).as_deref() == Some("done")
    });
    assert!(
        reclaimed,
        "worker B must reclaim and finish the crashed job"
    );
    let store = reprise_core::ai_staging::StagingStore::new(&staging);
    assert!(
        store.exists(job_id),
        "the reclaimed job produced its render"
    );
}

/// The worker acks a cancel between chunks. Seeds a running job with a cancel
/// already requested (and a stale lease so the CLI worker reclaims it), then
/// proves the worker stops and marks it cancelled with no render left behind.
#[test]
fn the_worker_acks_a_requested_cancel() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let job_ids = enqueue_n(&h, &staging, 1);
    let job_id = job_ids[0];
    {
        let conn = h.conn();
        // Claim (expired lease) then request cancel: job is running + flagged.
        reprise_core::ai_jobs::claim_next(&conn, 999, 0, 0).unwrap();
        let outcome = reprise_core::ai_jobs::request_cancel(&conn, job_id, 0).unwrap();
        assert_eq!(
            outcome,
            reprise_core::ai_jobs::CancelOutcome::CancelRequested
        );
    }

    let out = h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "work",
        "--once",
        "--fake-backend",
    ]);
    assert_eq!(code(&out), 0);
    assert_eq!(
        parse_json(&out)["cancelled"],
        1,
        "the worker acked the cancel"
    );
    assert_eq!(h.ai_job_status(job_id).as_deref(), Some("cancelled"));
    let store = reprise_core::ai_staging::StagingStore::new(&staging);
    assert!(
        !store.exists(job_id),
        "a cancelled render leaves nothing behind"
    );
}

/// Polls `condition` until true or the timeout elapses; returns whether it
/// became true.
fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(25));
    }
    condition()
}
