//! `instrumental` subcommands: create (and, added later, save/discard/wait).
//!
//! These drive the package-D AI-job facades (`ai_jobs`, `ai_staging`,
//! `ai_promotion`). `create` registers vocal-removal jobs; without a running
//! worker (the app or `jobs work`) they stay `queued`, and the output says so
//! honestly (plan 3.2). Dedup is a skip with a reference to the existing work,
//! never a second render (Beschluss 16).

use std::path::PathBuf;

use reprise_core::ai_jobs::{self, BatchOutcome, EnqueueOutcome};
use reprise_core::ai_staging::StagingStore;
use reprise_core::queries;
use rusqlite::Connection;
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
/// dedup fingerprint and the provenance attribution agree across every surface.
///
/// NOTE (D-facade gap, reported not fixed): `reprise-core` exposes no shared
/// "current instrumental model id" constant, so the app, the CLI, and the MCP
/// each hardcode this string and could drift. This value matches the htdemucs
/// export the runtime spike recommends (`docs/research/stem-separation-runtime.md`)
/// and the `"htdemucs@4"` used throughout core's promotion tests.
pub const DEFAULT_INSTRUMENTAL_MODEL: &str = "htdemucs@4";

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

    fn as_json(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Stage => "stage",
        }
    }
}

/// Registers instrumental jobs for `track_ids`. One id enqueues a single job;
/// several form one batch. Existing work is referenced, not re-rendered.
pub fn create(
    conn: &mut Connection,
    staging_dir: Option<&PathBuf>,
    track_ids: &[i64],
    mode: SaveMode,
    json_output: bool,
) -> Result<(), CliError> {
    require_tracks_exist(conn, track_ids)?;
    let store = staging::resolve(staging_dir);
    let now = now_unix();

    let outcome = enqueue(conn, &store, track_ids, now)?;
    if json_output {
        emit_json(&outcome, mode);
    } else {
        emit_text(&outcome, mode);
    }
    Ok(())
}

/// A batch enqueue result in a shape uniform across the single- and multi-id
/// paths: `batch_id` is `None` for a single job (the facade groups only
/// multi-select batches).
struct CreateOutcome {
    batch_id: Option<String>,
    jobs: Vec<(i64, EnqueueOutcome)>,
}

/// Enqueues one job (single id) or a batch (several ids), pairing each outcome
/// with its source track id in input order. Wrapped in the busy-retry policy —
/// a long foreign write (e.g. a running app's rescan) must not fail the enqueue.
fn enqueue(
    conn: &Connection,
    store: &StagingStore,
    track_ids: &[i64],
    now: i64,
) -> Result<CreateOutcome, CliError> {
    if let [single] = track_ids {
        let outcome = with_retry(
            || ai_jobs::enqueue_instrumental(conn, store, *single, DEFAULT_INSTRUMENTAL_MODEL, now),
            rusqlite_is_busy,
        )?;
        Ok(CreateOutcome {
            batch_id: None,
            jobs: vec![(*single, outcome)],
        })
    } else {
        let BatchOutcome { batch_id, jobs } = with_retry(
            || {
                ai_jobs::enqueue_instrumental_batch(
                    conn,
                    store,
                    track_ids,
                    DEFAULT_INSTRUMENTAL_MODEL,
                    now,
                )
            },
            rusqlite_is_busy,
        )?;
        Ok(CreateOutcome {
            batch_id: Some(batch_id),
            jobs: track_ids.iter().copied().zip(jobs).collect(),
        })
    }
}

/// Fails with [`CliError::NotFound`] if any id has no track row, before any job
/// is enqueued — a batch is all-or-nothing, so this validates the whole input
/// up front rather than letting a foreign-key violation surface mid-batch.
fn require_tracks_exist(conn: &Connection, track_ids: &[i64]) -> Result<(), CliError> {
    for &id in track_ids {
        if queries::query_track_summary(conn, id)?.is_none() {
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
            SaveMode::Save => {
                println!("note: finished renders will be saved into the library");
            }
            SaveMode::Stage => println!(
                "note: finished renders will wait in staging for `instrumental save`/`discard`"
            ),
        }
    }
}
