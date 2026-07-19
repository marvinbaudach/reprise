use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use super::local_rules::{self, ReadTrack};
use super::remote::{self, RemoteProviderError, RemoteResolver};
use super::scope;
use super::{
    DoctorError, DoctorScanOptions, DoctorScanOutcome, DoctorScanProgress, DoctorScanRequest,
    DoctorScopeRequest, DoctorTrackSnapshot, FrozenScope, LocalScanRequest, ScanControl,
};
use crate::fingerprint::FingerprintBackend;

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
        let request = DoctorScanRequest {
            scope: request.scope.clone(),
            options: DoctorScanOptions::local_only(),
        };
        let mut resolver = remote::ProviderRemoteResolver::new(remote::NoNetworkProvider);
        self.scan_with_resolver(&request, None, &mut resolver, &mut progress)
    }

    pub fn scan(
        &mut self,
        request: &DoctorScanRequest,
        fingerprint_backend: Option<&dyn FingerprintBackend>,
        mut progress: impl FnMut(DoctorScanProgress) -> ScanControl,
    ) -> Result<DoctorScanOutcome, DoctorError> {
        let mut resolver = remote::ProviderRemoteResolver::new(remote::NetworkProvider::new());
        self.scan_with_resolver(request, fingerprint_backend, &mut resolver, &mut progress)
    }

    pub(crate) fn scan_with_resolver(
        &mut self,
        request: &DoctorScanRequest,
        fingerprint_backend: Option<&dyn FingerprintBackend>,
        resolver: &mut dyn RemoteResolver,
        progress: &mut dyn FnMut(DoctorScanProgress) -> ScanControl,
    ) -> Result<DoctorScanOutcome, DoctorError> {
        let FrozenScope::Tracks(tracks) = self.freeze_scope(&request.scope)? else {
            return Ok(DoctorScanOutcome::ScopeFallbackRequired);
        };
        let previous_scan_id = self.last_complete_scan()?.map(|scan| scan.id);
        let mut read_tracks = Vec::with_capacity(tracks.len());
        let mut snapshot_tracks = Vec::with_capacity(tracks.len());
        let mut remote_resolutions = Vec::with_capacity(tracks.len());
        let mut skipped_tracks = 0;
        for (position, track) in tracks.iter().enumerate() {
            if progress(DoctorScanProgress {
                completed_tracks: position,
                total_tracks: tracks.len(),
            }) == ScanControl::Cancel
            {
                return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
            }
            match remote::read_remote_metadata(&track.path) {
                Ok((tags, metadata)) => {
                    snapshot_tracks.push(DoctorTrackSnapshot {
                        reference: track.clone(),
                        tags: Some(tags.clone()),
                        stale: false,
                    });
                    read_tracks.push(ReadTrack {
                        reference: track.clone(),
                        tags,
                    });
                    if request.options.remote_enabled {
                        let mut control = || {
                            progress(DoctorScanProgress {
                                completed_tracks: position,
                                total_tracks: tracks.len(),
                            })
                        };
                        match resolver.resolve_track(
                            &metadata,
                            &track.path,
                            fingerprint_backend,
                            &mut control,
                        ) {
                            Ok(resolution) => {
                                remote_resolutions.push((track.track_id, resolution));
                            }
                            Err(RemoteProviderError::Cancelled) => {
                                return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
                            }
                            Err(_) => {}
                        }
                    }
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
            if progress(DoctorScanProgress {
                completed_tracks: position + 1,
                total_tracks: tracks.len(),
            }) == ScanControl::Cancel
            {
                return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
            }
        }
        let (mut proposals, mut unresolved_groups) = local_rules::proposals_for(&read_tracks);
        for (track_id, mut resolution) in remote_resolutions {
            for proposal in &mut resolution.proposals {
                proposal.track_id = track_id;
                proposal.local_fallback = take_local_fallback(
                    &mut proposals,
                    &mut unresolved_groups,
                    track_id,
                    proposal.field,
                );
            }
            for group in &mut resolution.groups {
                group.group_key = format!("{}:{track_id}", group.group_key);
                for member in &mut group.members {
                    member.track_id = track_id;
                }
                group.local_fallback = take_local_fallback(
                    &mut proposals,
                    &mut unresolved_groups,
                    track_id,
                    group.field,
                );
            }
            proposals.extend(resolution.proposals);
            unresolved_groups.extend(resolution.groups);
        }
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let scan = super::store::persist_complete_scan(&super::store::CompleteScanData {
            conn: self.conn,
            scope_kind: request.scope.kind(),
            created_at,
            options: request.options,
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

fn take_local_fallback(
    proposals: &mut Vec<super::DoctorProposal>,
    groups: &mut Vec<super::DoctorUnresolvedGroup>,
    track_id: i64,
    field: super::DoctorField,
) -> Option<super::DoctorLocalFallback> {
    let proposal_position = proposals.iter().position(|proposal| {
        proposal.track_id == track_id
            && proposal.field == field
            && proposal.source == super::ProposalSource::Local
    });
    let proposal_fallback = proposal_position.map(|position| {
        let proposal = proposals.remove(position);
        super::DoctorLocalFallback::Proposal {
            proposed: proposal.proposed,
            confidence: proposal.confidence,
            problem_class: proposal.problem_class,
        }
    });
    let mut manual_fallback = None;
    for group in groups.iter_mut().filter(|group| group.field == field) {
        let original_members = group.members.clone();
        let current = group
            .members
            .iter()
            .find(|member| member.track_id == track_id)
            .map(|member| member.current.clone());
        group.members.retain(|member| member.track_id != track_id);
        if let Some(current) = current {
            manual_fallback = Some(super::DoctorLocalFallback::Manual {
                group_key: group.group_key.clone(),
                candidates: group.candidates.clone(),
                members: original_members,
            });
            if let Some(candidate) = group
                .candidates
                .iter_mut()
                .find(|candidate| candidate.value == current)
            {
                candidate.count = candidate.count.saturating_sub(1);
            }
            group.candidates.retain(|candidate| candidate.count > 0);
        }
    }
    groups.retain(|group| group.members.len() > 1 && group.candidates.len() > 1);
    proposal_fallback.or(manual_fallback)
}
