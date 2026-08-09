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
