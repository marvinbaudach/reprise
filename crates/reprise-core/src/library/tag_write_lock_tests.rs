use std::fs::{OpenOptions, TryLockError};
use std::io::{ErrorKind, Read};

use rusqlite::Connection;

use super::tag_write_lock::{
    attempt_after_try_lock, claim_tag_write_slot, TagWriteLiveness, TagWriteLock,
    TagWriteLockAttempt,
};
use super::TagWriteBusy;

#[derive(Debug, thiserror::Error)]
enum ClaimOutcome {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Busy(#[from] TagWriteBusy),
}

fn claim(conn: &Connection) -> Result<(), ClaimOutcome> {
    claim_tag_write_slot(conn)
}

fn schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE tag_write_jobs (id INTEGER PRIMARY KEY, state TEXT NOT NULL);",
    )
    .unwrap();
}

#[test]
fn an_empty_queue_hands_out_the_slot() {
    let conn = Connection::open_in_memory().unwrap();
    schema(&conn);

    assert!(claim(&conn).is_ok());
}

#[test]
fn a_running_job_holds_the_slot() {
    let conn = Connection::open_in_memory().unwrap();
    schema(&conn);
    conn.execute("INSERT INTO tag_write_jobs (state) VALUES ('running')", [])
        .unwrap();

    assert!(matches!(claim(&conn), Err(ClaimOutcome::Busy(_))));
}

/// A broken database is not a busy one. Reporting `TagWriteBusy` for a failed
/// query would tell the caller to wait for a job that does not exist — and an
/// agent that retries on busy would then retry forever against a database that
/// will never answer.
#[test]
fn a_failing_query_reports_the_database_error_instead_of_claiming_busy() {
    let conn = Connection::open_in_memory().unwrap();
    // No `tag_write_jobs` table at all: the probe cannot answer the question.

    let error = claim(&conn).expect_err("a missing table must not read as an answer");

    assert!(
        matches!(error, ClaimOutcome::Database(_)),
        "expected the database error to surface, got {error:?}"
    );
}

#[test]
fn acquired_lock_identifies_its_process_and_blocks_a_second_handle() {
    let directory = tempfile::tempdir().unwrap();
    let held = TagWriteLock::acquire(directory.path()).unwrap();

    assert!(matches!(held, TagWriteLockAttempt::Held(_)));
    assert!(matches!(
        TagWriteLock::acquire(directory.path()).unwrap(),
        TagWriteLockAttempt::Busy
    ));

    let mut diagnostics = String::new();
    OpenOptions::new()
        .read(true)
        .open(directory.path().join("tag-write.lock"))
        .unwrap()
        .read_to_string(&mut diagnostics)
        .unwrap();
    assert_eq!(diagnostics, format!("pid={}\n", std::process::id()));
}

#[test]
fn probe_uses_a_separate_handle_and_sees_this_process_as_live() {
    let directory = tempfile::tempdir().unwrap();
    let held = TagWriteLock::acquire(directory.path()).unwrap();
    assert!(matches!(held, TagWriteLockAttempt::Held(_)));

    assert_eq!(
        TagWriteLock::probe(directory.path()),
        TagWriteLiveness::Live
    );
}

#[test]
fn dropping_the_guard_makes_liveness_absent() {
    let directory = tempfile::tempdir().unwrap();
    let held = TagWriteLock::acquire(directory.path()).unwrap();
    assert!(matches!(held, TagWriteLockAttempt::Held(_)));
    drop(held);

    assert_eq!(
        TagWriteLock::probe(directory.path()),
        TagWriteLiveness::Absent
    );
}

#[test]
fn unsupported_advisory_locks_remain_a_third_attempt_outcome() {
    let directory = tempfile::tempdir().unwrap();
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.path().join("tag-write.lock"))
        .unwrap();

    let attempt = attempt_after_try_lock(
        file,
        Err(TryLockError::Error(ErrorKind::Unsupported.into())),
    )
    .unwrap();

    assert!(matches!(attempt, TagWriteLockAttempt::Unenforceable));
}
