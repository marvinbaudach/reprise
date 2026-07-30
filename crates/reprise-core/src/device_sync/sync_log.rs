//! What a sync run actually did, kept so it can be answered later (MTP-20).
//!
//! A run is written as it happens rather than at the end: `start_run` records
//! it as `Running`, `finish_run` closes it. A run that never gets closed —
//! the app died, the cable was pulled — is not lost but marked `Interrupted`
//! the next time a sync starts, because "it never finished" is exactly the
//! answer someone is looking for afterwards.
//!
//! Successful copies are counted, not itemized. Only deviations get a row,
//! so a 278-file run leaves a handful of lines instead of hundreds.

use rusqlite::Row;

use super::machine::SyncOutcome;
use crate::db::DbError;

/// How many runs are kept before the oldest ages out.
pub const RETAINED_RUNS: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Running,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}

impl RunOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            "failed" => Self::Failed,
            "interrupted" => Self::Interrupted,
            _ => Self::Running,
        }
    }
}

/// Why a file did not simply get copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviationKind {
    Skipped,
    Failed,
    Deleted,
    ConversionFallback,
    PlaylistWriteFailed,
}

impl DeviationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Failed => "failed",
            Self::Deleted => "deleted",
            Self::ConversionFallback => "conversion_fallback",
            Self::PlaylistWriteFailed => "playlist_write_failed",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "failed" => Self::Failed,
            "deleted" => Self::Deleted,
            "conversion_fallback" => Self::ConversionFallback,
            "playlist_write_failed" => Self::PlaylistWriteFailed,
            _ => Self::Skipped,
        }
    }
}

/// What is known when a run begins.
#[derive(Debug, Clone)]
pub struct RunStart {
    pub device_serial: String,
    pub device_name: String,
    pub transfer_profile: String,
    pub started_at: i64,
    pub planned: u32,
}

/// What is known when it ends.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub finished_at: i64,
    pub outcome: RunOutcome,
    pub copied: u32,
    pub skipped: u32,
    pub deleted: u32,
    pub failed: u32,
    pub bytes_copied: u64,
    pub detail: Option<String>,
}

/// What the runner counted while the run was going.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunCounters {
    pub copied: u32,
    pub skipped: u32,
    pub deleted: u32,
    pub failed: u32,
    pub bytes_copied: u64,
}

/// Turns the machine's outcome and the runner's counters into the entry that
/// gets recorded. A run that lost individual tracks without a stage failing
/// says so, because otherwise its entry would read as unexplained.
pub fn summarize(outcome: &SyncOutcome, counters: RunCounters, finished_at: i64) -> RunSummary {
    let (outcome, detail) = match outcome {
        SyncOutcome::Completed { .. } => (RunOutcome::Completed, None),
        SyncOutcome::Cancelled => (RunOutcome::Cancelled, None),
        SyncOutcome::Failed {
            terminal_error,
            failed_tracks,
        } => (
            RunOutcome::Failed,
            terminal_error.clone().or_else(|| {
                (!failed_tracks.is_empty())
                    .then(|| format!("{} tracks failed", failed_tracks.len()))
            }),
        ),
    };
    RunSummary {
        finished_at,
        outcome,
        copied: counters.copied,
        skipped: counters.skipped,
        deleted: counters.deleted,
        failed: counters.failed,
        bytes_copied: counters.bytes_copied,
        detail,
    }
}

/// One file that did not go through cleanly.
#[derive(Debug, Clone)]
pub struct Deviation {
    pub kind: DeviationKind,
    pub track_id: Option<i64>,
    pub device_path: String,
    pub detail: String,
}

/// A recorded run, as shown in the device page's history.
#[derive(Debug, Clone)]
pub struct RunRecord {
    pub id: i64,
    pub device_serial: String,
    pub device_name: String,
    pub transfer_profile: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub outcome: RunOutcome,
    pub planned: u32,
    pub copied: u32,
    pub skipped: u32,
    pub deleted: u32,
    pub failed: u32,
    pub bytes_copied: u64,
    pub detail: Option<String>,
}

