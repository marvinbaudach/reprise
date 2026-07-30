//! `instrumental` subcommands: create (and, added later, save/discard/wait).
//!
//! These drive the package-D AI-job facades (`ai_jobs`, `ai_staging`,
//! `ai_promotion`). `create` registers vocal-removal jobs; without a running
//! worker (the app or `jobs work`) they stay `queued`, and the output says so
//! honestly (plan 3.2). Dedup is a skip with a reference to the existing work,
//! never a second render (Beschluss 16).

use std::path::PathBuf;

use reprise_core::ai_jobs::{self, BatchOutcome, EnqueueOutcome};
use reprise_core::ai_promotion::{self, PromotionConfig, PromotionError};
use reprise_core::ai_staging::StagingStore;
use reprise_core::db::Db;
use reprise_core::library::settings;
use reprise_core::queries;
use serde_json::{json, Value};

use crate::clock::now_unix;
use crate::error::CliError;
use crate::json_models;
use crate::output::print_json;
use crate::retry::{rusqlite_is_busy, with_retry};
use crate::staging;

/// The canonical model identifier `instrumental create` records as each job's
/// `params_fingerprint` (dedup key) and, on promotion, as the `REPRISE_AI_MODEL`
/// provenance tag. It must match what the real worker backend produces so the
/// dedup fingerprint and the provenance attribution agree across every surface,
/// so it is sourced from the single canonical constant in `reprise-core` rather
/// than a local literal — the app, the CLI, and the MCP now share one source of
/// truth and can no longer drift apart (the earlier D-facade gap, now closed).
pub const DEFAULT_INSTRUMENTAL_MODEL: &str = reprise_core::stem_separation::CURRENT_MODEL_ID;

/// Whether the finished render should ultimately be promoted into the library
/// (`Save`, the default) or left in staging for an explicit later decision
/// (`Stage`) — Beschluss 15.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    Save,
    Stage,
}

impl SaveMode {
    /// Resolves the mode from the mutually-exclusive `--save`/`--stage` flags.
    /// Neither flag means save (the automation default).
    pub fn from_flags(_save: bool, stage: bool) -> Self {
        if stage {
            Self::Stage
        } else {
            Self::Save
        }
    }

    pub(crate) fn as_json(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Stage => "stage",
        }
    }

    /// The persisted save-intent the enqueue records on every fresh job
    /// (Beschluss 15): `Save` auto-promotes the finished render into the library
    /// on completion, `Stage` leaves it staged for an explicit decision. The
    /// worker's completion path honors this without any manual save step.
    pub(crate) fn auto_promote(self) -> bool {
        matches!(self, Self::Save)
    }
}

/// `--wait`/`--wait-timeout` for `instrumental create`.
#[derive(Debug, Clone, Copy)]
pub struct WaitOptions {
    wait: bool,
    timeout_secs: u64,
}

impl WaitOptions {
    pub fn new(wait: bool, timeout_secs: u64) -> Self {
        Self { wait, timeout_secs }
    }

    pub(crate) fn timeout_secs(self) -> u64 {
        self.timeout_secs
    }
}

/// Registers instrumental jobs for `track_ids`. One id enqueues a single job;
/// several form one batch. Existing work is referenced, not re-rendered. With
/// `--wait`, blocks until each job reaches a terminal state (needs a running
/// worker), promoting finished renders in save mode.
pub fn create(
    db: &Db,
    staging_dir: Option<&PathBuf>,
    track_ids: &[i64],
    mode: SaveMode,
    waiting: WaitOptions,
    json_output: bool,
) -> Result<(), CliError> {
    require_tracks_exist(db, track_ids)?;
    let store = staging::resolve(staging_dir);
    let now = now_unix();

    // In save + wait mode we will promote finished renders, so fail fast if
    // there is nowhere to file them rather than after a long wait.
    let config = if waiting.wait && mode == SaveMode::Save {
        Some(promotion_config(db)?)
    } else {
        None
    };

    let outcome = enqueue(db, &store, track_ids, mode.auto_promote(), now)?;

    if !waiting.wait {
        if json_output {
            emit_json(&outcome, mode);
        } else {
            emit_text(&outcome, mode);
        }
        return Ok(());
    }
    crate::commands::instrumental_wait::wait_for_jobs(
        db,
        &store,
        config.as_ref(),
        &outcome,
        mode,
        waiting,
        json_output,
    )
}

