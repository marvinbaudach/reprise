//! Isolated Library Doctor workers.
//!
//! Every worker opens its own database connection and cooperatively checks
//! cancellation between tracks. GTK-owned state never crosses the thread
//! boundary.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use reprise_core::fingerprint::FingerprintBackend;
use reprise_core::library_doctor::{
    DoctorApplyPlan, DoctorCleanupReport, DoctorError, DoctorScan, DoctorScanOutcome,
    DoctorScanProgress, DoctorScanRequest, DoctorWriteControl, DoctorWriteProgress,
    DoctorWriteReport, LibraryDoctor, ScanControl,
};

/// A worker failure, split into the one distinction the surface acts on.
///
/// `busy` means another tag-writing job holds the slot: the user waits and
/// tries again. Everything else is ours to report and theirs to do nothing
/// about — so `detail` goes to the log and never into a toast, where a raw
/// `rusqlite` sentence would be both untranslated and useless.
pub(super) struct JobFailure {
    pub(super) busy: bool,
    pub(super) detail: String,
}

impl JobFailure {
    pub(super) fn user_message(&self) -> String {
        if self.busy {
            crate::ui::strings::text(crate::ui::strings::TAG_WRITE_BUSY)
        } else {
            crate::ui::strings::text(crate::ui::strings::DOCTOR_JOB_FAILED)
        }
    }
}

impl From<DoctorError> for JobFailure {
    fn from(error: DoctorError) -> Self {
        Self {
            busy: matches!(error, DoctorError::TagWriteBusy(_)),
            detail: error.to_string(),
        }
    }
}

impl From<reprise_core::db::DbError> for JobFailure {
    fn from(error: reprise_core::db::DbError) -> Self {
        Self {
            busy: false,
            detail: error.to_string(),
        }
    }
}

pub(super) fn run_scan(
    db_path: &Path,
    request: &DoctorScanRequest,
    fingerprint: &dyn FingerprintBackend,
    cancellation: &AtomicBool,
    publish: &mut dyn FnMut(DoctorScanProgress),
) -> Result<DoctorScanOutcome, String> {
    let conn =
        reprise_core::db::Db::open_migrated(Some(db_path)).map_err(|error| error.to_string())?;
    LibraryDoctor::new(&conn)
        .scan(request, Some(fingerprint), |progress| {
            publish(progress);
            if cancellation.load(Ordering::Relaxed) {
                ScanControl::Cancel
            } else {
                ScanControl::Continue
            }
        })
        .map_err(|error| error.to_string())
}

pub(super) fn run_auto_apply(
    db_path: &Path,
    scan: &DoctorScan,
    cancellation: &AtomicBool,
    publish: &mut dyn FnMut(DoctorWriteProgress),
) -> Result<Option<DoctorWriteReport>, JobFailure> {
    let conn = reprise_core::db::Db::open_migrated(Some(db_path))?;
    LibraryDoctor::new(&conn)
        .apply_auto_tier(scan, |progress| {
            publish(progress);
            if cancellation.load(Ordering::Relaxed) {
                DoctorWriteControl::Cancel
            } else {
                DoctorWriteControl::Continue
            }
        })
        .map_err(JobFailure::from)
}

pub(super) fn run_apply(
    db_path: &Path,
    plan: &DoctorApplyPlan,
    cancellation: &AtomicBool,
    publish: &mut dyn FnMut(DoctorWriteProgress),
) -> Result<Option<DoctorWriteReport>, String> {
    let conn =
        reprise_core::db::Db::open_migrated(Some(db_path)).map_err(|error| error.to_string())?;
    LibraryDoctor::new(&conn)
        .apply_review_plan(plan, |progress| {
            publish(progress);
            if cancellation.load(Ordering::Relaxed) {
                DoctorWriteControl::Cancel
            } else {
                DoctorWriteControl::Continue
            }
        })
        .map(Some)
        .map_err(|error| error.to_string())
}

pub(super) fn run_revert(
    db_path: &Path,
    cancellation: &AtomicBool,
    publish: &mut dyn FnMut(DoctorWriteProgress),
) -> Result<Option<DoctorCleanupReport>, String> {
    let conn =
        reprise_core::db::Db::open_migrated(Some(db_path)).map_err(|error| error.to_string())?;
    LibraryDoctor::new(&conn)
        .revert_last_cleanup(|progress| {
            publish(progress);
            if cancellation.load(Ordering::Relaxed) {
                DoctorWriteControl::Cancel
            } else {
                DoctorWriteControl::Continue
            }
        })
        .map_err(|error| error.to_string())
}
