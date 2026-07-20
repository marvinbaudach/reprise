use super::{
    DoctorField, DoctorLocalFallback, DoctorProposal, DoctorScan, DoctorUnresolvedGroup,
    ProblemClass, ProposalSource,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DoctorProblemCount {
    pub safe: usize,
    pub review: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DoctorScanSummary {
    pub safe_changes: usize,
    pub review_changes: usize,
    pub unresolved_groups: usize,
    pub checked_tracks: usize,
    pub skipped_tracks: usize,
    problem_counts: [DoctorProblemCount; 5],
}

impl DoctorScanSummary {
    pub const fn counts_for(self, class: ProblemClass) -> DoctorProblemCount {
        self.problem_counts[problem_class_position(class)]
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.safe_changes = self.safe_changes.saturating_add(other.safe_changes);
        self.review_changes = self.review_changes.saturating_add(other.review_changes);
        self.unresolved_groups = self
            .unresolved_groups
            .saturating_add(other.unresolved_groups);
        self.checked_tracks = self.checked_tracks.saturating_add(other.checked_tracks);
        self.skipped_tracks = self.skipped_tracks.saturating_add(other.skipped_tracks);
        for (counts, added) in self.problem_counts.iter_mut().zip(other.problem_counts) {
            counts.safe = counts.safe.saturating_add(added.safe);
            counts.review = counts.review.saturating_add(added.review);
        }
    }
}

pub fn project_scan(scan: &DoctorScan, remote_visible: bool) -> DoctorScan {
    if remote_visible {
        return scan.clone();
    }
    let mut projected = scan.clone();
    projected.options.remote_enabled = false;
    projected.proposals.clear();
    projected.unresolved_groups.clear();

    for proposal in &scan.proposals {
        if proposal.source == ProposalSource::Local {
            projected.proposals.push(proposal.clone());
        } else if let Some(fallback) = &proposal.local_fallback {
            restore_fallback(
                &mut projected,
                proposal.track_id,
                proposal.field,
                &proposal.current,
                fallback,
            );
        }
    }
    for group in &scan.unresolved_groups {
        let remote = group.local_fallback.is_some()
            || group
                .candidates
                .iter()
                .any(|candidate| !candidate.evidence.is_empty());
        if !remote {
            projected.unresolved_groups.push(group.clone());
        } else if let Some(fallback) = &group.local_fallback {
            let Some(member) = group.members.first() else {
                continue;
            };
            restore_fallback(
                &mut projected,
                member.track_id,
                group.field,
                &member.current,
                fallback,
            );
        }
    }
    projected
}

pub fn scan_summary(scan: &DoctorScan, remote_visible: bool) -> DoctorScanSummary {
    let projected = project_scan(scan, remote_visible);
    let stale_tracks = projected.stale_track_ids();
    summary_for_parts(
        &projected.proposals,
        projected.unresolved_groups.len(),
        projected.checked_tracks,
        projected.skipped_tracks,
        &stale_tracks,
    )
}

pub(crate) fn partial_scan_summary(
    proposals: &[DoctorProposal],
    unresolved_groups: usize,
    checked_tracks: usize,
    skipped_tracks: usize,
) -> DoctorScanSummary {
    summary_for_parts(
        proposals,
        unresolved_groups,
        checked_tracks,
        skipped_tracks,
        &[],
    )
}

fn summary_for_parts(
    proposals: &[DoctorProposal],
    unresolved_groups: usize,
    checked_tracks: usize,
    skipped_tracks: usize,
    stale_tracks: &[i64],
) -> DoctorScanSummary {
    let mut summary = DoctorScanSummary {
        checked_tracks,
        skipped_tracks,
        unresolved_groups,
        ..DoctorScanSummary::default()
    };
    for proposal in proposals {
        let safe = proposal.source == ProposalSource::Local
            && proposal.preselected
            && !stale_tracks.contains(&proposal.track_id);
        let counts = &mut summary.problem_counts[problem_class_position(proposal.problem_class)];
        if safe {
            summary.safe_changes += 1;
            counts.safe += 1;
        } else {
            summary.review_changes += 1;
            counts.review += 1;
        }
    }
    summary
}

fn restore_fallback(
    projected: &mut DoctorScan,
    track_id: i64,
    field: DoctorField,
    current: &super::DoctorValue,
    fallback: &DoctorLocalFallback,
) {
    match fallback {
        DoctorLocalFallback::Proposal {
            proposed,
            confidence,
            problem_class,
        } => {
            let exists = projected.proposals.iter().any(|proposal| {
                proposal.track_id == track_id
                    && proposal.field == field
                    && proposal.source == ProposalSource::Local
            });
            if !exists {
                projected.proposals.push(DoctorProposal {
                    track_id,
                    field,
                    current: current.clone(),
                    proposed: proposed.clone(),
                    source: ProposalSource::Local,
                    confidence: *confidence,
                    preselected: true,
                    problem_class: *problem_class,
                    evidence: Vec::new(),
                    local_fallback: None,
                });
            }
        }
        DoctorLocalFallback::Manual {
            group_key,
            candidates,
            members,
        } => {
            projected
                .unresolved_groups
                .retain(|group| group.field != field || group.group_key != *group_key);
            projected.unresolved_groups.push(DoctorUnresolvedGroup {
                field,
                group_key: group_key.clone(),
                candidates: candidates.clone(),
                members: members.clone(),
                local_fallback: None,
            });
        }
    }
}

const fn problem_class_position(class: ProblemClass) -> usize {
    match class {
        ProblemClass::CasingWhitespace => 0,
        ProblemClass::MissingAlbumArtist => 1,
        ProblemClass::GenreVariant => 2,
        ProblemClass::MissingWrongYear => 3,
        ProblemClass::MissingRecordingMbid => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    fn track(track_id: i64) -> DoctorTrackSnapshot {
        DoctorTrackSnapshot {
            reference: DoctorTrackRef {
                track_id,
                path: format!("/fixture/{track_id}.flac").into(),
                file_mtime: 1,
                file_size: 2,
                device: Some(3),
                inode: Some(track_id),
            },
            tags: None,
            stale: false,
        }
    }

    fn remote_scan() -> DoctorScan {
        DoctorScan {
            id: 1,
            scope_kind: "selection".into(),
            created_at: 2,
            options: DoctorScanOptions {
                remote_enabled: true,
            },
            checked_tracks: 2,
            skipped_tracks: 1,
            track_ids: vec![1, 2],
            tracks: vec![track(1), track(2)],
            proposals: vec![
                DoctorProposal {
                    track_id: 1,
                    field: DoctorField::Title,
                    current: DoctorValue::Text(" old ".into()),
                    proposed: DoctorValue::Text("Canonical".into()),
                    source: ProposalSource::MusicBrainz,
                    confidence: 92,
                    preselected: false,
                    problem_class: ProblemClass::CasingWhitespace,
                    evidence: Vec::new(),
                    local_fallback: Some(DoctorLocalFallback::Proposal {
                        proposed: DoctorValue::Text("old".into()),
                        confidence: 100,
                        problem_class: ProblemClass::CasingWhitespace,
                    }),
                },
                DoctorProposal {
                    track_id: 2,
                    field: DoctorField::RecordingMbid,
                    current: DoctorValue::Empty,
                    proposed: DoctorValue::Text("mbid".into()),
                    source: ProposalSource::AcoustId,
                    confidence: 41,
                    preselected: false,
                    problem_class: ProblemClass::MissingRecordingMbid,
                    evidence: Vec::new(),
                    local_fallback: None,
                },
            ],
            unresolved_groups: Vec::new(),
        }
    }

    #[test]
    fn doc_1d_hiding_remote_restores_local_fallback_and_removes_remote_rows() {
        let scan = remote_scan();

        let projected = project_scan(&scan, false);

        assert!(!projected.options.remote_enabled);
        assert_eq!(projected.proposals.len(), 1);
        let fallback = &projected.proposals[0];
        assert_eq!(fallback.track_id, 1);
        assert_eq!(fallback.source, ProposalSource::Local);
        assert_eq!(fallback.proposed, DoctorValue::Text("old".into()));
        assert!(fallback.preselected);
        assert!(fallback.evidence.is_empty());
        assert_eq!(scan.proposals.len(), 2, "the durable result stays intact");
    }

    #[test]
    fn doc_2b_summary_separates_safe_review_classes_and_unresolved_groups() {
        let scan = remote_scan();

        let local = scan_summary(&scan, false);
        let remote = scan_summary(&scan, true);

        assert_eq!(local.safe_changes, 1);
        assert_eq!(local.review_changes, 0);
        assert_eq!(local.unresolved_groups, 0);
        assert_eq!(local.checked_tracks, 2);
        assert_eq!(local.skipped_tracks, 1);
        assert_eq!(
            local.counts_for(ProblemClass::CasingWhitespace),
            DoctorProblemCount { safe: 1, review: 0 }
        );
        assert_eq!(remote.safe_changes, 0);
        assert_eq!(remote.review_changes, 2);
        assert_eq!(
            remote.counts_for(ProblemClass::MissingRecordingMbid),
            DoctorProblemCount { safe: 0, review: 1 }
        );
    }
}
