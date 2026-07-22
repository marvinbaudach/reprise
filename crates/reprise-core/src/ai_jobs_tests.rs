use super::*;
use crate::ai_staging::StagingStore;

const LEASE_SECS: i64 = 60;

fn migrated() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

fn seed_track(conn: &Connection, id: i64) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
         VALUES (?1, ?2, 'T', 'A', 1, 1, 1)",
        params![id, format!("/music/{id}.flac")],
    )
    .unwrap();
}

/// A staging store over a temp dir, with a render file present for `job_id`.
fn staging_with_render(dir: &std::path::Path, job_id: i64) -> StagingStore {
    let store = StagingStore::new(dir);
    store.ensure_dir().unwrap();
    std::fs::write(store.path_for_job(job_id), b"render").unwrap();
    store
}

fn job_ops(conn: &Connection) -> Vec<(String, String)> {
    events::read_since(conn, 0, None)
        .unwrap()
        .into_iter()
        .filter(|change| change.entity == JOB_ENTITY)
        .map(|change| (change.entity_id, change.operation))
        .collect()
}

#[test]
fn enqueue_creates_a_queued_job_and_logs_one_event() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);

    let outcome = enqueue_instrumental(&conn, &empty, 1, "m@1", 100).unwrap();

    let job_id = match outcome {
        EnqueueOutcome::Created { job_id } => job_id,
        other => panic!("expected Created, got {other:?}"),
    };
    let job = get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, JobState::Queued);
    assert_eq!(job.source_track_id, Some(1));
    assert_eq!(job.params_fingerprint, "m@1");
    assert_eq!(job.created_at, 100);
    assert_eq!(
        job_ops(&conn),
        [(job_id.to_string(), "enqueue".to_string())]
    );
}

#[test]
fn re_enqueuing_an_open_job_dedups_without_a_second_row_or_event() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    let first = enqueue_instrumental(&conn, &empty, 1, "m@1", 100).unwrap();

    let second = enqueue_instrumental(&conn, &empty, 1, "m@1", 200).unwrap();

    assert_eq!(
        second,
        EnqueueOutcome::Deduplicated {
            job_id: first.job_id(),
            result_track_id: None,
        }
    );
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    // Only the first enqueue logged an event.
    assert_eq!(job_ops(&conn).len(), 1);
}

#[test]
fn a_different_model_is_a_distinct_job() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    enqueue_instrumental(&conn, &empty, 1, "m@1", 100).unwrap();

    let other = enqueue_instrumental(&conn, &empty, 1, "m@2", 100).unwrap();

    assert!(matches!(other, EnqueueOutcome::Created { .. }));
}

#[test]
fn dedup_references_a_saved_result() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    seed_track(&conn, 2);
    let job_id = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();
    // Drive it to a saved state.
    conn.execute(
        "UPDATE ai_jobs SET status = 'done', result_track_id = 2 WHERE id = ?1",
        [job_id],
    )
    .unwrap();

    let again = enqueue_instrumental(&conn, &empty, 1, "m@1", 10).unwrap();
    assert_eq!(
        again,
        EnqueueOutcome::Deduplicated {
            job_id,
            result_track_id: Some(2),
        }
    );
}

#[test]
fn dedup_references_a_staged_render_only_while_its_file_exists() {
    let dir = tempfile::tempdir().unwrap();
    let conn = migrated();
    seed_track(&conn, 1);
    let empty = StagingStore::new(dir.path());
    let job_id = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();
    // Finish it into staging (done, no result yet) and drop the render on disk.
    conn.execute("UPDATE ai_jobs SET status = 'done' WHERE id = ?1", [job_id])
        .unwrap();
    let staging = staging_with_render(dir.path(), job_id);

    // Render present -> dedup to the staged job.
    assert_eq!(
        enqueue_instrumental(&conn, &staging, 1, "m@1", 10).unwrap(),
        EnqueueOutcome::Deduplicated {
            job_id,
            result_track_id: None,
        }
    );

    // Render gone (discarded / promoted-then-deleted) -> the work is free again.
    staging.discard(job_id).unwrap();
    assert!(matches!(
        enqueue_instrumental(&conn, &staging, 1, "m@1", 20).unwrap(),
        EnqueueOutcome::Created { .. }
    ));
}

