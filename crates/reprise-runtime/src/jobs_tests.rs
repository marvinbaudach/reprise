//! Job translation and the one job command the runtime serves today.

use std::path::Path;

use reprise_core::ai_jobs;
use reprise_core::ai_staging::StagingStore;
use reprise_runtime_protocol::jobs::JobCommand;

use crate::error::{Rejected, RuntimeError};

const NOW: i64 = 1_753_600_000;

/// Inserts a track so a job has something to reference, then enqueues a job
/// and claims it, leaving it `running` with a lease — the exact state a
/// crashed process leaves behind.
///
/// Returns the job id. Lives here rather than in `runtime_tests` because it
/// is job-facade knowledge, and the crash-recovery test only borrows it.
pub(crate) fn enqueue_running_job(database: &Path) -> i64 {
    let conn = reprise_core::db::open_migrated(Some(database)).expect("the database opens");
    seed_track(&conn, 1);
    let staging = tempfile::tempdir().expect("a staging directory");
    let outcome =
        ai_jobs::enqueue_instrumental(&conn, &StagingStore::new(staging.path()), 1, "model@1", NOW)
            .expect("enqueueing succeeds");
    let job_id = outcome.job_id();
    ai_jobs::claim_next(&conn, 7, NOW, 600)
        .expect("claiming succeeds")
        .expect("the freshly enqueued job is claimable");
    job_id
}

/// A minimal library row. Jobs reference tracks, so one has to exist; the
/// path is never opened.
fn seed_track(conn: &rusqlite::Connection, id: i64) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
         VALUES (?1, ?2, 'Track', 'Artist', 1, 1, 1)",
        rusqlite::params![id, format!("/music/{id}.flac")],
    )
    .expect("the fixture track inserts");
}

fn database() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join("reprise.sqlite");
    (directory, path)
}

#[test]
fn a_claimed_job_is_reported_as_running() {
    let (_directory, path) = database();
    let job_id = enqueue_running_job(&path);
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();

    let snapshot = super::snapshot_of(&conn, job_id)
        .expect("reading succeeds")
        .expect("the job exists");

    assert_eq!(snapshot.job_id, job_id);
    assert_eq!(snapshot.state, "running");
    assert_eq!(snapshot.kind, "instrumental");
    assert!(!snapshot.cancel_requested);
    assert!(super::is_active(&conn).unwrap());
}

#[test]
fn cancelling_a_running_job_records_the_request_without_claiming_it_stopped() {
    let (_directory, path) = database();
    let job_id = enqueue_running_job(&path);
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();

    let touched = super::command(&conn, NOW, &JobCommand::Cancel(job_id))
        .expect("cancelling a running job is admissible");

    assert_eq!(touched, job_id);
    let snapshot = super::snapshot_of(&conn, job_id).unwrap().unwrap();
    assert!(
        snapshot.cancel_requested,
        "the ask is recorded immediately …"
    );
    assert_eq!(
        snapshot.state, "running",
        "… but the state still reports what actually happened: the worker \
         acknowledges between chunks, so cancellation is a request, never an \
         assertion"
    );
}

#[test]
fn cancelling_a_job_that_does_not_exist_is_rejected() {
    let (_directory, path) = database();
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();

    assert_eq!(
        super::command(&conn, NOW, &JobCommand::Cancel(404)).expect_err("there is no such job"),
        RuntimeError::Rejected(Rejected::UnknownJob)
    );
}

#[test]
fn saving_and_discarding_say_so_instead_of_silently_doing_nothing() {
    let (_directory, path) = database();
    let job_id = enqueue_running_job(&path);
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();

    for command in [JobCommand::Save(job_id), JobCommand::Discard(job_id)] {
        assert_eq!(
            super::command(&conn, NOW, &command)
                .expect_err("the staging store is not the runtime's yet"),
            RuntimeError::Rejected(Rejected::UnsupportedCommand),
            "a client learns the command is unserved rather than watching a \
             no-op look like success"
        );
    }
}

#[test]
fn an_empty_job_table_is_idle() {
    let (_directory, path) = database();
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();

    assert!(super::snapshots(&conn).unwrap().is_empty());
    assert!(!super::is_active(&conn).unwrap());
}
