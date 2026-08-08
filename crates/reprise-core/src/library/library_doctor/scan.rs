use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::db::Db;
use crate::library::group_key::normalize_group_key;

use super::local_rules::{self, ReadTrack};
use super::remote::{self, RemoteProviderError, RemoteResolver};
use super::scope;
use super::{
    DoctorError, DoctorScanOptions, DoctorScanOutcome, DoctorScanPhase, DoctorScanProgress,
    DoctorScanRequest, DoctorScopeRequest, DoctorTrackSnapshot, FrozenScope, LocalScanRequest,
    ScanControl,
};
use crate::fingerprint::FingerprintBackend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorScanCompletion {
    pub scan: super::DoctorScan,
    pub auto_applied: Option<super::DoctorWriteReport>,
}

pub struct LibraryDoctor<'connection> {
    pub(super) db: &'connection Db,
    pub(super) conn: &'connection Connection,
}

impl<'connection> LibraryDoctor<'connection> {
    pub fn new(db: &'connection Db) -> Self {
        let conn = db.conn();
        Self { db, conn }
    }

    pub fn freeze_scope(
        &mut self,
        request: &DoctorScopeRequest,
    ) -> Result<FrozenScope, DoctorError> {
        scope::freeze_scope(self.db, request).map_err(DoctorError::from)
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
        let FrozenScope::Tracks(tracks) = self.freeze_scope(&request.scope)? else {
            return Ok(DoctorScanOutcome::ScopeFallbackRequired);
        };
        let provider = remote::CachedRemoteProvider::new(
            remote::NetworkProvider::new(),
            self.conn,
            unix_timestamp(),
        );
        let mut resolver = remote::ProviderRemoteResolver::new(provider);
        self.scan_tracks_with_resolver(
            request,
            &tracks,
            fingerprint_backend,
            &mut resolver,
            &mut progress,
        )
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
        self.scan_tracks_with_resolver(request, &tracks, fingerprint_backend, resolver, progress)
    }

