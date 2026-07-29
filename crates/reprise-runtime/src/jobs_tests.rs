//! Job translation and the one job command the runtime serves today.

use std::path::Path;

use reprise_core::ai_jobs;
use reprise_core::ai_promotion::PromotionError;
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

#[test]
fn sanitize_error_kind_passes_through_a_real_diagnostic_kind() {
    for kind in ["path_guard", "unsupported-format", "io_error", "Timeout2"] {
        assert_eq!(
            super::sanitize_error_kind(kind),
            kind,
            "a short token of letters, digits, `_` and `-` is exactly what a \
             real error_kind looks like, so it must survive unchanged"
        );
    }
}

#[test]
fn sanitize_error_kind_replaces_the_exact_path_guard_message() {
    // The finding's own example: `reprise-core`'s promotion path writes
    // `error.to_string()` into `error_kind`, and this is what that produces
    // for a real `PathGuard` refusal.
    let message = PromotionError::PathGuard {
        attempted: "/home/marvin/Music/outside.flac".into(),
    }
    .to_string();
    assert_eq!(
        message,
        "refusing to write outside the instrumentals folder: \
         /home/marvin/Music/outside.flac"
    );

    assert_eq!(
        super::sanitize_error_kind(&message),
        "error",
        "a filesystem path must never reach a client, and this is the exact \
         string the review found leaking one"
    );
}

#[test]
fn sanitize_error_kind_replaces_anything_containing_a_slash() {
    assert_eq!(super::sanitize_error_kind("a/b"), "error");
    assert_eq!(super::sanitize_error_kind("/etc/passwd"), "error");
}

#[test]
fn sanitize_error_kind_replaces_a_sentence_with_spaces() {
    assert_eq!(
        super::sanitize_error_kind("could not write final tags"),
        "error",
        "a kind is one token; a sentence is a message, and the protocol \
         promises the latter never crosses"
    );
}

#[test]
fn sanitize_error_kind_replaces_an_overlong_token() {
    let too_long = "a".repeat(41);
    assert_eq!(
        super::sanitize_error_kind(&too_long),
        "error",
        "no real error_kind is anywhere near this long; treating length as \
         part of the shape check catches whatever else a denylist of \
         characters alone would miss"
    );
}

#[test]
fn sanitize_error_kind_replaces_an_empty_string() {
    assert_eq!(super::sanitize_error_kind(""), "error");
}

#[test]
fn a_failed_jobs_snapshot_never_carries_the_path_guard_message() {
    let (_directory, path) = database();
    let job_id = enqueue_running_job(&path);
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    let leaky_message = PromotionError::PathGuard {
        attempted: "/home/marvin/Music/outside.flac".into(),
    }
    .to_string();
    ai_jobs::mark_failed(&conn, job_id, 7, &leaky_message, NOW)
        .expect("marking the claimed job failed succeeds");

    let snapshot = super::snapshot_of(&conn, job_id).unwrap().unwrap();

    assert_eq!(snapshot.state, "failed");
    assert_eq!(
        snapshot.error_kind.as_deref(),
        Some("error"),
        "the runtime boundary must sanitize what reprise-core wrote to the \
         column, since a client on the other side of D-Bus receives \
         whatever this snapshot carries"
    );
}
