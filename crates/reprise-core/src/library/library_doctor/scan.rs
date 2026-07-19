use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use super::local_rules::{self, ReadTrack};
use super::scope;
use super::{
    DoctorError, DoctorScanOptions, DoctorScanOutcome, DoctorScanProgress, DoctorScopeRequest,
    DoctorTrackSnapshot, FrozenScope, LocalScanRequest, ScanControl,
};

pub struct LibraryDoctor<'connection> {
    pub(super) conn: &'connection mut Connection,
}

impl<'connection> LibraryDoctor<'connection> {
    pub fn new(conn: &'connection mut Connection) -> Self {
        Self { conn }
    }

    pub fn freeze_scope(
        &mut self,
        request: &DoctorScopeRequest,
    ) -> Result<FrozenScope, DoctorError> {
        scope::freeze_scope(self.conn, request).map_err(DoctorError::from)
    }

    pub fn scan_local(
        &mut self,
        request: &LocalScanRequest,
        mut progress: impl FnMut(DoctorScanProgress) -> ScanControl,
    ) -> Result<DoctorScanOutcome, DoctorError> {
        let FrozenScope::Tracks(tracks) = self.freeze_scope(&request.scope)? else {
            return Ok(DoctorScanOutcome::ScopeFallbackRequired);
        };
        let previous_scan_id = self.last_complete_scan()?.map(|scan| scan.id);
        let mut read_tracks = Vec::with_capacity(tracks.len());
        let mut snapshot_tracks = Vec::with_capacity(tracks.len());
        let mut skipped_tracks = 0;
        for (position, track) in tracks.iter().enumerate() {
            if progress(DoctorScanProgress {
                completed_tracks: position,
                total_tracks: tracks.len(),
            }) == ScanControl::Cancel
            {
                return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
            }
            match crate::library::tag_edit::read_editable_tags(&track.path) {
                Ok(tags) => {
                    snapshot_tracks.push(DoctorTrackSnapshot {
                        reference: track.clone(),
                        tags: Some(tags.clone()),
                        stale: false,
                    });
                    read_tracks.push(ReadTrack {
                        reference: track.clone(),
                        tags,
                    });
                }
                Err(_) => {
                    skipped_tracks += 1;
                    snapshot_tracks.push(DoctorTrackSnapshot {
                        reference: track.clone(),
                        tags: None,
                        stale: false,
                    });
                }
            }
        }
        if progress(DoctorScanProgress {
            completed_tracks: tracks.len(),
            total_tracks: tracks.len(),
        }) == ScanControl::Cancel
        {
            return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
        }
        let (proposals, unresolved_groups) = local_rules::proposals_for(&read_tracks);
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let scan = super::store::persist_complete_scan(&super::store::CompleteScanData {
            conn: self.conn,
            scope_kind: request.scope.kind(),
            created_at,
            options: DoctorScanOptions::local_only(),
            checked_tracks: read_tracks.len(),
            skipped_tracks,
            tracks: &snapshot_tracks,
            proposals: &proposals,
            unresolved_groups: &unresolved_groups,
        })?;
        Ok(DoctorScanOutcome::Completed(scan))
    }

    pub fn last_complete_scan(&self) -> Result<Option<super::DoctorScan>, DoctorError> {
        super::store::last_complete_scan(self.conn)
    }
}
