//! `instrumental create --wait`: block until each enqueued job reaches a
//! terminal state, promoting finished renders in save mode, then report each
//! outcome. A timeout is the honest "no worker is running" signal (plan 3.2) —
//! the jobs stay queued for a worker to pick up later.

use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use reprise_core::ai_jobs::{self, AiJob, JobState};
use reprise_core::ai_promotion::{self, PromotionConfig};
use reprise_core::ai_staging::StagingStore;
use reprise_core::queries;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::clock::now_unix;
use crate::commands::instrumental::{map_promotion_error, CreateOutcome, SaveMode, WaitOptions};
use crate::error::CliError;
use crate::output::print_json;

/// How often `--wait` re-reads job state. Reads never block on a writer (WAL),
/// so this is a cheap poll, not a busy-spin.
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// The terminal outcome of one waited job.
enum WaitResult {
    /// Finished and promoted into the library (save mode).
    Saved { result_track_id: i64, path: PathBuf },
    /// Finished and left in staging (stage mode).
    Staged,
    /// The render failed.
    Failed { error_kind: String },
    /// The job was cancelled while waiting.
    Cancelled,
    /// The timeout elapsed before the job finished — it stays queued/running
    /// for a worker to pick up later (plan 3.2).
    TimedOut { state: JobState },
}

impl WaitResult {
    fn is_success(&self) -> bool {
        matches!(self, Self::Saved { .. } | Self::Staged)
    }

    fn status(&self) -> &'static str {
        match self {
            Self::Saved { .. } => "saved",
            Self::Staged => "staged",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut { .. } => "timeout",
        }
    }
}

/// Waits for every enqueued job to reach a terminal state (or the timeout),
/// promoting finished renders in save mode, then reports each outcome. Exits
/// non-zero if any job did not end successfully — including a timeout.
pub fn wait_for_jobs(
    conn: &mut Connection,
    store: &StagingStore,
    config: Option<&PromotionConfig>,
    outcome: &CreateOutcome,
    mode: SaveMode,
    waiting: WaitOptions,
    json_output: bool,
) -> Result<(), CliError> {
    let deadline = Instant::now() + Duration::from_secs(waiting.timeout_secs());
    let job_ids: Vec<i64> = outcome.jobs.iter().map(|(_, o)| o.job_id()).collect();
    let source_of: Vec<i64> = outcome.jobs.iter().map(|(track_id, _)| *track_id).collect();

    let mut results: Vec<Option<WaitResult>> = (0..job_ids.len()).map(|_| None).collect();
    loop {
        let mut all_done = true;
        for (index, &job_id) in job_ids.iter().enumerate() {
            if results[index].is_some() {
                continue;
            }
            match settle_job(conn, store, config, mode, job_id)? {
                Some(result) => results[index] = Some(result),
                None => all_done = false,
            }
        }
        if all_done {
            break;
        }
        if Instant::now() >= deadline {
            mark_timeouts(conn, &job_ids, &mut results)?;
            break;
        }
        sleep(WAIT_POLL_INTERVAL);
    }

    let finalized: Vec<WaitResult> = results.into_iter().map(Option::unwrap).collect();
    emit(outcome, &source_of, &job_ids, &finalized, mode, json_output);
    if finalized.iter().all(WaitResult::is_success) {
        Ok(())
    } else {
        Err(CliError::Unavailable(
            "one or more instrumental jobs did not finish successfully".to_string(),
        ))
    }
}

/// Reads one job once. Returns its terminal [`WaitResult`], or `None` if it is
/// still queued/running. A finished, unsaved render is promoted here in save
/// mode.
fn settle_job(
    conn: &mut Connection,
    store: &StagingStore,
    config: Option<&PromotionConfig>,
    mode: SaveMode,
    job_id: i64,
) -> Result<Option<WaitResult>, CliError> {
    let job = ai_jobs::get_job(conn, job_id)?
        .ok_or_else(|| CliError::NotFound(format!("job {job_id}")))?;
    match job.state {
        JobState::Queued | JobState::Running => Ok(None),
        JobState::Failed => Ok(Some(WaitResult::Failed {
            error_kind: job.error_kind.unwrap_or_else(|| "unknown".to_string()),
        })),
        JobState::Cancelled => Ok(Some(WaitResult::Cancelled)),
        JobState::Done => settle_done(conn, store, config, mode, &job),
    }
}

