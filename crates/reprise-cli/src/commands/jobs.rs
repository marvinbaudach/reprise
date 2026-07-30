//! `jobs` subcommands: status (and, behind the `worker` feature, `work`).
//!
//! `status` reads the package-D `ai_jobs` facades so the CLI shows the *same*
//! job rows, progress permille and result track ids as the GTK conversion view
//! and the MCP surface (plan 2.2). `work` (the standalone worker host) lives in
//! `crate::commands::worker`, compiled only with the `worker` feature.

use std::path::PathBuf;

use reprise_core::ai_jobs::{self, AiJob};
use reprise_core::ai_staging::StagingStore;
use reprise_core::db::Db;
use serde_json::{json, Value};

use crate::error::CliError;
use crate::json_models;
use crate::output::print_json;
use crate::staging;

/// Percent (0..=100) for a job's `progress_permille` (0..=1000), for text output.
fn percent(progress_permille: u16) -> u16 {
    progress_permille / 10
}

/// Lists AI jobs. With `batch`, restricts to that batch and shows its aggregate
/// progress; without it, lists every non-cancelled job (the conversion view's
/// rows).
pub fn status(
    db: &Db,
    staging_dir: Option<&PathBuf>,
    batch: Option<&str>,
    json_output: bool,
) -> Result<(), CliError> {
    let store = staging::resolve(staging_dir);
    match batch {
        Some(batch_id) => status_batch(db, &store, batch_id, json_output),
        None => status_all(db, &store, json_output),
    }
}

fn status_all(db: &Db, store: &StagingStore, json_output: bool) -> Result<(), CliError> {
    let jobs = ai_jobs::list_active_jobs(db)?;
    if json_output {
        print_json(&Value::Array(job_rows(store, &jobs)));
    } else if jobs.is_empty() {
        println!("no jobs");
    } else {
        for job in &jobs {
            print_job_line(store, job);
        }
    }
    Ok(())
}

fn status_batch(
    db: &Db,
    store: &StagingStore,
    batch_id: &str,
    json_output: bool,
) -> Result<(), CliError> {
    let jobs = ai_jobs::list_jobs_in_batch(db, batch_id)?;
    let progress = ai_jobs::batch_progress(db, batch_id)?;
    if json_output {
        print_json(&json!({
            "batch": json_models::batch_progress(batch_id, &progress),
            "jobs": job_rows(store, &jobs),
        }));
    } else if jobs.is_empty() {
        println!("no jobs in batch {batch_id}");
    } else {
        println!(
            "batch {batch_id}: {}/{} done, {}% overall ({} running, {} queued, {} failed)",
            progress.done,
            progress.total,
            percent(progress.permille),
            progress.running,
            progress.queued,
            progress.failed,
        );
        for job in &jobs {
            print_job_line(store, job);
        }
    }
    Ok(())
}

/// Each job as JSON, annotated with whether a staging render is on disk (a
/// `done`, unsaved job with `staged: true` is immediately playable and
/// promotable).
fn job_rows(store: &StagingStore, jobs: &[AiJob]) -> Vec<Value> {
    jobs.iter()
        .map(|job| {
            let mut value = json_models::job(job);
            value["staged"] = Value::Bool(store.exists(job.id));
            value
        })
        .collect()
}

fn print_job_line(store: &StagingStore, job: &AiJob) {
    let result = job
        .result_track_id
        .map_or_else(|| "-".to_string(), |id| id.to_string());
    let source = job
        .source_track_id
        .map_or_else(|| "-".to_string(), |id| id.to_string());
    let staged = if store.exists(job.id) { " staged" } else { "" };
    println!(
        "{}\t{}\t{}%\tsource={source}\tresult={result}{staged}",
        job.id,
        job.state.as_str(),
        percent(job.progress_permille),
    );
}
