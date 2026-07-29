//! Background jobs, read and steered through the runtime's own database
//! connection.
//!
//! Jobs are the one part of runtime state that is *not* in memory: their
//! rows, leases and progress live in SQLite, which is what makes them the
//! only part that survives a crash. This module therefore holds no state of
//! its own — it translates between `reprise-core`'s job facade and the
//! protocol's snapshots, and it is the reason the runtime owns the writer
//! (§9.1) rather than sharing it.
//!
//! No SQL is assembled here. Every statement goes through a named
//! `reprise-core` facade, exactly as the headless surfaces do.

use reprise_core::ai_jobs::{self, AiJob, CancelOutcome, JobState};
use reprise_runtime_protocol::jobs::{JobCommand, JobSnapshot};
use rusqlite::Connection;

use crate::error::{failed_database, Rejected, RuntimeError};

/// Translates a job row into its wire shape.
///
/// The one non-obvious mapping is `Done`: the core state says the render
/// exists, and whether it was *promoted* is a separate column. The protocol
/// splits that into `staged` and `saved`, because "finished, waiting for you
/// to decide" and "in your library" are different things to a user and to an
/// agent.
fn snapshot(job: &AiJob) -> JobSnapshot {
    let state = match job.state {
        JobState::Queued => "queued",
        JobState::Running => "running",
        JobState::Done if job.result_track_id.is_some() => "saved",
        JobState::Done => "staged",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
    };
    JobSnapshot {
        job_id: job.id,
        kind: job.kind.clone(),
        state: state.to_owned(),
        progress_permille: job.progress_permille,
        batch_id: job.batch_id.clone(),
        source_track_id: job.source_track_id,
        result_track_id: job.result_track_id,
        cancel_requested: job.cancel_requested,
        error_kind: job.error_kind.as_deref().map(sanitize_error_kind),
    }
}

/// The longest a real diagnostic kind (`unsupported_format`, `path_guard`)
/// is expected to be. Generous for that, hopeless for a sentence or a path.
const MAX_ERROR_KIND_LEN: usize = 40;

/// What a client sees in place of anything that fails the shape check below.
const OPAQUE_ERROR_KIND: &str = "error";

/// Keeps `error_kind` down to what the protocol's doc comment promises —
/// "never a path, never the underlying error's message" — even though the
/// column does not.
///
/// `reprise-core`'s promotion path writes `error.to_string()` into this
/// column, and `PromotionError::PathGuard`'s `Display` embeds the absolute
/// path it refused to write outside of: "refusing to write outside the
/// instrumentals folder: /home/…". A local surface reading the database
/// directly is entitled to that whole message; anything that crosses the
/// D-Bus boundary is not, so the runtime is where this gets caught.
///
/// The check is an allow-list of what a kind is *built from*, not a
/// denylist of what a leak might contain — a denylist only ever knows the
/// leaks someone already thought of. A short token of letters, digits, `_`
/// and `-` is what every real kind already looks like; a path has a `/` and
/// a sentence has spaces, so both are already excluded without either
/// needing to be named here.
fn sanitize_error_kind(raw: &str) -> String {
    let looks_like_a_kind = !raw.is_empty()
        && raw.len() <= MAX_ERROR_KIND_LEN
        && raw
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');
    if looks_like_a_kind {
        raw.to_owned()
    } else {
        OPAQUE_ERROR_KIND.to_owned()
    }
}

/// Every job a client should see, in id order.
pub(crate) fn snapshots(conn: &Connection) -> Result<Vec<JobSnapshot>, RuntimeError> {
    let jobs = ai_jobs::list_active_jobs(conn).map_err(|error| failed_database(&error))?;
    Ok(jobs.iter().map(snapshot).collect())
}

/// One job's current shape, or `None` if it is gone.
pub(crate) fn snapshot_of(
    conn: &Connection,
    job_id: i64,
) -> Result<Option<JobSnapshot>, RuntimeError> {
    let job = ai_jobs::get_job(conn, job_id).map_err(|error| failed_database(&error))?;
    Ok(job.as_ref().map(snapshot))
}

/// Whether any job is still unfinished — one of §9.6's four idle conditions.
/// A runtime that shut down here would abandon work to save memory, which is
/// a data-loss feature, not an optimization.
pub(crate) fn is_active(conn: &Connection) -> Result<bool, RuntimeError> {
    let jobs = ai_jobs::list_active_jobs(conn).map_err(|error| failed_database(&error))?;
    Ok(jobs
        .iter()
        .any(|job| matches!(job.state, JobState::Queued | JobState::Running)))
}

/// Applies a job command. Returns the job it touched, so the caller can
/// publish exactly that job's new shape.
pub(crate) fn command(
    conn: &Connection,
    now_unix: i64,
    command: &JobCommand,
) -> Result<i64, RuntimeError> {
    match command {
        JobCommand::Cancel(job_id) => {
            let outcome = ai_jobs::request_cancel(conn, *job_id, now_unix)
                .map_err(|error| failed_database(&error))?;
            match outcome {
                // Both are successes: a queued job is gone, a running one has
                // been asked and will acknowledge between chunks. The
                // snapshot's `cancel_requested` reports the ask, `state`
                // reports what actually happened — the protocol is explicit
                // that cancellation is a request, never an assertion.
                CancelOutcome::CancelledImmediately | CancelOutcome::CancelRequested => Ok(*job_id),
                CancelOutcome::NotCancellable => Err(RuntimeError::Rejected(Rejected::UnknownJob)),
            }
        }
        // Promoting or dropping a staged render moves files through the
        // staging store, which the runtime does not own yet. Task 3.5 adds
        // it together with the rest of the instrumental surface; until then
        // saying so is better than a silent no-op that looks like success.
        JobCommand::Save(_) | JobCommand::Discard(_) => {
            Err(RuntimeError::Rejected(Rejected::UnsupportedCommand))
        }
    }
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
pub(crate) mod jobs_tests;
