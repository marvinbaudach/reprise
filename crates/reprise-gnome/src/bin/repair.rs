//! Standalone CLI for diagnosing and repairing audio file metadata.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use reprise_core::library::repair::{
    self, diagnose, diagnose_dir, diagnose_library, repair as do_repair, Diagnosis, ExternalFixer,
    FixOutcome,
};

#[derive(Parser)]
#[command(name = "reprise-repair", about = "Diagnose and repair audio file metadata")]
struct Cli {
    /// Files or directories to scan (default: all tracks from reprise DB).
    paths: Vec<PathBuf>,

    /// Actually repair (default: diagnose only / dry-run).
    #[arg(long)]
    fix: bool,

    /// Skip .bak backup creation.
    #[arg(long)]
    no_backup: bool,

    /// Output as JSON.
    #[arg(long)]
    json: bool,

    /// Summary only, no per-file output.
    #[arg(long, short)]
    quiet: bool,
}

/// Calls `mp3val -f -si <path>` to add a missing Xing/VBRI header.
struct Mp3ValFixer;

impl ExternalFixer for Mp3ValFixer {
    fn fix_vbr_header(&self, path: &Path) -> FixOutcome {
        let output = match std::process::Command::new("mp3val")
            .args(["-f", "-si"])
            .arg(path)
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                return FixOutcome::Failed {
                    error: format!("failed to run mp3val: {e}"),
                };
            }
        };
        if output.status.success() {
            FixOutcome::Fixed
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            FixOutcome::Failed {
                error: format!("mp3val exited {}: {stderr}", output.status),
            }
        }
    }
}

fn collect_diagnoses(cli: &Cli) -> Vec<Diagnosis> {
    if cli.paths.is_empty() {
        // Library mode: read from reprise DB.
        let db_path = reprise_core::db::default_path();
        if !db_path.exists() {
            eprintln!("No reprise database found at {}", db_path.display());
            eprintln!("Pass file/directory paths as arguments instead.");
            return Vec::new();
        }
        let conn = match reprise_core::db::open(Some(&db_path)) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to open database: {e}");
                return Vec::new();
            }
        };
        eprintln!("Scanning tracks from reprise library...");
        diagnose_library(&conn)
    } else {
        let mut all = Vec::new();
        for path in &cli.paths {
            if path.is_dir() {
                all.extend(diagnose_dir(path));
            } else if path.is_file() {
                match diagnose(path) {
                    Ok(d) => all.push(d),
                    Err(e) => eprintln!("  {}: {e}", path.display()),
                }
            } else {
                eprintln!("  {}: not found", path.display());
            }
        }
        all
    }
}

fn print_diagnosis(d: &Diagnosis) {
    println!("  {}", d.path.display());
    for issue in &d.issues {
        println!("    \u{26a0} {issue}");
    }
}

fn print_report(path: &Path, reports: &[repair::RepairReport]) {
    for r in reports {
        let symbol = match &r.outcome {
            FixOutcome::Fixed => "\u{2713}",
            FixOutcome::Skipped { .. } => "-",
            FixOutcome::Failed { .. } => "\u{2717}",
        };
        let detail = match &r.outcome {
            FixOutcome::Fixed => r.issue.to_string(),
            FixOutcome::Skipped { reason } => format!("{} (skipped: {reason})", r.issue),
            FixOutcome::Failed { error } => format!("{} (FAILED: {error})", r.issue),
        };
        println!("  {symbol} {} \u{2014} {detail}", path.display());
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.json {
        eprintln!("--json output is not yet implemented");
        return ExitCode::FAILURE;
    }

    let all_diagnoses = collect_diagnoses(&cli);
    let with_issues: Vec<_> = all_diagnoses
        .into_iter()
        .filter(|d| !d.issues.is_empty())
        .collect();

    if with_issues.is_empty() {
        if !cli.quiet {
            println!("No issues found.");
        }
        return ExitCode::SUCCESS;
    }

    if !cli.fix {
        // Dry-run mode: print diagnoses and exit.
        if !cli.quiet {
            for d in &with_issues {
                print_diagnosis(d);
            }
            println!();
        }

        let issue_count: usize = with_issues.iter().map(|d| d.issues.len()).sum();
        println!(
            "Found {issue_count} issue(s) in {} file(s).",
            with_issues.len()
        );
        println!("Run with --fix to repair.");
        return ExitCode::SUCCESS;
    }

    // Fix mode.
    let fixer = Mp3ValFixer;
    let backup = !cli.no_backup;
    let mut fixed = 0u32;
    let mut failed = 0u32;

    if !cli.quiet {
        println!("Repairing {} file(s)...\n", with_issues.len());
    }

    for d in &with_issues {
        let reports = do_repair(&d.path, &d.issues, &fixer, backup);
        if !cli.quiet {
            print_report(&d.path, &reports);
        }
        if reports.iter().all(|r| r.outcome == FixOutcome::Fixed) {
            fixed += 1;
        } else if reports
            .iter()
            .any(|r| matches!(r.outcome, FixOutcome::Failed { .. }))
        {
            failed += 1;
        }
    }

    println!();
    println!(
        "Repaired {fixed}/{total} file(s). {failed} failed.{bak_note}",
        total = with_issues.len(),
        bak_note = if backup {
            " Backups in *.bak"
        } else {
            ""
        },
    );

    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
