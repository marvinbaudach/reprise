//! `scan` — walk a folder into the library.

use std::path::PathBuf;

use reprise_core::db::Db;
use reprise_core::library::scanner::{self, ScanOutcome, ScanReport};
use reprise_core::library::settings;
use serde_json::json;

use crate::error::CliError;
use crate::output::print_json;
use crate::retry::{scan_is_busy, with_retry};

/// Scans `path` (or the configured library root when omitted) and reports what
/// the walk changed. A completed scan appends a single `change_log` row, so a
/// running app refreshes live.
pub fn run(db: &Db, path: Option<PathBuf>, json_output: bool) -> Result<(), CliError> {
    let root = resolve_root(db, path)?;
    let outcome = with_retry(|| scanner::scan_folder(db, &root), scan_is_busy)
        .map_err(|error| CliError::Database(error.to_string()))?;

    match outcome {
        ScanOutcome::Completed(report) => {
            report_completed(&root, &report, json_output);
            Ok(())
        }
        ScanOutcome::RootUnavailable { root } => {
            let display = root.display().to_string();
            if json_output {
                print_json(&json!({ "root": display, "outcome": "root_unavailable" }));
            }
            Err(CliError::Unavailable(format!(
                "scan root {display} is unavailable — nothing was scanned"
            )))
        }
    }
}

/// Resolves the folder to scan: the explicit argument, else the configured
/// library root, else a clear error.
fn resolve_root(db: &Db, path: Option<PathBuf>) -> Result<PathBuf, CliError> {
    if let Some(path) = path {
        return Ok(path);
    }
    match settings::get_library_root(db)? {
        Some(root) => Ok(PathBuf::from(root)),
        None => Err(CliError::InvalidInput(
            "no folder given and no library root configured".to_string(),
        )),
    }
}

fn report_completed(root: &std::path::Path, report: &ScanReport, json_output: bool) {
    let display = root.display().to_string();
    if json_output {
        print_json(&json!({
            "root": display,
            "outcome": "completed",
            "added": report.added,
            "updated": report.updated,
            "skipped": report.skipped_unchanged,
            "excluded": report.excluded,
            "errors": report.errors,
            "moved": report.moved,
            "vanished": report.vanished,
            "healed": report.healed,
        }));
    } else {
        println!(
            "scanned {display}: {} added, {} updated, {} moved, {} vanished, {} errors",
            report.added, report.updated, report.moved, report.vanished, report.errors
        );
        // Plan 3.3: a running app's watcher scans the same tree anyway; a
        // second scan is harmless (WAL + busy-retry). Kept on stderr so it
        // never contaminates piped output.
        eprintln!("note: if the Reprise app is running, it also picks up these changes live");
    }
}
