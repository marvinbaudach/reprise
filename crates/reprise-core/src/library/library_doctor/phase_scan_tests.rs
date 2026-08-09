use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use super::*;

struct PhaseOrderResolver {
    local_complete: Rc<Cell<bool>>,
    first_remote_saw_local_complete: Rc<Cell<bool>>,
}

impl RemoteResolver for PhaseOrderResolver {
    fn resolve_album(
        &mut self,
        _: &super::remote::AlbumRequest,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> Result<super::remote::AlbumResolution, RemoteProviderError> {
        self.first_remote_saw_local_complete
            .set(self.local_complete.get());
        Ok(super::remote::AlbumResolution::default())
    }

    fn resolve_track(
        &mut self,
        _: &RemoteTrackMetadata,
        _: &Path,
        _: Option<&dyn FingerprintBackend>,
        _: Option<&super::remote::AlbumMatch>,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> Result<RemoteResolution, RemoteProviderError> {
        Ok(RemoteResolution::default())
    }
}

/// Fingerprints without decoding anything, and reports once while it does —
/// the shape of the real backend, which calls back every 50 ms.
struct SilentFingerprintBackend;

impl FingerprintBackend for SilentFingerprintBackend {
    fn capability(&self) -> crate::fingerprint::FingerprintCapability {
        crate::fingerprint::FingerprintCapability::Available {
            cache_namespace: "test".into(),
        }
    }

    fn fingerprint(
        &self,
        _: &Path,
        progress: &mut dyn FnMut(
            crate::fingerprint::FingerprintProgress,
        ) -> crate::fingerprint::FingerprintControl,
    ) -> Result<crate::fingerprint::FingerprintOutcome, crate::fingerprint::FingerprintError> {
        progress(crate::fingerprint::FingerprintProgress {
            processed_seconds: 0,
            duration_seconds: Some(1),
        });
        Ok(crate::fingerprint::FingerprintOutcome::Completed(
            crate::fingerprint::Fingerprint {
                encoded: "AQAAAA".into(),
                duration_seconds: 1,
                cache_namespace: "test".into(),
            },
        ))
    }
}

/// Fingerprints every track, and asks once more afterwards so the phase the
/// scan reports after the expensive step is visible too.
struct FingerprintingResolver;

impl RemoteResolver for FingerprintingResolver {
    fn resolve_track(
        &mut self,
        _: &RemoteTrackMetadata,
        path: &Path,
        fingerprint_backend: Option<&dyn FingerprintBackend>,
        _: Option<&super::remote::AlbumMatch>,
        control: &mut dyn FnMut() -> ScanControl,
    ) -> Result<RemoteResolution, RemoteProviderError> {
        let backend = fingerprint_backend.expect("the scan must hand a fingerprint backend down");
        backend
            .fingerprint(path, &mut |_| {
                if control() == ScanControl::Cancel {
                    crate::fingerprint::FingerprintControl::Cancel
                } else {
                    crate::fingerprint::FingerprintControl::Continue
                }
            })
            .unwrap();
        let _ = control();
        Ok(RemoteResolution::default())
    }
}

/// Fingerprinting decodes the audio and is the most expensive step of a scan:
/// one track held the bar at "60/61 tracks" for over a minute, which reads as
/// a hang. The phase has to say what is happening — without moving the
/// counter, which only ever goes forward.
#[test]
fn doc_1g_a_fingerprinted_track_says_so_while_it_runs() {
    let dir = tempfile::tempdir().unwrap();
    let conn = migrated_connection();
    let path = fixture_copy(dir.path(), "fingerprint-phase.flac");
    write_tags(&path, "Title", "Artist", "Album", "Artist", "Rock");
    insert_track(&conn, 1, &path, "Artist");
    let mut reported = Vec::new();

    LibraryDoctor::new(&conn)
        .scan_with_resolver(
            &DoctorScanRequest {
                scope: DoctorScopeRequest::Selection { track_ids: vec![1] },
                options: DoctorScanOptions {
                    remote_enabled: true,
                },
            },
            Some(&SilentFingerprintBackend),
            &mut FingerprintingResolver,
            &mut |progress| {
                reported.push(progress);
                ScanControl::Continue
            },
        )
        .unwrap();

    assert!(
        reported
            .iter()
            .any(|item| item.phase == DoctorScanPhase::Fingerprinting),
        "the fingerprinted track must be announced: {:?}",
        reported.iter().map(|item| item.phase).collect::<Vec<_>>()
    );
    assert!(
        reported
            .windows(2)
            .all(|pair| pair[0].completed_tracks <= pair[1].completed_tracks),
        "the counter may not go backwards while the phase changes"
    );
    assert_eq!(
        reported.last().map(|item| item.phase),
        Some(DoctorScanPhase::CheckingRemote),
        "the phase has to fall back once the fingerprint is done"
    );
}

fn track_refs(dir: &Path, count: i64) -> Vec<DoctorTrackRef> {
    (1..=count)
        .map(|id| {
            let path = super::fixture_copy(dir, &format!("cancel-{id:02}.flac"));
            super::write_tags(&path, "Title", "Artist", "Album", "Artist", "Rock");
            DoctorTrackRef {
                track_id: id,
                path,
                file_mtime: 0,
                file_size: 0,
                device: None,
                inode: None,
            }
        })
        .collect()
}

#[test]
fn doc_1g_the_reading_pass_stops_for_a_cancelled_scan() {
    let dir = tempfile::tempdir().unwrap();
    let tracks = track_refs(dir.path(), 8);

    let cancelled = super::super::scan::read_tracks_parallel(&tracks, &mut || ScanControl::Cancel);
    let completed =
        super::super::scan::read_tracks_parallel(&tracks, &mut || ScanControl::Continue);

    assert!(cancelled.cancelled);
    assert!(
        cancelled.reads.is_empty(),
        "a scan cancelled before the pass starts must not read a single file, read {}",
        cancelled.reads.len()
    );
    assert!(!completed.cancelled);
    assert_eq!(completed.reads.len(), tracks.len());
}

#[test]
fn doc_1g_the_local_pass_completes_before_the_first_network_request() {
    let dir = tempfile::tempdir().unwrap();
    let conn = migrated_connection();
    for id in 1..=2 {
        let path = fixture_copy(dir.path(), &format!("phase-order-{id}.flac"));
        write_tags(&path, "Title", "Artist", "Album", "Artist", "Rock");
        insert_track(&conn, id, &path, "Artist");
    }
    let local_complete = Rc::new(Cell::new(false));
    let first_remote_saw_local_complete = Rc::new(Cell::new(false));
    let mut resolver = PhaseOrderResolver {
        local_complete: local_complete.clone(),
        first_remote_saw_local_complete: first_remote_saw_local_complete.clone(),
    };

    LibraryDoctor::new(&conn)
        .scan_with_resolver(
            &DoctorScanRequest {
                scope: DoctorScopeRequest::Selection {
                    track_ids: vec![1, 2],
                },
                options: DoctorScanOptions {
                    remote_enabled: true,
                },
            },
            None,
            &mut resolver,
            &mut |progress| {
                if progress.phase == DoctorScanPhase::CheckingRemote {
                    local_complete.set(true);
                }
                ScanControl::Continue
            },
        )
        .unwrap();

    assert!(first_remote_saw_local_complete.get());
}