    fn scan_tracks_with_resolver(
        &self,
        request: &DoctorScanRequest,
        tracks: &[super::DoctorTrackRef],
        fingerprint_backend: Option<&dyn FingerprintBackend>,
        resolver: &mut dyn RemoteResolver,
        progress: &mut dyn FnMut(DoctorScanProgress) -> ScanControl,
    ) -> Result<DoctorScanOutcome, DoctorError> {
        let previous_scan = self.last_complete_scan()?;
        let previous_scan_id = previous_scan.as_ref().map(|scan| scan.id);
        let previous_tracks = previous_scan_id.map_or_else(
            || Ok(HashMap::new()),
            |scan_id| super::store::previous_scan_identities(self.conn, scan_id),
        )?;
        let current_track_ids = tracks
            .iter()
            .map(|track| track.track_id)
            .collect::<Vec<_>>();
        // A remote result belongs to the selected release for the whole album,
        // not just to one file. Reuse it only when the complete frozen input is
        // identical; a partial reuse could mix two release decisions again.
        let reuse_remote_scan = request.options.remote_enabled
            && previous_scan.as_ref().is_some_and(|scan| {
                scan.options.remote_enabled
                    && scan.track_ids == current_track_ids
                    && tracks.iter().all(|track| {
                        previous_tracks
                            .get(&track.track_id)
                            .is_some_and(|previous| previous.snapshot.reference == *track)
                    })
            });
        let may_reuse_files = !request.options.remote_enabled || reuse_remote_scan;
        let mut read_tracks = Vec::with_capacity(tracks.len());
        let mut remote_metadata = Vec::with_capacity(tracks.len());
        let mut snapshot_tracks = Vec::with_capacity(tracks.len());
        let mut skipped_tracks = 0;
        let mut preview_summary = super::DoctorScanSummary::default();
        let to_read = tracks
            .iter()
            .filter(|track| {
                !may_reuse_files
                    || !previous_tracks
                        .get(&track.track_id)
                        .is_some_and(|previous| previous.snapshot.reference == **track)
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut local_reads = read_tracks_parallel(&to_read)
            .into_iter()
            .map(|(track, result)| (track.track_id, result))
            .collect::<HashMap<_, _>>();
        for (position, track) in tracks.iter().enumerate() {
            if progress(DoctorScanProgress {
                phase: DoctorScanPhase::ReadingTags,
                completed_tracks: position,
                total_tracks: tracks.len(),
                summary: preview_summary,
            }) == ScanControl::Cancel
            {
                return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
            }
            let result = previous_tracks
                .get(&track.track_id)
                .filter(|previous| may_reuse_files && previous.snapshot.reference == *track)
                .map_or_else(
                    || local_reads.remove(&track.track_id).unwrap_or(None),
                    |previous| {
                        previous.snapshot.tags.clone().map(|tags| {
                            let metadata = metadata_from_saved_tags(&tags);
                            (tags, metadata)
                        })
                    },
                );
            match result {
                Some((tags, metadata)) => {
                    snapshot_tracks.push(DoctorTrackSnapshot {
                        reference: (*track).clone(),
                        tags: Some(tags.clone()),
                        stale: false,
                    });
                    let read_track = ReadTrack {
                        reference: (*track).clone(),
                        tags,
                    };
                    let (track_proposals, _track_groups) =
                        local_rules::proposals_for(std::slice::from_ref(&read_track));
                    // Groups are deliberately reported as none while the scan
                    // runs. A spelling conflict is a statement about several
                    // tracks disagreeing with each other, and a running scan
                    // has not met them all yet: `add_grouped_field` cannot tie
                    // a single track against itself, so the local part of
                    // `track_groups` is always empty here, while the remote
                    // part is whatever this one track's lookup happened to
                    // return — and `merge` adds that up per track. Either way
                    // the sum is a forecast, not a count, and it disagreed
                    // visibly with the tracks-checked number beside it. The
                    // completed scan recomputes over the whole set below, from
                    // the scanned tracks alone, and is the only place this
                    // number is true.
                    preview_summary.merge(super::presentation::partial_scan_summary(
                        &track_proposals,
                        0,
                        1,
                        0,
                    ));
                    read_tracks.push(read_track);
                    remote_metadata.push(metadata);
                }
                None => {
                    skipped_tracks += 1;
                    preview_summary.merge(super::presentation::partial_scan_summary(&[], 0, 0, 1));
                    snapshot_tracks.push(DoctorTrackSnapshot {
                        reference: (*track).clone(),
                        tags: None,
                        stale: false,
                    });
                }
            }
            let last_local_remote_track =
                request.options.remote_enabled && position + 1 == tracks.len();
            if !last_local_remote_track
                && progress(DoctorScanProgress {
                    phase: DoctorScanPhase::ReadingTags,
                    completed_tracks: position + 1,
                    total_tracks: tracks.len(),
                    summary: preview_summary,
                }) == ScanControl::Cancel
            {
                return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
            }
        }
        let (mut proposals, mut unresolved_groups) = local_rules::proposals_for(&read_tracks);
        if request.options.remote_enabled {
            if progress(DoctorScanProgress {
                phase: DoctorScanPhase::CheckingRemote,
                completed_tracks: tracks.len().saturating_sub(1),
                total_tracks: tracks.len(),
                summary: preview_summary,
            }) == ScanControl::Cancel
            {
                return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
            }
            if reuse_remote_scan {
                reuse_remote_results(
                    &read_tracks,
                    &previous_tracks,
                    &mut proposals,
                    &mut unresolved_groups,
                );
            } else {
                let album_groups = group_album_tracks(&read_tracks);
                for indices in album_groups {
                    let query = album_query(&read_tracks, &indices);
                    let album_resolution = if let Some(query) = query {
                        let mut control = || {
                            progress(DoctorScanProgress {
                                phase: DoctorScanPhase::CheckingRemote,
                                completed_tracks: tracks.len().saturating_sub(1),
                                total_tracks: tracks.len(),
                                summary: preview_summary,
                            })
                        };
                        match resolver.resolve_album(&remote::AlbumRequest { query }, &mut control)
                        {
                            Ok(resolution) => resolution,
                            Err(RemoteProviderError::Cancelled) => {
                                return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
                            }
                            Err(_) => remote::AlbumResolution::default(),
                        }
                    } else {
                        remote::AlbumResolution {
                            attempted: true,
                            album_match: None,
                        }
                    };
                    for index in indices {
                        let read_track = &read_tracks[index];
                        let metadata = &remote_metadata[index];
                        let published_summary = preview_summary;
                        let mut control = || {
                            progress(DoctorScanProgress {
                                phase: DoctorScanPhase::CheckingRemote,
                                completed_tracks: tracks.len().saturating_sub(1),
                                total_tracks: tracks.len(),
                                summary: published_summary,
                            })
                        };
                        match resolver.resolve_track(
                            metadata,
                            &read_track.reference.path,
                            fingerprint_backend,
                            album_resolution.album_match.as_ref(),
                            &mut control,
                        ) {
                            Ok(mut resolution) => {
                                if album_resolution.attempted {
                                    retain_track_fields(&mut resolution);
                                }
                                merge_remote_resolution(
                                    read_track.reference.track_id,
                                    resolution,
                                    &mut proposals,
                                    &mut unresolved_groups,
                                );
                            }
                            Err(RemoteProviderError::Cancelled) => {
                                return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
                            }
                            Err(_) => {}
                        }
                        if let Some(album_match) = &album_resolution.album_match {
                            let resolution =
                                remote::album_resolution_for_track(metadata, album_match);
                            merge_remote_resolution(
                                read_track.reference.track_id,
                                resolution,
                                &mut proposals,
                                &mut unresolved_groups,
                            );
                        }
                    }
                }
            }
            preview_summary = super::presentation::partial_scan_summary(
                &proposals,
                unresolved_groups.len(),
                read_tracks.len(),
                skipped_tracks,
            );
            if progress(DoctorScanProgress {
                phase: DoctorScanPhase::CheckingRemote,
                completed_tracks: tracks.len(),
                total_tracks: tracks.len(),
                summary: preview_summary,
            }) == ScanControl::Cancel
            {
                return Ok(DoctorScanOutcome::Cancelled { previous_scan_id });
            }
        }
        let created_at = unix_timestamp();
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

    pub fn apply_auto_tier(
        &mut self,
        scan: &super::DoctorScan,
        progress: impl FnMut(super::DoctorWriteProgress) -> super::DoctorWriteControl,
    ) -> Result<Option<super::DoctorWriteReport>, DoctorError> {
        let plan = super::DoctorReviewSession::from_scan(
            scan.clone(),
            super::DoctorReviewFilter::AutoApply,
        )
        .freeze_plan();
        if plan.changes().is_empty() {
            return Ok(None);
        }
        self.apply_review_plan(&plan, progress).map(Some)
    }
}

fn read_tracks_parallel(
    tracks: &[super::DoctorTrackRef],
) -> Vec<(
    super::DoctorTrackRef,
    Option<(
        crate::library::tag_edit::EditableTags,
        remote::RemoteTrackMetadata,
    )>,
)> {
    if tracks.is_empty() {
        return Vec::new();
    }
    let worker_count = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(tracks.len());
    let chunk_size = tracks.len().div_ceil(worker_count);
    std::thread::scope(|scope| {
        let handles = tracks
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|track| {
                            (
                                track.clone(),
                                remote::read_remote_metadata(&track.path).ok(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    })
}

fn metadata_from_saved_tags(
    tags: &crate::library::tag_edit::EditableTags,
) -> remote::RemoteTrackMetadata {
    let present = |value: &str| (!value.trim().is_empty()).then(|| value.to_owned());
    remote::RemoteTrackMetadata {
        title: present(&tags.title),
        artist: present(&tags.artist),
        album: present(&tags.album),
        album_artist: present(&tags.album_artist),
        year: tags.year,
        recording_mbid: None,
        release_mbid: None,
        release_group_mbid: None,
        artist_mbid: None,
        release_artist_mbid: None,
        duration_ms: None,
    }
}

fn reuse_remote_results(
    read_tracks: &[ReadTrack],
    previous_tracks: &HashMap<i64, super::store::PreviousTrackScan>,
    proposals: &mut Vec<super::DoctorProposal>,
    unresolved_groups: &mut Vec<super::DoctorUnresolvedGroup>,
) {
    for track in read_tracks {
        let track_id = track.reference.track_id;
        let Some(previous) = previous_tracks.get(&track_id) else {
            continue;
        };
        let reused_proposals = previous
            .proposals
            .iter()
            .filter(|proposal| proposal.source != super::ProposalSource::Local)
            .cloned()
            .map(|mut proposal| {
                proposal.local_fallback = None;
                proposal
            })
            .collect();
        let reused_groups = previous
            .unresolved_groups
            .iter()
            .filter(|group| {
                group.members.first().map(|member| member.track_id) == Some(track_id)
                    && group
                        .candidates
                        .iter()
                        .any(|candidate| !candidate.evidence.is_empty())
            })
            .cloned()
            .map(|mut group| {
                group.local_fallback = None;
                if let Some(unscoped) = group.group_key.strip_suffix(&format!(":{track_id}")) {
                    group.group_key = unscoped.to_owned();
                }
                group
            })
            .collect();
        merge_remote_resolution(
            track_id,
            remote::RemoteResolution {
                proposals: reused_proposals,
                groups: reused_groups,
            },
            proposals,
            unresolved_groups,
        );
    }
}

fn retain_track_fields(resolution: &mut remote::RemoteResolution) {
    resolution.proposals.retain(|proposal| {
        matches!(
            proposal.field,
            super::DoctorField::Title
                | super::DoctorField::Artist
                | super::DoctorField::RecordingMbid
        )
    });
    resolution.groups.retain(|group| {
        matches!(
            group.field,
            super::DoctorField::Title | super::DoctorField::Artist
        )
    });
}

fn group_album_tracks(tracks: &[ReadTrack]) -> Vec<Vec<usize>> {
    let mut positions = HashMap::<String, usize>::new();
    let mut groups = Vec::<Vec<usize>>::new();
    for (index, track) in tracks.iter().enumerate() {
        let key = format!(
            "{}\u{1}{}",
            normalize_group_key(&track.tags.album_artist),
            normalize_group_key(&track.tags.album)
        );
        let position = *positions.entry(key).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[position].push(index);
    }
    groups
}

fn album_query(tracks: &[ReadTrack], indices: &[usize]) -> Option<remote::AlbumQuery> {
    let first = tracks.get(*indices.first()?)?;
    if normalize_group_key(&first.tags.album_artist).is_empty()
        || normalize_group_key(&first.tags.album).is_empty()
    {
        return None;
    }
    Some(remote::AlbumQuery {
        album_artist: first.tags.album_artist.clone(),
        album: first.tags.album.clone(),
        track_titles: indices
            .iter()
            .map(|index| tracks[*index].tags.title.clone())
            .collect(),
        track_count: u32::try_from(indices.len()).unwrap_or(u32::MAX),
        year: indices.iter().find_map(|index| tracks[*index].tags.year),
    })
}

fn merge_remote_resolution(
    track_id: i64,
    mut resolution: remote::RemoteResolution,
    proposals: &mut Vec<super::DoctorProposal>,
    unresolved_groups: &mut Vec<super::DoctorUnresolvedGroup>,
) {
    let mut remote_proposals = Vec::with_capacity(resolution.proposals.len());
    for mut proposal in resolution.proposals.drain(..) {
        proposal.track_id = track_id;
        let same_local_target = proposals.iter().any(|local| {
            local.track_id == track_id
                && local.field == proposal.field
                && local.source == super::ProposalSource::Local
                && local.proposed == proposal.proposed
        });
        if same_local_target {
            continue;
        }
        proposal.local_fallback =
            take_local_fallback(proposals, unresolved_groups, track_id, proposal.field);
        remote_proposals.push(proposal);
    }
    for group in &mut resolution.groups {
        group.group_key = format!("{}:{track_id}", group.group_key);
        for member in &mut group.members {
            member.track_id = track_id;
        }
        group.local_fallback =
            take_local_fallback(proposals, unresolved_groups, track_id, group.field);
    }
    proposals.extend(remote_proposals);
    unresolved_groups.extend(resolution.groups);
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
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
