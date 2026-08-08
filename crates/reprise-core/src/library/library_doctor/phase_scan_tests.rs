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
