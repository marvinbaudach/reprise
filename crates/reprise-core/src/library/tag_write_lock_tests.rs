use rusqlite::Connection;

use super::tag_write_lock::claim_tag_write_slot;
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