#[test]
fn batch_enqueue_shares_a_batch_id_and_aggregates_progress() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    for id in [1, 2, 3] {
        seed_track(&conn, id);
    }

    let batch = enqueue_instrumental_batch(&conn, &empty, &[1, 2, 3], "m@1", 0).unwrap();
    assert_eq!(batch.jobs.len(), 3);
    assert!(batch
        .jobs
        .iter()
        .all(|j| matches!(j, EnqueueOutcome::Created { .. })));

    // Move one job to done, one to half progress; the aggregate reflects both.
    let ids: Vec<i64> = batch.jobs.iter().map(|j| j.job_id()).collect();
    conn.execute(
        "UPDATE ai_jobs SET status = 'done', progress_permille = 1000 WHERE id = ?1",
        [ids[0]],
    )
    .unwrap();
    conn.execute(
        "UPDATE ai_jobs SET status = 'running', progress_permille = 500 WHERE id = ?1",
        [ids[1]],
    )
    .unwrap();

    let progress = batch_progress(&conn, &batch.batch_id).unwrap();
    assert_eq!(progress.total, 3);
    assert_eq!(progress.done, 1);
    assert_eq!(progress.running, 1);
    assert_eq!(progress.queued, 1);
    // (1000 + 500 + 0) / 3 = 500.
    assert_eq!(progress.permille, 500);
    assert_eq!(list_jobs_in_batch(&conn, &batch.batch_id).unwrap().len(), 3);
}

#[test]
fn claim_marks_running_sets_the_lease_and_logs_start() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    let job_id = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();

    let claimed = claim_next(&conn, 77, 1_000, LEASE_SECS).unwrap().unwrap();

    assert_eq!(claimed.id, job_id);
    assert_eq!(claimed.lease_expires_at, 1_060);
    let job = get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, JobState::Running);
    assert!(job_ops(&conn).contains(&(job_id.to_string(), "start".to_string())));
    // Nothing else to claim.
    assert!(claim_next(&conn, 77, 1_000, LEASE_SECS).unwrap().is_none());
}

#[test]
fn a_valid_lease_blocks_a_second_worker_until_it_expires() {
    // Two connections over one file DB: the exactly-one-claimer guarantee.
    let file = tempfile::NamedTempFile::new().unwrap();
    let a = crate::db::open(Some(file.path())).unwrap();
    crate::db::migrate(&a).unwrap();
    let b = crate::db::open(Some(file.path())).unwrap();
    let empty = StagingStore::new("/unused");
    seed_track(&a, 1);
    let job_id = enqueue_instrumental(&a, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();

    // Worker A claims with a lease to now+60.
    let claimed_a = claim_next(&a, 1, 100, LEASE_SECS).unwrap();
    assert_eq!(claimed_a.map(|j| j.id), Some(job_id));

    // Worker B, before the lease expires, gets nothing.
    assert!(claim_next(&b, 2, 120, LEASE_SECS).unwrap().is_none());

    // After the lease expires, B reclaims the same job.
    let reclaimed = claim_next(&b, 2, 1_000, LEASE_SECS).unwrap();
    assert_eq!(reclaimed.map(|j| j.id), Some(job_id));
    let claimed_by: i64 = a
        .query_row(
            "SELECT claimed_by FROM ai_jobs WHERE id = ?1",
            [job_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(claimed_by, 2, "the reclaiming worker now owns the job");
}

#[test]
fn heartbeat_extends_the_lease_for_the_owner_and_reports_cancel() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    let job_id = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();
    claim_next(&conn, 5, 100, LEASE_SECS).unwrap();

    let beat = heartbeat(&conn, job_id, 5, 200, LEASE_SECS).unwrap();
    assert_eq!(
        beat,
        HeartbeatOutcome {
            still_owner: true,
            cancel_requested: false
        }
    );
    let lease: i64 = conn
        .query_row(
            "SELECT lease_expires_at FROM ai_jobs WHERE id = ?1",
            [job_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(lease, 260);

    // A non-owner is told it lost the job.
    let stranger = heartbeat(&conn, job_id, 999, 300, LEASE_SECS).unwrap();
    assert!(!stranger.still_owner);

    // A cancel request surfaces through the owner's next heartbeat.
    request_cancel(&conn, job_id, 250).unwrap();
    assert!(
        heartbeat(&conn, job_id, 5, 300, LEASE_SECS)
            .unwrap()
            .cancel_requested
    );
}

#[test]
fn set_progress_is_owner_guarded_and_records_no_event() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    let job_id = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();
    claim_next(&conn, 5, 0, LEASE_SECS).unwrap();
    let events_before = job_ops(&conn).len();

    assert!(set_progress(&conn, job_id, 5, 400).unwrap());
    assert!(
        !set_progress(&conn, job_id, 999, 900).unwrap(),
        "non-owner rejected"
    );

    assert_eq!(
        get_job(&conn, job_id).unwrap().unwrap().progress_permille,
        400
    );
    assert_eq!(
        job_ops(&conn).len(),
        events_before,
        "progress is not a lifecycle event"
    );
}

#[test]
fn mark_done_completes_the_job_and_logs_done() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    let job_id = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();
    claim_next(&conn, 5, 0, LEASE_SECS).unwrap();

    assert!(mark_done(&conn, job_id, 5, 500).unwrap());
    let job = get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, JobState::Done);
    assert_eq!(job.progress_permille, 1000);
    assert_eq!(
        job.result_track_id, None,
        "done means staged, not yet saved"
    );
    assert_eq!(job.finished_at, Some(500));
    assert!(job_ops(&conn).contains(&(job_id.to_string(), "done".to_string())));
    // A non-owner cannot complete it.
    assert!(!mark_done(&conn, job_id, 999, 600).unwrap());
}

#[test]
fn mark_failed_records_the_error_kind() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    let job_id = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();
    claim_next(&conn, 5, 0, LEASE_SECS).unwrap();

    assert!(mark_failed(&conn, job_id, 5, "backend", 500).unwrap());
    let job = get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, JobState::Failed);
    assert_eq!(job.error_kind.as_deref(), Some("backend"));
    assert!(job_ops(&conn).contains(&(job_id.to_string(), "fail".to_string())));
}

