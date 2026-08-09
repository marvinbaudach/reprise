//! One persisted due-check for automatic library maintenance at startup.
//!
//! Exact tasks compare the revision of their last completed pass with the
//! scanner-maintained library-input revision. The revision changes only when
//! a scan changes catalog inputs, so reading it is constant-time and cannot
//! collide like a timestamp/count heuristic.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::Db;

const LIBRARY_SIGNATURE_KEY: &str = "startup_tasks.library_signature";
const RECORD_PREFIX: &str = "startup_tasks.completed.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupTask {
    Spectrogram,
    CoverDownload,
    Lyrics,
}

impl StartupTask {
    pub const EXACT: [Self; 3] = [Self::Spectrogram, Self::CoverDownload, Self::Lyrics];

    pub fn log_name(self) -> &'static str {
        match self {
            Self::Spectrogram => "spectrogram batch",
            Self::CoverDownload => "cover batch",
            Self::Lyrics => "lyrics batch",
        }
    }

    fn key(self) -> String {
        let suffix = match self {
            Self::Spectrogram => "spectrogram",
            Self::CoverDownload => "covers",
            Self::Lyrics => "lyrics",
        };
        format!("{RECORD_PREFIX}{suffix}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibrarySignature(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueReason {
    NeverCompleted,
    LibrarySignatureChanged {
        previous: LibrarySignature,
        current: LibrarySignature,
    },
    LibrarySignatureUnchanged {
        completed_at: i64,
        signature: LibrarySignature,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueDecision {
    Run(DueReason),
    Skip(DueReason),
}

impl DueDecision {
    pub fn is_due(self) -> bool {
        matches!(self, Self::Run(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct TaskRecord {
    completed_at: i64,
    signature: LibrarySignature,
}

#[derive(Debug, Error)]
pub enum StartupTaskError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid library signature: {0}")]
    InvalidLibrarySignature(String),
}

pub fn exact_signature_decision(
    db: &Db,
    task: StartupTask,
) -> Result<DueDecision, StartupTaskError> {
    let current = current_signature_in(db.conn())?;
    let Some(stored) = crate::library::settings::get_setting_in(db.conn(), &task.key())? else {
        return Ok(DueDecision::Run(DueReason::NeverCompleted));
    };
    let record = match serde_json::from_str::<TaskRecord>(&stored) {
        Ok(record) => record,
        Err(error) => {
            tracing::warn!(
                task = task.log_name(),
                %error,
                "invalid startup-task completion record; running conservatively"
            );
            return Ok(DueDecision::Run(DueReason::NeverCompleted));
        }
    };
    if record.signature == current {
        Ok(DueDecision::Skip(DueReason::LibrarySignatureUnchanged {
            completed_at: record.completed_at,
            signature: current,
        }))
    } else {
        Ok(DueDecision::Run(DueReason::LibrarySignatureChanged {
            previous: record.signature,
            current,
        }))
    }
}

pub fn record_completed(db: &Db, task: StartupTask) -> Result<(), StartupTaskError> {
    record_completed_at(db, task, now_unix())
}

pub fn record_completed_at(
    db: &Db,
    task: StartupTask,
    completed_at: i64,
) -> Result<(), StartupTaskError> {
    let record = TaskRecord {
        completed_at,
        signature: current_signature_in(db.conn())?,
    };
    let value = serde_json::to_string(&record).expect("TaskRecord serialization cannot fail");
    set_internal_setting_in(db.conn(), &task.key(), &value)?;
    Ok(())
}

pub(crate) fn advance_library_signature_in(
    conn: &Connection,
) -> Result<LibrarySignature, rusqlite::Error> {
    let current = match crate::library::settings::get_setting_in(conn, LIBRARY_SIGNATURE_KEY)? {
        None => LibrarySignature(0),
        Some(value) => match value.parse::<u64>() {
            Ok(value) => LibrarySignature(value),
            Err(_) => {
                tracing::warn!(
                    value,
                    "invalid library signature; restarting its monotonic revision"
                );
                LibrarySignature(0)
            }
        },
    };
    let next = LibrarySignature(current.0.saturating_add(1));
    set_internal_setting_in(conn, LIBRARY_SIGNATURE_KEY, &next.0.to_string())?;
    Ok(next)
}

fn current_signature_in(conn: &Connection) -> Result<LibrarySignature, StartupTaskError> {
    let stored = crate::library::settings::get_setting_in(conn, LIBRARY_SIGNATURE_KEY)?;
    match stored {
        None => Ok(LibrarySignature(0)),
        Some(value) => value
            .parse::<u64>()
            .map(LibrarySignature)
            .map_err(|_| StartupTaskError::InvalidLibrarySignature(value)),
    }
}

/// Writes private scheduler metadata without publishing a user-data change.
///
/// The normal settings facade deliberately appends a `change_log` row. These
/// records are only evidence for whether this process should start work; they
/// change no frontend projection, and publishing them would both wake other
/// frontends and turn one catalog-changing scan into two public events.
fn set_internal_setting_in(
    conn: &Connection,
    key: &str,
    value: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy_track(root: &std::path::Path, name: &str) {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        std::fs::copy(fixture, root.join(name)).unwrap();
    }

    #[test]
    fn an_exact_task_is_skipped_only_after_this_library_signature_completed() {
        let db = crate::db::Db::open_in_memory().unwrap();

        assert!(exact_signature_decision(&db, StartupTask::Spectrogram)
            .unwrap()
            .is_due());

        record_completed_at(&db, StartupTask::Spectrogram, 123).unwrap();
        let decision = exact_signature_decision(&db, StartupTask::Spectrogram).unwrap();

        assert_eq!(
            decision,
            DueDecision::Skip(DueReason::LibrarySignatureUnchanged {
                completed_at: 123,
                signature: LibrarySignature(0),
            })
        );
    }

    #[test]
    fn a_catalog_changing_scan_invalidates_every_exact_task_record() {
        let db = crate::db::Db::open_in_memory().unwrap();
        for task in StartupTask::EXACT {
            record_completed_at(&db, task, 123).unwrap();
        }
        let root = tempfile::tempdir().unwrap();
        copy_track(root.path(), "new.flac");

        crate::library::scanner::scan_folder(&db, root.path()).unwrap();

        for task in StartupTask::EXACT {
            assert_eq!(
                exact_signature_decision(&db, task).unwrap(),
                DueDecision::Run(DueReason::LibrarySignatureChanged {
                    previous: LibrarySignature(0),
                    current: LibrarySignature(1),
                })
            );
        }
    }

    #[test]
    fn a_no_change_scan_does_not_invalidate_completed_exact_work() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let root = tempfile::tempdir().unwrap();
        copy_track(root.path(), "known.flac");
        crate::library::scanner::scan_folder(&db, root.path()).unwrap();
        record_completed_at(&db, StartupTask::CoverDownload, 456).unwrap();

        crate::library::scanner::scan_folder(&db, root.path()).unwrap();

        assert!(!exact_signature_decision(&db, StartupTask::CoverDownload)
            .unwrap()
            .is_due());
    }

    #[test]
    fn a_corrupt_library_signature_never_skips_work() {
        let db = crate::db::Db::open_in_memory().unwrap();
        record_completed_at(&db, StartupTask::Lyrics, 789).unwrap();
        crate::library::settings::set_setting(&db, LIBRARY_SIGNATURE_KEY, "not-a-revision")
            .unwrap();

        assert!(exact_signature_decision(&db, StartupTask::Lyrics).is_err());
    }
}
