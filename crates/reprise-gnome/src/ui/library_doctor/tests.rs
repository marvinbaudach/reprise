use std::path::Path;
use std::sync::atomic::AtomicBool;

use reprise_core::fingerprint::{
    FingerprintBackend, FingerprintCapability, FingerprintControl, FingerprintError,
    FingerprintOutcome, FingerprintProgress,
};
use reprise_core::library_doctor::{DoctorScopeRequest, DoctorViewSnapshot};
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

struct NeverFingerprint;

impl FingerprintBackend for NeverFingerprint {
    fn capability(&self) -> FingerprintCapability {
        FingerprintCapability::MissingPlugin {
            elements: vec!["chromaprint".into()],
        }
    }

    fn fingerprint(
        &self,
        _path: &Path,
        _progress: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
    ) -> Result<FingerprintOutcome, FingerprintError> {
        panic!("a local-only empty scan must not fingerprint")
    }
}

fn snapshot() -> DoctorViewSnapshot {
    DoctorViewSnapshot {
        source: ViewSource::Library,
        sort_field: "artist".into(),
        sort_dir: "asc".into(),
        filter: String::new(),
        browse: BrowseFilter::default(),
        queue_ids: Vec::new(),
    }
}

#[test]
fn doc_2a_scope_choice_freezes_the_requested_input_shape() {
    assert!(matches!(
        super::scope_request(0, snapshot(), vec![7]),
        DoctorScopeRequest::WholeLibrary
    ));
    assert!(matches!(
        super::scope_request(1, snapshot(), vec![7]),
        DoctorScopeRequest::CurrentView(_)
    ));
    assert_eq!(
        super::scope_request(2, snapshot(), vec![7, 8]),
        DoctorScopeRequest::Selection {
            track_ids: vec![7, 8]
        }
    );
}

#[test]
fn doc_7b_entry_scope_defaults_to_library_and_suggests_filtered_view() {
    assert_eq!(super::suggested_scope(&snapshot()), 0);

    let mut filtered = snapshot();
    filtered.filter = "needle".into();
    assert_eq!(super::suggested_scope(&filtered), 1);

    let mut browsed = snapshot();
    browsed.browse.genre = Some("Jazz".into());
    assert_eq!(super::suggested_scope(&browsed), 1);
}

#[test]
fn doctor_worker_uses_only_its_isolated_database_connection() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("doctor.db");
    drop(reprise_core::db::open_migrated(Some(&database)).unwrap());
    let request = reprise_core::library_doctor::DoctorScanRequest {
        scope: DoctorScopeRequest::WholeLibrary,
        options: reprise_core::library_doctor::DoctorScanOptions::local_only(),
    };
    let mut progress = Vec::new();

    let outcome = super::run_scan(
        &database,
        &request,
        &NeverFingerprint,
        &AtomicBool::new(false),
        &mut |item| progress.push(item),
    )
    .unwrap();

    let reprise_core::library_doctor::DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("empty local scan must complete")
    };
    assert_eq!(scan.checked_tracks, 0);
    assert!(progress.is_empty());
}

#[test]
fn late_scan_progress_is_rejected_after_finish_or_new_generation() {
    assert!(super::accepts_scan_progress(
        4,
        4,
        true,
        Some(super::DoctorJobKind::Scan)
    ));
    assert!(!super::accepts_scan_progress(
        5,
        4,
        true,
        Some(super::DoctorJobKind::Scan)
    ));
    assert!(!super::accepts_scan_progress(4, 4, false, None));
}