/// Opens a run. Any run still marked `Running` belongs to a session that
/// died, so it is closed as `Interrupted` rather than left ambiguous.
pub fn start_run(db: &crate::db::Db, start: &RunStart) -> Result<i64, DbError> {
    let conn = db.conn();
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "UPDATE sync_runs SET outcome = 'interrupted' WHERE outcome = 'running'",
        [],
    )?;
    transaction.execute(
        "INSERT INTO sync_runs \
           (device_serial, device_name, transfer_profile, started_at, outcome, planned) \
         VALUES (?1, ?2, ?3, ?4, 'running', ?5)",
        rusqlite::params![
            start.device_serial,
            start.device_name,
            start.transfer_profile,
            start.started_at,
            start.planned,
        ],
    )?;
    let run = transaction.last_insert_rowid();
    // Both statements share one keep-list so a run and its deviations can
    // never age apart.
    const KEEP: &str = "SELECT id FROM sync_runs ORDER BY started_at DESC, id DESC LIMIT ?1";
    transaction.execute(
        &format!("DELETE FROM sync_events WHERE run_id NOT IN ({KEEP})"),
        rusqlite::params![RETAINED_RUNS as i64],
    )?;
    transaction.execute(
        &format!("DELETE FROM sync_runs WHERE id NOT IN ({KEEP})"),
        rusqlite::params![RETAINED_RUNS as i64],
    )?;
    transaction.commit()?;
    Ok(run)
}

/// Closes a run with its balance.
pub fn finish_run(db: &crate::db::Db, run: i64, summary: &RunSummary) -> Result<(), DbError> {
    let conn = db.conn();
    conn.execute(
        "UPDATE sync_runs SET \
           finished_at = ?2, outcome = ?3, copied = ?4, skipped = ?5, deleted = ?6, \
           failed = ?7, bytes_copied = ?8, detail = ?9 \
         WHERE id = ?1",
        rusqlite::params![
            run,
            summary.finished_at,
            summary.outcome.as_str(),
            summary.copied,
            summary.skipped,
            summary.deleted,
            summary.failed,
            i64::try_from(summary.bytes_copied).unwrap_or(i64::MAX),
            summary.detail,
        ],
    )?;
    Ok(())
}

/// Records one file that did not go through cleanly.
pub fn note_deviation(db: &crate::db::Db, run: i64, deviation: &Deviation) -> Result<(), DbError> {
    let conn = db.conn();
    conn.execute(
        "INSERT INTO sync_events (run_id, kind, track_id, device_path, detail) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            run,
            deviation.kind.as_str(),
            deviation.track_id,
            deviation.device_path,
            deviation.detail,
        ],
    )?;
    Ok(())
}

/// The most recent runs, newest first.
pub fn recent_runs(db: &crate::db::Db, limit: usize) -> Result<Vec<RunRecord>, DbError> {
    let conn = db.conn();
    let mut statement = conn.prepare(
        "SELECT id, device_serial, device_name, transfer_profile, started_at, finished_at, \
                outcome, planned, copied, skipped, deleted, failed, bytes_copied, detail \
         FROM sync_runs ORDER BY started_at DESC, id DESC LIMIT ?1",
    )?;
    let runs = statement
        .query_map(rusqlite::params![limit as i64], read_run)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(runs)
}

/// The deviations of one run, in the order they happened.
pub fn deviations(db: &crate::db::Db, run: i64) -> Result<Vec<Deviation>, DbError> {
    let conn = db.conn();
    let mut statement = conn.prepare(
        "SELECT kind, track_id, device_path, detail FROM sync_events \
         WHERE run_id = ?1 ORDER BY rowid",
    )?;
    let found = statement
        .query_map(rusqlite::params![run], read_deviation)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(found)
}

fn read_run(row: &Row<'_>) -> Result<RunRecord, rusqlite::Error> {
    let outcome: String = row.get(6)?;
    Ok(RunRecord {
        id: row.get(0)?,
        device_serial: row.get(1)?,
        device_name: row.get(2)?,
        transfer_profile: row.get(3)?,
        started_at: row.get(4)?,
        finished_at: row.get(5)?,
        outcome: RunOutcome::from_str(&outcome),
        planned: row.get(7)?,
        copied: row.get(8)?,
        skipped: row.get(9)?,
        deleted: row.get(10)?,
        failed: row.get(11)?,
        // SQLite has no unsigned integers; the column is checked >= 0.
        bytes_copied: row.get::<_, i64>(12)?.max(0) as u64,
        detail: row.get(13)?,
    })
}

fn read_deviation(row: &Row<'_>) -> Result<Deviation, rusqlite::Error> {
    let kind: String = row.get(0)?;
    Ok(Deviation {
        kind: DeviationKind::from_str(&kind),
        track_id: row.get(1)?,
        device_path: row.get(2)?,
        detail: row.get(3)?,
    })
}
