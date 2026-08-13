//! One due-check register for automatic library maintenance at startup.
//!
//! Signature tasks compare the revision of their last completed pass with the
//! scanner-maintained library-input revision. Filesystem-dependent tasks use
//! the previous process's clean-exit age instead: a library signature cannot
//! reveal a removed sidecar, deleted file, or disconnected drive.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::Db;

const LIBRARY_SIGNATURE_KEY: &str = "startup_tasks.library_signature";
const RECORD_PREFIX: &str = "startup_tasks.completed.";
pub const STARTUP_SCAN_WINDOW_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureTask {
    Spectrogram,
    CoverDownload,
}

impl SignatureTask {
    pub const ALL: [Self; 2] = [Self::Spectrogram, Self::CoverDownload];

    pub fn log_name(self) -> &'static str {
        match self {
            Self::Spectrogram => "spectrogram batch",
            Self::CoverDownload => "cover batch",
        }
    }

    fn key(self) -> String {
        let suffix = match self {
            Self::Spectrogram => "spectrogram",
            Self::CoverDownload => "covers",
        };
        format!("{RECORD_PREFIX}{suffix}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeWindowTask {
    LibraryScan,
    Lyrics,
}

impl TimeWindowTask {
    pub const ALL: [Self; 2] = [Self::LibraryScan, Self::Lyrics];

    pub fn log_name(self) -> &'static str {
        match self {
            Self::LibraryScan => "library scan",
            Self::Lyrics => "lyrics batch",
        }
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
    NoCleanExit,
    RecentCleanExit {
        age_seconds: i64,
    },
    CleanExitTooOld {
        age_seconds: i64,
    },
    LibraryRootChanged,
    ClockMovedBackwards,
}

/// Decides whether filesystem-dependent stopped-app catch-up work is needed.
/// The live watcher and every manual trigger are independent of this decision.
pub fn time_window_decision(
    task: TimeWindowTask,
    previous_session: &crate::library::session::SessionState,
    current_library_root: &str,
    now: i64,
) -> DueDecision {
    let Some(clean_exit) = &previous_session.clean_exit else {
        return DueDecision::Run(DueReason::NoCleanExit);
    };
    if clean_exit.library_root != current_library_root {
        return DueDecision::Run(DueReason::LibraryRootChanged);
    }
    let Some(age_seconds) = now.checked_sub(clean_exit.completed_at) else {
        return DueDecision::Run(DueReason::ClockMovedBackwards);
    };
    if age_seconds < 0 {
        return DueDecision::Run(DueReason::ClockMovedBackwards);
    }
    if age_seconds >= STARTUP_SCAN_WINDOW_SECONDS {
        return DueDecision::Run(DueReason::CleanExitTooOld { age_seconds });
    }
    tracing::info!(
        task = task.log_name(),
        age_seconds,
        "startup task skipped: last clean exit was {} minutes ago",
        age_seconds / 60
    );
    DueDecision::Skip(DueReason::RecentCleanExit { age_seconds })
}

pub fn should_run_time_window(
    task: TimeWindowTask,
    previous_session: &crate::library::session::SessionState,
    current_library_root: &str,
) -> bool {
    time_window_decision(task, previous_session, current_library_root, now_unix()).is_due()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueDecision {
    Run(DueReason),
    Skip(DueReason),
}

/// The exact input revision one running pass is responsible for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactTaskPass {
    task: SignatureTask,
    signature: Option<LibrarySignature>,
}

impl ExactTaskPass {
    pub fn record_completed_or_warn(self, db: &Db) {
        let Some(signature) = self.signature else {
            tracing::warn!(
                task = self.task.log_name(),
                "startup task completion not persisted because its starting signature was unavailable"
            );
            return;
        };
        match record_pass_completed(db, self.task, signature, now_unix()) {
            Ok(true) => {}
            Ok(false) => tracing::info!(
                task = self.task.log_name(),
                started_signature = signature.0,
                "startup task completion not persisted: library signature changed during the pass"
            ),
            Err(error) => tracing::warn!(
                task = self.task.log_name(),
                %error,
                "could not persist startup task completion"
            ),
        }
    }
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
    task: SignatureTask,
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
        tracing::info!(
            task = task.log_name(),
            completed_at = record.completed_at,
            signature = current.0,
            "startup task skipped: library signature unchanged"
        );
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

/// Starts due work with the exact signature it is allowed to settle.
/// A due-state read failure still runs conservatively but receives no
/// settleable signature.
pub fn begin_exact(db: &Db, task: SignatureTask) -> Option<ExactTaskPass> {
    match exact_signature_decision(db, task) {
        Ok(DueDecision::Skip(_)) => None,
        Ok(DueDecision::Run(_)) => Some(ExactTaskPass {
            task,
            signature: match current_signature_in(db.conn()) {
                Ok(signature) => Some(signature),
                Err(error) => {
                    tracing::warn!(
                        task = task.log_name(),
                        %error,
                        "could not capture startup task signature; running conservatively"
                    );
                    None
                }
            },
        }),
        Err(error) => {
            tracing::warn!(
                task = task.log_name(),
                %error,
                "could not decide startup task freshness; running conservatively"
            );
            Some(ExactTaskPass {
                task,
                signature: None,
            })
        }
    }
}

/// Starts work requested directly by the user without consulting startup
/// freshness, while retaining the exact signature the completed pass may
/// settle for the next launch.
pub fn begin_user_triggered(db: &Db, task: SignatureTask) -> ExactTaskPass {
    ExactTaskPass {
        task,
        signature: match current_signature_in(db.conn()) {
            Ok(signature) => Some(signature),
            Err(error) => {
                tracing::warn!(
                    task = task.log_name(),
                    %error,
                    "could not capture user-triggered task signature; running conservatively"
                );
                None
            }
        },
    }
}

/// Seeds a completion record at a controlled time for deterministic tests.
/// Production callers must use [`begin_exact`] and finish its returned pass.
#[doc(hidden)]
pub fn record_completed_at(
    db: &Db,
    task: SignatureTask,
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

fn record_pass_completed(
    db: &Db,
    task: SignatureTask,
    started_signature: LibrarySignature,
    completed_at: i64,
) -> Result<bool, StartupTaskError> {
    let tx = Transaction::new_unchecked(db.conn(), TransactionBehavior::Immediate)?;
    if current_signature_in(&tx)? != started_signature {
        return Ok(false);
    }
    let record = TaskRecord {
        completed_at,
        signature: started_signature,
    };
    let value = serde_json::to_string(&record).expect("TaskRecord serialization cannot fail");
    set_internal_setting_in(&tx, &task.key(), &value)?;
    tx.commit()?;
    Ok(true)
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
    use crate::library::session::{CleanExit, SessionState};

    fn copy_track(root: &std::path::Path, name: &str) {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        std::fs::copy(fixture, root.join(name)).unwrap();
    }

    #[test]
    fn an_exact_task_is_skipped_only_after_this_library_signature_completed() {
        let db = crate::db::Db::open_in_memory().unwrap();

        assert!(exact_signature_decision(&db, SignatureTask::Spectrogram)
            .unwrap()
            .is_due());

        record_completed_at(&db, SignatureTask::Spectrogram, 123).unwrap();
        let decision = exact_signature_decision(&db, SignatureTask::Spectrogram).unwrap();

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
        for task in SignatureTask::ALL {
            record_completed_at(&db, task, 123).unwrap();
        }
        let root = tempfile::tempdir().unwrap();
        copy_track(root.path(), "new.flac");

        crate::library::scanner::scan_folder(&db, root.path()).unwrap();

        for task in SignatureTask::ALL {
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
        record_completed_at(&db, SignatureTask::CoverDownload, 456).unwrap();

        crate::library::scanner::scan_folder(&db, root.path()).unwrap();

        assert!(!exact_signature_decision(&db, SignatureTask::CoverDownload)
            .unwrap()
            .is_due());
    }

    #[test]
    fn a_corrupt_library_signature_never_skips_work() {
        let db = crate::db::Db::open_in_memory().unwrap();
        record_completed_at(&db, SignatureTask::Spectrogram, 789).unwrap();
        crate::library::settings::set_setting(&db, LIBRARY_SIGNATURE_KEY, "not-a-revision")
            .unwrap();

        assert!(exact_signature_decision(&db, SignatureTask::Spectrogram).is_err());
    }

    #[test]
    fn an_unchanged_signature_skip_names_the_task_and_reason_in_the_log() {
        let db = crate::db::Db::open_in_memory().unwrap();
        record_completed_at(&db, SignatureTask::CoverDownload, 321).unwrap();
        let logs = crate::log_capture::CapturedLogs::default();

        logs.capture(|| exact_signature_decision(&db, SignatureTask::CoverDownload).unwrap());

        let logs = logs.joined();
        assert!(logs.contains("cover batch"));
        assert!(logs.contains("library signature unchanged"));
    }

    #[test]
    fn a_pass_cannot_settle_a_signature_that_changed_while_it_was_running() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let pass = begin_exact(&db, SignatureTask::Spectrogram).unwrap();
        advance_library_signature_in(db.conn()).unwrap();

        pass.record_completed_or_warn(&db);

        assert!(exact_signature_decision(&db, SignatureTask::Spectrogram)
            .unwrap()
            .is_due());
    }

    #[test]
    fn a_user_triggered_pass_ignores_due_state_and_settles_the_current_signature() {
        let db = crate::db::Db::open_in_memory().unwrap();
        record_completed_at(&db, SignatureTask::CoverDownload, 123).unwrap();
        assert!(begin_exact(&db, SignatureTask::CoverDownload).is_none());

        let pass = begin_user_triggered(&db, SignatureTask::CoverDownload);
        pass.record_completed_or_warn(&db);

        assert!(begin_exact(&db, SignatureTask::CoverDownload).is_none());
    }

    #[test]
    fn a_recent_clean_exit_for_the_same_root_skips_and_logs_its_age() {
        let state = SessionState {
            clean_exit: Some(CleanExit {
                completed_at: 1_000,
                library_root: "/music".into(),
            }),
            ..SessionState::default()
        };
        let logs = crate::log_capture::CapturedLogs::default();

        let decision =
            logs.capture(|| time_window_decision(TimeWindowTask::Lyrics, &state, "/music", 1_240));

        assert_eq!(
            decision,
            DueDecision::Skip(DueReason::RecentCleanExit { age_seconds: 240 })
        );
        let logs = logs.joined();
        assert!(logs.contains("lyrics batch"));
        assert!(logs.contains("last clean exit was 4 minutes ago"));
    }

    #[test]
    fn filesystem_dependent_lyrics_are_not_an_exact_signature_task() {
        assert_eq!(
            SignatureTask::ALL,
            [SignatureTask::Spectrogram, SignatureTask::CoverDownload]
        );
        assert_eq!(
            TimeWindowTask::ALL,
            [TimeWindowTask::LibraryScan, TimeWindowTask::Lyrics]
        );
    }

    #[test]
    fn startup_scan_runs_without_a_clean_exit_at_the_boundary_or_after_root_change() {
        let missing = SessionState::default();
        assert_eq!(
            time_window_decision(TimeWindowTask::LibraryScan, &missing, "/music", 1_000),
            DueDecision::Run(DueReason::NoCleanExit)
        );

        let clean = SessionState {
            clean_exit: Some(CleanExit {
                completed_at: 1_000,
                library_root: "/music".into(),
            }),
            ..SessionState::default()
        };
        assert_eq!(
            time_window_decision(TimeWindowTask::LibraryScan, &clean, "/music", 1_900),
            DueDecision::Run(DueReason::CleanExitTooOld { age_seconds: 900 })
        );
        assert_eq!(
            time_window_decision(TimeWindowTask::LibraryScan, &clean, "/new-music", 1_100),
            DueDecision::Run(DueReason::LibraryRootChanged)
        );
        assert_eq!(
            time_window_decision(TimeWindowTask::LibraryScan, &clean, "/music", 999),
            DueDecision::Run(DueReason::ClockMovedBackwards)
        );
    }
}