/// A batch enqueue result in a shape uniform across the single- and multi-id
/// paths: `batch_id` is `None` for a single job (the facade groups only
/// multi-select batches). `pub(crate)` so `instrumental_wait` can read it.
pub(crate) struct CreateOutcome {
    pub(crate) batch_id: Option<String>,
    pub(crate) jobs: Vec<(i64, EnqueueOutcome)>,
}

/// Enqueues one job (single id) or a batch (several ids), pairing each outcome
/// with its source track id in input order. Every job goes through the batch
/// facade so the caller's `auto_promote` save-intent is persisted on each fresh
/// job (Beschluss 15); a single id is still reported with `batch_id: null`,
/// since the batch grouping of one job is an internal detail. Wrapped in the
/// busy-retry policy — a long foreign write (e.g. a running app's rescan) must
/// not fail the enqueue.
fn enqueue(
    db: &Db,
    store: &StagingStore,
    track_ids: &[i64],
    auto_promote: bool,
    now: i64,
) -> Result<CreateOutcome, CliError> {
    let BatchOutcome { batch_id, jobs } = with_retry(
        || {
            ai_jobs::enqueue_instrumental_batch(
                db,
                store,
                track_ids,
                DEFAULT_INSTRUMENTAL_MODEL,
                auto_promote,
                now,
            )
        },
        rusqlite_is_busy,
    )?;
    Ok(CreateOutcome {
        // A lone job is not surfaced as a batch — only a multi-select create is.
        batch_id: (track_ids.len() > 1).then_some(batch_id),
        jobs: track_ids.iter().copied().zip(jobs).collect(),
    })
}

/// Fails with [`CliError::NotFound`] if any id has no track row, before any job
/// is enqueued — a batch is all-or-nothing, so this validates the whole input
/// up front rather than letting a foreign-key violation surface mid-batch.
fn require_tracks_exist(db: &Db, track_ids: &[i64]) -> Result<(), CliError> {
    for &id in track_ids {
        if queries::query_track_summary(db, id)?.is_none() {
            return Err(CliError::NotFound(format!("track {id}")));
        }
    }
    Ok(())
}

fn emit_json(outcome: &CreateOutcome, mode: SaveMode) {
    let jobs: Vec<Value> = outcome
        .jobs
        .iter()
        .map(|(track_id, enqueue_outcome)| {
            json_models::enqueue_outcome(*track_id, *enqueue_outcome)
        })
        .collect();
    print_json(&json!({
        "batch_id": outcome.batch_id,
        "save": mode.as_json(),
        "jobs": jobs,
    }));
}

fn emit_text(outcome: &CreateOutcome, mode: SaveMode) {
    let mut created = 0usize;
    for (track_id, enqueue_outcome) in &outcome.jobs {
        match enqueue_outcome {
            EnqueueOutcome::Created { job_id } => {
                created += 1;
                println!("queued job {job_id} for track {track_id}");
            }
            EnqueueOutcome::Deduplicated {
                job_id,
                result_track_id: Some(result),
            } => {
                println!(
                    "track {track_id} already has a saved instrumental (job {job_id}, track {result})"
                );
            }
            EnqueueOutcome::Deduplicated {
                job_id,
                result_track_id: None,
            } => {
                println!("track {track_id} is already being converted (job {job_id})");
            }
        }
    }
    if created > 0 {
        // Honest about the two-worker reality (plan 3.2): nothing renders until
        // a worker runs.
        println!(
            "note: {created} job(s) queued — run `reprise-cli jobs work` or start the Reprise app to process them"
        );
        match mode {
            SaveMode::Save => println!(
                "note: each finished render is saved into your library automatically once a worker renders it"
            ),
            SaveMode::Stage => println!(
                "note: finished renders will wait in staging for `instrumental save`/`discard`"
            ),
        }
    }
}

