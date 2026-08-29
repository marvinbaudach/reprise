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
const LYRICS_WATERMARK_KEY: &str = "startup_tasks.lyrics_watermark";
const LYRICS_FULL_SWEEP_KEY: &str = "startup_tasks.lyrics_full_sweep";
pub const STARTUP_SCAN_WINDOW_SECONDS: i64 = 15 * 60;
pub const LYRICS_FULL_SWEEP_INTERVAL_SECONDS: i64 = 30 * 24 * 60 * 60;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricsScope {
    Everything,
    AddedSince(i64),
}

pub fn lyrics_scope(watermark: Option<i64>, last_full_sweep: Option<i64>, now: i64) -> LyricsScope {
    let Some(watermark) = watermark else {
        return LyricsScope::Everything;
    };
    let Some(last_full_sweep) = last_full_sweep else {
        return LyricsScope::Everything;
    };
    let Some(age) = now.checked_sub(last_full_sweep) else {
        return LyricsScope::Everything;
    };
    if !(0..LYRICS_FULL_SWEEP_INTERVAL_SECONDS).contains(&age) {
        LyricsScope::Everything
    } else {
        LyricsScope::AddedSince(watermark)
    }
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
enum PassKind {
    Startup,
    UserTriggered,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LyricsPass {
    started_at: i64,
    scope: LyricsScope,
}

impl LyricsPass {
    pub fn scope(&self) -> LyricsScope {
        self.scope
    }

    pub fn record_completed_or_warn(self, db: &Db) {
        if let Err(error) = set_internal_setting_in(
            db.conn(),
            LYRICS_WATERMARK_KEY,
            &self.started_at.to_string(),
        ) {
            tracing::warn!(%error, "could not persist lyrics startup-task completion");
        }
    }
}

pub fn begin_lyrics_pass(db: &Db, scope: LyricsScope) -> LyricsPass {
    let started_at = now_unix();
    if scope == LyricsScope::Everything {
        if let Err(error) =
            set_internal_setting_in(db.conn(), LYRICS_FULL_SWEEP_KEY, &started_at.to_string())
        {
            tracing::warn!(%error, "could not persist lyrics full-sweep attempt");
        }
    }
    LyricsPass { started_at, scope }
}

pub fn lyrics_watermark(db: &Db) -> Option<i64> {
    lyrics_timestamp(db, LYRICS_WATERMARK_KEY, "watermark")
}

pub fn lyrics_last_full_sweep(db: &Db) -> Option<i64> {
    lyrics_timestamp(db, LYRICS_FULL_SWEEP_KEY, "full sweep")
}

fn lyrics_timestamp(db: &Db, key: &str, timestamp: &str) -> Option<i64> {
    match crate::library::settings::get_setting_in(db.conn(), key) {
        Ok(Some(value)) => match value.parse() {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(
                    timestamp,
                    value,
                    %error,
                    "invalid lyrics startup-task timestamp; running conservatively"
                );
                None
            }
        },
        Ok(None) => {
            tracing::warn!(
                timestamp,
                "lyrics startup-task timestamp is absent; running conservatively"
            );
            None
        }
        Err(error) => {
            tracing::warn!(
                timestamp,
                %error,
                "could not read lyrics startup-task timestamp; running conservatively"
            );
            None
        }
    }
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
            signature: capture_signature(db, task, PassKind::Startup),
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

fn capture_signature(
    db: &Db,
    task: SignatureTask,
    pass_kind: PassKind,
) -> Option<LibrarySignature> {
    match current_signature_in(db.conn()) {
        Ok(signature) => Some(signature),
        Err(error) => {
            match pass_kind {
                PassKind::Startup => {
                    tracing::warn!(
                        task = task.log_name(),
                        %error,
                        "could not capture startup task signature; running conservatively"
                    );
                }
                PassKind::UserTriggered => {
                    tracing::warn!(
                        task = task.log_name(),
                        %error,
                        "could not capture user-triggered task signature; running conservatively"
                    );
                }
            }
            None
        }
    }
}

/// Starts work requested directly by the user without consulting startup
/// freshness, while retaining the exact signature the completed pass may
/// settle for the next launch.
pub fn begin_user_triggered(db: &Db, task: SignatureTask) -> ExactTaskPass {
    ExactTaskPass {
        task,
        signature: capture_signature(db, task, PassKind::UserTriggered),
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

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

#[cfg(test)]
#[path = "startup_tasks_lyrics_tests.rs"]
mod lyrics_tests;

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
        advance_library_signature_in(db.conn()).unwrap();
        assert!(begin_exact(&db, SignatureTask::CoverDownload).is_some());

        let pass = begin_user_triggered(&db, SignatureTask::CoverDownload);
        pass.record_completed_or_warn(&db);

        assert!(begin_exact(&db, SignatureTask::CoverDownload).is_none());
    }

    #[test]
    fn signature_capture_failure_keeps_each_request_kind_in_the_log() {
        let db = crate::db::Db::open_in_memory().unwrap();
        crate::library::settings::set_setting(&db, LIBRARY_SIGNATURE_KEY, "not-a-revision")
            .unwrap();
        let startup_logs = crate::log_capture::CapturedLogs::default();
        let user_logs = crate::log_capture::CapturedLogs::default();

        assert!(startup_logs
            .capture(|| capture_signature(&db, SignatureTask::CoverDownload, PassKind::Startup))
            .is_none());
        assert!(user_logs
            .capture(|| {
                capture_signature(&db, SignatureTask::CoverDownload, PassKind::UserTriggered)
            })
            .is_none());

        assert!(startup_logs
            .joined()
            .contains("could not capture startup task signature"));
        assert!(user_logs
            .joined()
            .contains("could not capture user-triggered task signature"));
    }

    #[test]
    fn lyrics_timestamps_are_absent_until_a_pass_writes_them() {
        let db = crate::db::Db::open_in_memory().unwrap();

        assert_eq!(lyrics_watermark(&db), None);
        assert_eq!(lyrics_last_full_sweep(&db), None);
    }

    #[test]
    fn a_completed_full_lyrics_pass_round_trips_both_timestamps() {
        let db = crate::db::Db::open_in_memory().unwrap();

        let pass = begin_lyrics_pass(&db, LyricsScope::Everything);
        let started_at = pass.started_at;
        pass.record_completed_or_warn(&db);

        assert_eq!(lyrics_last_full_sweep(&db), Some(started_at));
        assert_eq!(lyrics_watermark(&db), Some(started_at));
    }

    #[test]
    fn an_unparsable_lyrics_timestamp_warns_and_runs_conservatively() {
        let db = crate::db::Db::open_in_memory().unwrap();
        set_internal_setting_in(db.conn(), LYRICS_WATERMARK_KEY, "not-a-timestamp").unwrap();
        let logs = crate::log_capture::CapturedLogs::default();

        assert_eq!(logs.capture(|| lyrics_watermark(&db)), None);

        let logs = logs.joined();
        assert!(logs.contains("invalid lyrics startup-task timestamp"));
        assert!(logs.contains("not-a-timestamp"));
    }

    #[test]
    fn only_a_full_lyrics_pass_updates_the_full_sweep_attempt_clock() {
        let db = crate::db::Db::open_in_memory().unwrap();

        let full = begin_lyrics_pass(&db, LyricsScope::Everything);
        let full_started_at = full.started_at;
        assert_eq!(lyrics_last_full_sweep(&db), Some(full_started_at));

        let narrow = begin_lyrics_pass(&db, LyricsScope::AddedSince(123));
        assert_eq!(narrow.scope(), LyricsScope::AddedSince(123));
        assert_eq!(lyrics_last_full_sweep(&db), Some(full_started_at));
    }

    #[test]
    fn lyrics_timestamp_writes_do_not_disturb_signature_task_records() {
        let db = crate::db::Db::open_in_memory().unwrap();
        record_completed_at(&db, SignatureTask::CoverDownload, 321).unwrap();

        let pass = begin_lyrics_pass(&db, LyricsScope::Everything);
        pass.record_completed_or_warn(&db);

        assert_eq!(
            exact_signature_decision(&db, SignatureTask::CoverDownload).unwrap(),
            DueDecision::Skip(DueReason::LibrarySignatureUnchanged {
                completed_at: 321,
                signature: LibrarySignature(0),
            })
        );
    }

    #[test]
    fn lyrics_scope_is_full_until_coverage_is_known() {
        assert_eq!(
            lyrics_scope(None, Some(9_999), 10_000),
            LyricsScope::Everything
        );
    }

    #[test]
    fn lyrics_scope_is_full_without_a_recent_full_sweep() {
        let interval = LYRICS_FULL_SWEEP_INTERVAL_SECONDS;

        assert_eq!(
            lyrics_scope(Some(123), None, 10_000),
            LyricsScope::Everything
        );
        assert_eq!(
            lyrics_scope(Some(123), Some(10_000 - interval), 10_000),
            LyricsScope::Everything
        );
        assert_eq!(
            lyrics_scope(Some(123), Some(10_001), 10_000),
            LyricsScope::Everything
        );
    }

    #[test]
    fn lyrics_scope_uses_the_completed_watermark_during_the_full_sweep_window() {
        let interval = LYRICS_FULL_SWEEP_INTERVAL_SECONDS;

        assert_eq!(
            lyrics_scope(Some(123), Some(10_000 - interval + 1), 10_000),
            LyricsScope::AddedSince(123)
        );
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
