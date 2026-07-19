//! Isolated Library Doctor workers.
//!
//! Every worker opens its own database connection and cooperatively checks
//! cancellation between tracks. GTK-owned state never crosses the thread
//! boundary.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use reprise_core::fingerprint::FingerprintBackend;
use reprise_core::library_doctor::{
    DoctorApplyPlan, DoctorScanOutcome, DoctorScanProgress, DoctorScanRequest, DoctorWriteControl,
    DoctorWriteProgress, DoctorWriteReport, LibraryDoctor, ScanControl,
};

pub(super) fn run_scan(
    db_path: &Path,
    request: &DoctorScanRequest,
    fingerprint: &dyn FingerprintBackend,
    cancellation: &AtomicBool,
    publish: &mut dyn FnMut(DoctorScanProgress),
) -> Result<DoctorScanOutcome, String> {
    let mut conn =
        reprise_core::db::open_migrated(Some(db_path)).map_err(|error| error.to_string())?;
    LibraryDoctor::new(&mut conn)
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

pub(super) fn run_apply(
    db_path: &Path,
    plan: &DoctorApplyPlan,
    cancellation: &AtomicBool,
    publish: &mut dyn FnMut(DoctorWriteProgress),
) -> Result<Option<DoctorWriteReport>, String> {
    let mut conn =
        reprise_core::db::open_migrated(Some(db_path)).map_err(|error| error.to_string())?;
    LibraryDoctor::new(&mut conn)
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
) -> Result<Option<DoctorWriteReport>, String> {
    let mut conn =
        reprise_core::db::open_migrated(Some(db_path)).map_err(|error| error.to_string())?;
    LibraryDoctor::new(&mut conn)
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
