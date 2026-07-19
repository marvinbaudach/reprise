use std::path::Path;

use crate::fingerprint::{
    Fingerprint, FingerprintBackend, FingerprintCapability, FingerprintControl, FingerprintError,
    FingerprintOutcome, FingerprintProgress, GST_CHROMAPRINT_PIPELINE_REVISION,
};

struct FakeBackend;

impl FingerprintBackend for FakeBackend {
    fn capability(&self) -> FingerprintCapability {
        FingerprintCapability::Available {
            cache_namespace: "fake-v1".into(),
        }
    }

    fn fingerprint(
        &self,
        _path: &Path,
        progress: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
    ) -> Result<FingerprintOutcome, FingerprintError> {
        if progress(FingerprintProgress {
            processed_seconds: 1,
            duration_seconds: Some(13),
        }) == FingerprintControl::Cancel
        {
            return Ok(FingerprintOutcome::Cancelled);
        }
        Ok(FingerprintOutcome::Completed(Fingerprint {
            encoded: "stable".into(),
            duration_seconds: 13,
            cache_namespace: "fake-v1".into(),
        }))
    }
}

fn assert_backend_is_send_sync<T: Send + Sync>() {}

#[test]
fn fingerprint_backend_is_send_sync_and_preserves_completed_metadata() {
    assert_backend_is_send_sync::<FakeBackend>();
    let backend: &dyn FingerprintBackend = &FakeBackend;
    assert_eq!(
        backend.capability(),
        FingerprintCapability::Available {
            cache_namespace: "fake-v1".into()
        }
    );

    let result = backend
        .fingerprint(Path::new("not-read-by-fake"), &mut |_| {
            FingerprintControl::Continue
        })
        .unwrap();
    assert_eq!(
        result,
        FingerprintOutcome::Completed(Fingerprint {
            encoded: "stable".into(),
            duration_seconds: 13,
            cache_namespace: "fake-v1".into(),
        })
    );
}

#[test]
fn cancellation_is_a_successful_typed_outcome() {
    let outcome = FakeBackend
        .fingerprint(Path::new("not-read-by-fake"), &mut |_| {
            FingerprintControl::Cancel
        })
        .unwrap();
    assert_eq!(outcome, FingerprintOutcome::Cancelled);
}

#[test]
fn unavailable_capabilities_preserve_actionable_details() {
    let missing = FingerprintCapability::MissingPlugin {
        elements: vec!["chromaprint".into(), "audiobuffersplit".into()],
    };
    let failed = FingerprintCapability::BackendInitFailed {
        detail: "registry unavailable".into(),
    };

    assert_eq!(
        missing,
        FingerprintCapability::MissingPlugin {
            elements: vec!["chromaprint".into(), "audiobuffersplit".into()]
        }
    );
    assert_eq!(
        failed,
        FingerprintCapability::BackendInitFailed {
            detail: "registry unavailable".into()
        }
    );
}

#[test]
fn pipeline_revision_is_stable_without_claiming_a_runtime_library_version() {
    assert_eq!(GST_CHROMAPRINT_PIPELINE_REVISION, "pipeline-v1");
}