/// Promotes each finished, staged render into the library (the save decision).
/// Requires a configured library root — promotion files live under
/// `<root>/Reprise Instrumentals/…` behind the core path guard. Per-job outcomes
/// are reported together and the process exits non-zero if any job fails, so a
/// partial "save all" is never silently reported as success.
pub fn save(
    db: &Db,
    staging_dir: Option<&PathBuf>,
    job_ids: &[i64],
    json_output: bool,
) -> Result<(), CliError> {
    let config = promotion_config(db)?;
    let store = staging::resolve(staging_dir);
    let now = now_unix();
    let mut rows = Vec::new();
    let mut first_error: Option<CliError> = None;
    for &job_id in job_ids {
        match ai_promotion::promote(db, &store, &config, job_id, now) {
            Ok(outcome) => {
                if json_output {
                    rows.push(json!({
                        "job_id": job_id,
                        "status": "saved",
                        "result_track_id": outcome.result_track_id,
                        "path": outcome.path.to_string_lossy(),
                    }));
                } else {
                    println!(
                        "saved job {job_id} -> track {} ({})",
                        outcome.result_track_id,
                        outcome.path.display()
                    );
                }
            }
            Err(error) => {
                let mapped = map_promotion_error(error, job_id);
                record_failure(job_id, mapped, json_output, &mut rows, &mut first_error);
            }
        }
    }
    finish_per_job(&rows, json_output, first_error)
}

/// Discards each finished, staged render (deletes the staging file and drops the
/// job out of the conversion view — Beschluss 15). A job that is not a finished,
/// unsaved render is reported as an error for that id.
pub fn discard(
    db: &Db,
    staging_dir: Option<&PathBuf>,
    job_ids: &[i64],
    json_output: bool,
) -> Result<(), CliError> {
    let store = staging::resolve(staging_dir);
    let now = now_unix();
    let mut rows = Vec::new();
    let mut first_error: Option<CliError> = None;
    for &job_id in job_ids {
        let discarded = with_retry(
            || ai_jobs::discard_staged(db, &store, job_id, now),
            rusqlite_is_busy,
        );
        match discarded {
            Ok(true) => {
                if json_output {
                    rows.push(json!({ "job_id": job_id, "status": "discarded" }));
                } else {
                    println!("discarded job {job_id}");
                }
            }
            Ok(false) => {
                let error = CliError::NotFound(format!("staged render for job {job_id}"));
                record_failure(job_id, error, json_output, &mut rows, &mut first_error);
            }
            Err(error) => {
                let mapped = CliError::from(error);
                record_failure(job_id, mapped, json_output, &mut rows, &mut first_error);
            }
        }
    }
    finish_per_job(&rows, json_output, first_error)
}

/// Records one failed per-job action: a JSON error row (json mode) or a stderr
/// line (text mode), keeping the first error to drive the exit code.
fn record_failure(
    job_id: i64,
    error: CliError,
    json_output: bool,
    rows: &mut Vec<Value>,
    first_error: &mut Option<CliError>,
) {
    if json_output {
        rows.push(json!({
            "job_id": job_id,
            "status": "error",
            "error": error.to_string(),
        }));
    } else {
        eprintln!("job {job_id}: {error}");
    }
    if first_error.is_none() {
        *first_error = Some(error);
    }
}

/// Emits the collected JSON rows (json mode) and turns the first per-job error,
/// if any, into the command's exit code.
fn finish_per_job(
    rows: &[Value],
    json_output: bool,
    first_error: Option<CliError>,
) -> Result<(), CliError> {
    if json_output {
        print_json(&Value::Array(rows.to_vec()));
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Builds the promotion config from the configured library root, or a clear
/// error when none is set (promotion has nowhere to file the result).
fn promotion_config(db: &Db) -> Result<PromotionConfig, CliError> {
    match settings::get_library_root(db)? {
        Some(root) => Ok(PromotionConfig::new(root)),
        None => Err(CliError::InvalidInput(
            "no library root configured — cannot save instrumentals".to_string(),
        )),
    }
}

/// Maps a core promotion failure onto the CLI's typed error/exit-code contract.
pub(crate) fn map_promotion_error(error: PromotionError, job_id: i64) -> CliError {
    match error {
        PromotionError::JobNotFound(id) => CliError::NotFound(format!("job {id}")),
        PromotionError::NotPromotable(id) => {
            CliError::InvalidInput(format!("job {id} is not a finished, unsaved render"))
        }
        PromotionError::StagingMissing(id) => {
            CliError::NotFound(format!("staged render for job {id}"))
        }
        PromotionError::PathGuard { attempted } => CliError::InvalidInput(format!(
            "refusing to write outside the instrumentals folder: {attempted}"
        )),
        PromotionError::SourceMetadataUnavailable => {
            CliError::Unavailable(format!("source metadata for job {job_id} is unavailable"))
        }
        PromotionError::Tag(message) | PromotionError::Registration(message) => {
            CliError::Database(message)
        }
        PromotionError::Io(error) => CliError::Database(error.to_string()),
        PromotionError::Db(error) => CliError::from(error),
    }
}