/// Resolves a `done` job: already-saved returns its track; a staged render is
/// promoted in save mode, or reported as staged in stage mode.
fn settle_done(
    conn: &mut Connection,
    store: &StagingStore,
    config: Option<&PromotionConfig>,
    mode: SaveMode,
    job: &AiJob,
) -> Result<Option<WaitResult>, CliError> {
    if let Some(result_track_id) = job.result_track_id {
        // Already promoted (e.g. by the app or a prior save).
        return Ok(Some(WaitResult::Saved {
            result_track_id,
            path: promoted_path(conn, result_track_id),
        }));
    }
    match mode {
        SaveMode::Stage => Ok(Some(WaitResult::Staged)),
        SaveMode::Save => {
            let now = now_unix();
            // `config` is always `Some` in save+wait mode (checked up front).
            let config = config.expect("promotion config present in save+wait mode");
            match ai_promotion::promote(conn, store, config, job.id, now) {
                Ok(promotion) => Ok(Some(WaitResult::Saved {
                    result_track_id: promotion.result_track_id,
                    path: promotion.path,
                })),
                Err(error) => Err(map_promotion_error(error, job.id)),
            }
        }
    }
}

/// Best-effort on-disk path for an already-saved result track (for reporting).
fn promoted_path(conn: &Connection, result_track_id: i64) -> PathBuf {
    queries::query_track_summary(conn, result_track_id)
        .ok()
        .flatten()
        .map_or_else(PathBuf::new, |summary| PathBuf::from(summary.path))
}

/// Records a `TimedOut` result (with the last-seen state) for every job that
/// never reached a terminal state.
fn mark_timeouts(
    conn: &Connection,
    job_ids: &[i64],
    results: &mut [Option<WaitResult>],
) -> Result<(), CliError> {
    for (index, &job_id) in job_ids.iter().enumerate() {
        if results[index].is_some() {
            continue;
        }
        let state = ai_jobs::get_job(conn, job_id)?.map_or(JobState::Queued, |job| job.state);
        results[index] = Some(WaitResult::TimedOut { state });
    }
    Ok(())
}

fn emit(
    outcome: &CreateOutcome,
    source_of: &[i64],
    job_ids: &[i64],
    results: &[WaitResult],
    mode: SaveMode,
    json_output: bool,
) {
    if json_output {
        let jobs: Vec<Value> = (0..results.len())
            .map(|i| result_json(source_of[i], job_ids[i], &results[i]))
            .collect();
        print_json(&json!({
            "batch_id": outcome.batch_id,
            "save": mode.as_json(),
            "waited": true,
            "jobs": jobs,
        }));
    } else {
        for i in 0..results.len() {
            print_line(source_of[i], job_ids[i], &results[i]);
        }
    }
}

fn result_json(source_track_id: i64, job_id: i64, result: &WaitResult) -> Value {
    let mut value = json!({
        "source_track_id": source_track_id,
        "job_id": job_id,
        "status": result.status(),
    });
    match result {
        WaitResult::Saved {
            result_track_id,
            path,
        } => {
            value["result_track_id"] = json!(result_track_id);
            value["path"] = json!(path.to_string_lossy());
        }
        WaitResult::Failed { error_kind } => value["error"] = json!(error_kind),
        WaitResult::TimedOut { state } => value["state"] = json!(state.as_str()),
        WaitResult::Staged | WaitResult::Cancelled => {}
    }
    value
}

fn print_line(source_track_id: i64, job_id: i64, result: &WaitResult) {
    match result {
        WaitResult::Saved {
            result_track_id,
            path,
        } => println!(
            "job {job_id} (track {source_track_id}): saved -> track {result_track_id} ({})",
            path.display()
        ),
        WaitResult::Staged => {
            println!("job {job_id} (track {source_track_id}): staged (run `instrumental save`)");
        }
        WaitResult::Failed { error_kind } => {
            eprintln!("job {job_id} (track {source_track_id}): failed ({error_kind})");
        }
        WaitResult::Cancelled => {
            eprintln!("job {job_id} (track {source_track_id}): cancelled");
        }
        WaitResult::TimedOut { state } => eprintln!(
            "job {job_id} (track {source_track_id}): timed out still {} — is a worker running?",
            state.as_str()
        ),
    }
}