#[test]
fn cancelling_a_queued_job_is_immediate() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    let job_id = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();

    assert_eq!(
        request_cancel(&conn, job_id, 500).unwrap(),
        CancelOutcome::CancelledImmediately
    );
    let job = get_job(&conn, job_id).unwrap().unwrap();
    assert_eq!(job.state, JobState::Cancelled);
    assert!(job_ops(&conn).contains(&(job_id.to_string(), "cancel".to_string())));
    // A cancelled job is no longer claimable.
    assert!(claim_next(&conn, 5, 600, LEASE_SECS).unwrap().is_none());
}

#[test]
fn cancelling_a_running_job_flags_it_then_the_worker_acks() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    let job_id = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();
    claim_next(&conn, 5, 0, LEASE_SECS).unwrap();

    assert_eq!(
        request_cancel(&conn, job_id, 100).unwrap(),
        CancelOutcome::CancelRequested
    );
    assert_eq!(
        get_job(&conn, job_id).unwrap().unwrap().state,
        JobState::Running
    );
    // Only the owner, and only with a pending request, can ack.
    assert!(!mark_cancelled(&conn, job_id, 999, 200).unwrap());
    assert!(mark_cancelled(&conn, job_id, 5, 200).unwrap());
    assert_eq!(
        get_job(&conn, job_id).unwrap().unwrap().state,
        JobState::Cancelled
    );
}

#[test]
fn discard_staged_cancels_the_job_deletes_the_file_and_frees_dedup() {
    let dir = tempfile::tempdir().unwrap();
    let conn = migrated();
    seed_track(&conn, 1);
    let staging = StagingStore::new(dir.path());
    let job_id = enqueue_instrumental(&conn, &staging, 1, "m@1", 0)
        .unwrap()
        .job_id();
    conn.execute("UPDATE ai_jobs SET status = 'done' WHERE id = ?1", [job_id])
        .unwrap();
    staging.ensure_dir().unwrap();
    std::fs::write(staging.path_for_job(job_id), b"render").unwrap();

    assert!(discard_staged(&conn, &staging, job_id, 900).unwrap());
    assert_eq!(
        get_job(&conn, job_id).unwrap().unwrap().state,
        JobState::Cancelled
    );
    assert!(!staging.exists(job_id), "discard deletes the render file");
    // The work can be enqueued fresh again.
    assert!(matches!(
        enqueue_instrumental(&conn, &staging, 1, "m@1", 1000).unwrap(),
        EnqueueOutcome::Created { .. }
    ));
}

#[test]
fn attach_result_track_moves_a_done_job_to_saved() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    seed_track(&conn, 2);
    let job_id = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();
    conn.execute("UPDATE ai_jobs SET status = 'done' WHERE id = ?1", [job_id])
        .unwrap();

    assert!(attach_result_track(&conn, job_id, 2).unwrap());
    assert_eq!(
        get_job(&conn, job_id).unwrap().unwrap().result_track_id,
        Some(2)
    );
    assert!(job_ops(&conn).contains(&(job_id.to_string(), "save".to_string())));
}

#[test]
fn list_active_excludes_cancelled_jobs() {
    let conn = migrated();
    let empty = StagingStore::new("/unused");
    seed_track(&conn, 1);
    seed_track(&conn, 2);
    let keep = enqueue_instrumental(&conn, &empty, 1, "m@1", 0)
        .unwrap()
        .job_id();
    let drop = enqueue_instrumental(&conn, &empty, 2, "m@1", 0)
        .unwrap()
        .job_id();
    request_cancel(&conn, drop, 10).unwrap();

    let active: Vec<i64> = list_active_jobs(&conn)
        .unwrap()
        .into_iter()
        .map(|j| j.id)
        .collect();
    assert_eq!(active, [keep]);
}

#[test]
fn job_state_round_trips() {
    for state in [
        JobState::Queued,
        JobState::Running,
        JobState::Done,
        JobState::Failed,
        JobState::Cancelled,
    ] {
        assert_eq!(JobState::parse(state.as_str()), Some(state));
    }
    assert_eq!(JobState::parse("garbage"), None);
}
