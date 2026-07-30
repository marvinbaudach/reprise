//! Public result/outcome/progress types for the scanner, split out of
//! `scanner.rs` to keep it under the 800-line cap. Re-exported from `scanner`
//! (`pub use scanner_types::*`) so callers keep referring to `scanner::
//! ScanReport`, `scanner::ScanOutcome`, etc.

use crate::db::Db;

use crate::models::ImportErrorKind;

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("database error: {0}")]
    Db(#[from] crate::db::DbError),
    #[error("Sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Task 1.7: replaces the old bare `Tags(String)` — classified at the
    /// source, see `import_errors`'s module doc comment.
    #[error("import error ({kind:?}): {detail}")]
    Import {
        kind: ImportErrorKind,
        detail: String,
    },
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("relink target {track_id} is no longer an active missing track")]
    RelinkTargetChanged { track_id: i64 },
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub added: u32,
    pub updated: u32,
    pub skipped_unchanged: u32,
    /// Files deliberately removed from the catalog and matched by stable
    /// filesystem identity (or an exact-path fallback) before tag parsing.
    pub excluded: u32,
    pub errors: u32,
    /// Stage 2 Task 8: files recognized as relocated (same `(device, inode)`
    /// or, failing that, an unambiguous tag+size fingerprint match against a
    /// row whose old path is gone) rather than treated as new. A moved file
    /// counts here, not in `added`.
    pub moved: u32,
    /// Task 1.5: count of previously-present tracks under this scan's root
    /// newly marked missing by this same scan's folded-in reconcile pass —
    /// see the module's `## Fold: scan IS reconcile` doc section. An
    /// already-missing row is not recounted. Always `0` when the scan
    /// returns [`ScanOutcome::RootUnavailable`] instead of wrapping this
    /// report in [`ScanOutcome::Completed`], since that outcome means the
    /// mark phase never ran at all.
    pub vanished: u32,
    /// Task 1.9: count of `import_errors` rows deleted by a pass-1 import
    /// success this same scan — i.e. `import_errors::clear_error` returned
    /// `true` for a path whose read actually produced real tags. This is
    /// the end-of-scan toast's "N import errors fixed themselves" number.
    /// Deliberately narrower than every `clear_error` call this module
    /// makes: a pass-2 (untagged) rescue calls `record_error`, not `clear_
    /// error`, on purpose (see `scan_folder_inner`'s `## Hint coexistence`
    /// doc section) — that row survives as a hint, so nothing healed, and
    /// it never reaches this counter. `moved` (above) is a related but
    /// distinct signal — a track can move without ever having had an error,
    /// and a healed error's file need not have moved — so the two counters
    /// are incremented independently and may both apply to the same file.
    pub healed: u32,
}

/// What a `scan_folder`/`scan_folder_with_progress` call concluded — Task
/// 1.5 replaced the bare `ScanReport` return with this two-variant outcome
/// so a scan can distinguish "I walked `root` and reconciled it" from "I
/// have no evidence about `root` at all" without silently reporting the
/// latter as a suspiciously-empty former. See the module's `## Root guard`
/// doc section on `scan_folder_inner` for exactly when [`RootUnavailable`]
/// fires and why marking nothing beats marking every track "unmounted".
///
/// [`RootUnavailable`]: ScanOutcome::RootUnavailable
#[derive(Debug)]
pub enum ScanOutcome {
    /// The walk ran (even if it found nothing) and, unless the root guard
    /// tripped, the vanish-mark phase ran too, in the same transaction as
    /// the walk's own upserts.
    Completed(ScanReport),
    /// Nothing was written — not even an "unmounted" mark — because the
    /// root guard tripped: see `scan_folder_inner`'s doc comment.
    RootUnavailable { root: std::path::PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanProgress {
    Discovering,
    Scanning {
        processed: u64,
        total: u64,
        current_path: std::path::PathBuf,
    },
    Fetching {
        done: u64,
        total: u64,
    },
}

/// Summary passed to the UI after a scan finishes, for the completion toast.
#[derive(Debug, Clone, Copy)]
pub struct ScanResult {
    pub new_tracks: u32,
    pub failed: u32,
}

impl ScanReport {
    pub fn to_scan_result(&self) -> ScanResult {
        ScanResult {
            new_tracks: self.added,
            failed: self.errors,
        }
    }
}

/// Runs the stateful work that belongs after—and only after—a completed
/// scan, regardless of whether the scan was explicit or watcher-triggered.
/// Keeping this next to [`ScanOutcome`] prevents a second scan entry point
/// from forgetting the `last_scan_relinked` update or running destructive
/// auto-clean after `RootUnavailable`.
pub fn finalize_completed_scan(
    db: &Db,
    report: &ScanReport,
    now: i64,
) -> Result<Vec<i64>, ScanError> {
    let conn = db.conn();
    crate::library::settings::set_last_scan_relinked_in(conn, report.moved)?;
    Ok(crate::queries::run_auto_clean(db, now)?)
}
