//! Saying that a track is being fingerprinted.
//!
//! Fingerprinting decodes the audio and is by far the most expensive step of a
//! scan. Observed on a real run: the bar stood at "60/61 tracks" for more than
//! a minute on the one track that needed it, which reads as a crash.
//!
//! The scan cannot see that step from the outside — it happens inside
//! `RemoteResolver::resolve_track`, several layers down. So the announcement
//! comes from the one thing that is reached only when a track really is
//! fingerprinted: the backend the scan hands down, wrapped on the way. That
//! keeps the conditions for fingerprinting exactly where they are — this
//! module decides nothing about when, only that it is said.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::fingerprint::{
    FingerprintBackend, FingerprintCapability, FingerprintControl, FingerprintError,
    FingerprintOutcome, FingerprintProgress,
};

use super::DoctorScanPhase;

/// Raises `running` for as long as the wrapped backend is working.
///
/// A flag rather than a callback, because `FingerprintBackend` is `Send +
/// Sync`: an `&AtomicBool` crosses that bound, a progress closure does not.
/// The scan reads the flag whenever it publishes progress, and the backend
/// calls back often enough for that to be seen — the GStreamer one reports
/// immediately and then every 50 ms.
pub(super) struct AnnouncedFingerprintBackend<'scan> {
    inner: &'scan dyn FingerprintBackend,
    running: &'scan AtomicBool,
}

impl<'scan> AnnouncedFingerprintBackend<'scan> {
    pub(super) const fn new(
        inner: &'scan dyn FingerprintBackend,
        running: &'scan AtomicBool,
    ) -> Self {
        Self { inner, running }
    }
}

impl FingerprintBackend for AnnouncedFingerprintBackend<'_> {
    fn capability(&self) -> FingerprintCapability {
        self.inner.capability()
    }

    fn fingerprint(
        &self,
        path: &Path,
        progress: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
    ) -> Result<FingerprintOutcome, FingerprintError> {
        self.running.store(true, Ordering::Relaxed);
        let outcome = self.inner.fingerprint(path, progress);
        self.running.store(false, Ordering::Relaxed);
        outcome
    }
}

/// Which half of the network pass the scan is in right now.
pub(super) fn remote_phase(fingerprinting: &AtomicBool) -> DoctorScanPhase {
    if fingerprinting.load(Ordering::Relaxed) {
        DoctorScanPhase::Fingerprinting
    } else {
        DoctorScanPhase::CheckingRemote
    }
}

#[cfg(test)]
mod tests {
    use super::{remote_phase, AnnouncedFingerprintBackend};
    use crate::fingerprint::{
        FingerprintBackend, FingerprintCapability, FingerprintControl, FingerprintError,
        FingerprintOutcome, FingerprintProgress,
    };
    use crate::library::library_doctor::DoctorScanPhase;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    struct FlagReadingBackend<'flag> {
        seen_while_running: &'flag AtomicBool,
        flag: &'flag AtomicBool,
    }

    impl FingerprintBackend for FlagReadingBackend<'_> {
        fn capability(&self) -> FingerprintCapability {
            FingerprintCapability::Available {
                cache_namespace: "test".into(),
            }
        }

        fn fingerprint(
            &self,
            _: &Path,
            _: &mut dyn FnMut(FingerprintProgress) -> FingerprintControl,
        ) -> Result<FingerprintOutcome, FingerprintError> {
            self.seen_while_running.store(
                remote_phase(self.flag) == DoctorScanPhase::Fingerprinting,
                std::sync::atomic::Ordering::Relaxed,
            );
            Err(FingerprintError::DurationUnavailable)
        }
    }

    /// The flag has to be down again even when the backend fails, or every
    /// later lookup would keep claiming to fingerprint.
    #[test]
    fn doc_1g_the_flag_stands_only_for_the_duration_of_the_fingerprint() {
        let flag = AtomicBool::new(false);
        let seen_while_running = AtomicBool::new(false);
        let inner = FlagReadingBackend {
            seen_while_running: &seen_while_running,
            flag: &flag,
        };
        let announced = AnnouncedFingerprintBackend::new(&inner, &flag);

        assert_eq!(remote_phase(&flag), DoctorScanPhase::CheckingRemote);
        assert!(announced
            .fingerprint(Path::new("track.flac"), &mut |_| {
                FingerprintControl::Continue
            })
            .is_err());
        assert!(seen_while_running.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(remote_phase(&flag), DoctorScanPhase::CheckingRemote);
        assert!(matches!(
            announced.capability(),
            FingerprintCapability::Available { .. }
        ));
    }
}
