use std::collections::{HashMap, HashSet};

use super::{
    DoctorField, DoctorProposal, DoctorScan, DoctorTrackRef, DoctorValue, ProblemClass,
    ProposalSource,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DoctorReviewRowId(u64);

impl DoctorReviewRowId {
    pub(crate) const fn raw(self) -> u64 {
        self.0
    }

    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DoctorReviewGroupId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorReviewFilter {
    AllChanges,
    LocalSafeOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorReviewRowState {
    Ready,
    Stale,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorReviewRowOrigin {
    Proposal,
    ManualGroup(DoctorReviewGroupId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReviewRow {
    pub id: DoctorReviewRowId,
    pub track_id: i64,
    pub field: DoctorField,
    pub current: DoctorValue,
    pub proposed: DoctorValue,
    pub source: ProposalSource,
    pub confidence: u8,
    pub evidence: Vec<super::remote::RemoteEvidence>,
    pub problem_class: ProblemClass,
    pub state: DoctorReviewRowState,
    pub selected: bool,
    pub origin: DoctorReviewRowOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReviewGroup {
    pub id: DoctorReviewGroupId,
    pub field: DoctorField,
    pub group_key: String,
    pub candidates: Vec<super::DoctorCandidate>,
    pub chosen: Option<DoctorValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoctorReviewSummary {
    pub track_count: usize,
    pub file_count: usize,
    pub tag_change_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorApplyChange {
    pub row_id: DoctorReviewRowId,
    pub track: DoctorTrackRef,
    pub field: DoctorField,
    pub expected: DoctorValue,
    pub proposed: DoctorValue,
    pub source: ProposalSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorApplyPlan {
    scan_id: i64,
    changes: Vec<DoctorApplyChange>,
    track_count: usize,
    file_count: usize,
    tag_change_count: usize,
}

impl DoctorApplyPlan {
    pub const fn scan_id(&self) -> i64 {
        self.scan_id
    }

    pub fn changes(&self) -> &[DoctorApplyChange] {
        &self.changes
    }

    pub const fn track_count(&self) -> usize {
        self.track_count
    }

    pub const fn file_count(&self) -> usize {
        self.file_count
    }

    pub const fn tag_change_count(&self) -> usize {
        self.tag_change_count
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DoctorReviewError {
    #[error("unknown Library Doctor review row")]
    UnknownRow,
    #[error("a stale or conflicting row cannot be selected")]
    RowNotReady,
    #[error("unknown Library Doctor review group")]
    UnknownGroup,
    #[error("the value is not a candidate in this Library Doctor review group")]
    InvalidCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RowSortKey {
    category: u8,
    scope_position: usize,
    field_position: u8,
    sequence: u64,
}

pub struct DoctorReviewSession {
    scan_id: i64,
    source_scan: DoctorScan,
    filter: DoctorReviewFilter,
    remote_visible: bool,
    tracks: HashMap<i64, DoctorTrackRef>,
    rows: Vec<DoctorReviewRow>,
    groups: Vec<DoctorReviewGroup>,
    sort_keys: HashMap<DoctorReviewRowId, RowSortKey>,
    local_safe: HashMap<DoctorReviewRowId, bool>,
    tie_templates: HashMap<DoctorReviewGroupId, Vec<TieRowTemplate>>,
    tie_selection: HashMap<DoctorReviewRowId, bool>,
}

#[derive(Debug, Clone)]
struct TieRowTemplate {
    id: DoctorReviewRowId,
    track_id: i64,
    field: DoctorField,
    current: DoctorValue,
    state: DoctorReviewRowState,
}

impl DoctorReviewSession {
    pub fn from_scan(scan: DoctorScan, filter: DoctorReviewFilter) -> Self {
        let remote_visible = scan.options.remote_enabled;
        let source_scan = scan.clone();
        Self::build(scan, filter, source_scan, remote_visible)
    }

    fn build(
        scan: DoctorScan,
        filter: DoctorReviewFilter,
        source_scan: DoctorScan,
        remote_visible: bool,
    ) -> Self {
        let scope_positions = scan
            .track_ids
            .iter()
            .enumerate()
            .map(|(position, track_id)| (*track_id, position))
            .collect::<HashMap<_, _>>();
        let tracks = scan
            .tracks
            .iter()
            .map(|track| (track.reference.track_id, track.reference.clone()))
            .collect::<HashMap<_, _>>();
        let stale = scan
            .tracks
            .iter()
            .map(|track| (track.reference.track_id, track.stale))
            .collect::<HashMap<_, _>>();
        let mut rows = Vec::new();
        let mut sort_keys = HashMap::new();
        let mut local_safe = HashMap::new();
        let mut next_id = 0_u64;
        for proposal in scan.proposals {
            let is_stale = stale.get(&proposal.track_id).copied().unwrap_or(true);
            let is_local_safe = proposal.source == ProposalSource::Local && proposal.preselected;
            if filter == DoctorReviewFilter::LocalSafeOnly && (!is_local_safe || is_stale) {
                continue;
            }
            let id = DoctorReviewRowId(next_id);
            next_id += 1;
            let state = if is_stale {
                DoctorReviewRowState::Stale
            } else {
                DoctorReviewRowState::Ready
            };
            sort_keys.insert(
                id,
                RowSortKey {
                    category: proposal_category(&proposal, state),
                    scope_position: scope_positions
                        .get(&proposal.track_id)
                        .copied()
                        .unwrap_or(usize::MAX),
                    field_position: field_position(proposal.field),
                    sequence: next_id,
                },
            );
            local_safe.insert(id, is_local_safe);
            rows.push(DoctorReviewRow {
                id,
                track_id: proposal.track_id,
                field: proposal.field,
                current: proposal.current,
                proposed: proposal.proposed,
                source: proposal.source,
                confidence: proposal.confidence,
                evidence: proposal.evidence,
                problem_class: proposal.problem_class,
                state,
                selected: state == DoctorReviewRowState::Ready && is_local_safe,
                origin: DoctorReviewRowOrigin::Proposal,
            });
        }
        let mut groups = Vec::new();
        let mut tie_templates = HashMap::new();
        if filter == DoctorReviewFilter::AllChanges {
            for unresolved in scan.unresolved_groups {
                let group_id = DoctorReviewGroupId(groups.len() as u64);
                let mut templates = Vec::new();
                for member in unresolved.members {
                    let id = DoctorReviewRowId(next_id);
                    next_id += 1;
                    let is_stale = stale.get(&member.track_id).copied().unwrap_or(true);
                    sort_keys.insert(
                        id,
                        RowSortKey {
                            category: if is_stale { 5 } else { 1 },
                            scope_position: scope_positions
                                .get(&member.track_id)
                                .copied()
                                .unwrap_or(usize::MAX),
                            field_position: field_position(unresolved.field),
                            sequence: next_id,
                        },
                    );
                    local_safe.insert(id, false);
                    templates.push(TieRowTemplate {
                        id,
                        track_id: member.track_id,
                        field: unresolved.field,
                        current: member.current,
                        state: if is_stale {
                            DoctorReviewRowState::Stale
                        } else {
                            DoctorReviewRowState::Ready
                        },
                    });
                }
                tie_templates.insert(group_id, templates);
                groups.push(DoctorReviewGroup {
                    id: group_id,
                    field: unresolved.field,
                    group_key: unresolved.group_key,
                    candidates: unresolved.candidates,
                    chosen: None,
                });
            }
            groups.sort_by_key(|group| {
                tie_templates
                    .get(&group.id)
                    .and_then(|templates| {
                        templates
                            .iter()
                            .filter_map(|template| sort_keys.get(&template.id))
                            .min()
                            .copied()
                    })
                    .unwrap_or(RowSortKey {
                        category: 1,
                        scope_position: usize::MAX,
                        field_position: field_position(group.field),
                        sequence: group.id.0,
                    })
            });
        }
        rows.sort_by_key(|row| sort_keys[&row.id]);
        Self {
            scan_id: scan.id,
            source_scan,
            filter,
            remote_visible,
            tracks,
            rows,
            groups,
            sort_keys,
            local_safe,
            tie_templates,
            tie_selection: HashMap::new(),
        }
    }

    pub fn rows(&self) -> &[DoctorReviewRow] {
        &self.rows
    }

    pub fn groups(&self) -> &[DoctorReviewGroup] {
        &self.groups
    }

    pub const fn remote_visible(&self) -> bool {
        self.remote_visible
    }

    pub fn set_remote_visible(&mut self, visible: bool) {
        if self.remote_visible == visible {
            return;
        }
        let prior_rows = self
            .rows
            .iter()
            .map(|row| {
                (
                    row.track_id,
                    row.field,
                    row.current.clone(),
                    row.proposed.clone(),
                    row.source,
                    row.selected,
                )
            })
            .collect::<Vec<_>>();
        let prior_groups = self
            .groups
            .iter()
            .filter_map(|group| {
                group
                    .chosen
                    .clone()
                    .map(|chosen| (group.field, group.group_key.clone(), chosen))
            })
            .collect::<Vec<_>>();
        let projected = super::project_scan(&self.source_scan, visible);
        let mut rebuilt = Self::build(projected, self.filter, self.source_scan.clone(), visible);
        for (field, group_key, chosen) in prior_groups {
            let group_id = rebuilt
                .groups
                .iter()
                .find(|group| group.field == field && group.group_key == group_key)
                .map(|group| group.id);
            if let Some(group_id) = group_id {
                let _ = rebuilt.choose_candidate(group_id, &chosen);
            }
        }
        for row in &mut rebuilt.rows {
            if let Some((.., selected)) = prior_rows.iter().find(|prior| {
                prior.0 == row.track_id
                    && prior.1 == row.field
                    && prior.2 == row.current
                    && prior.3 == row.proposed
                    && prior.4 == row.source
            }) {
                row.selected = *selected && row.state == DoctorReviewRowState::Ready;
            }
        }
        *self = rebuilt;
    }

    pub fn set_selected(
        &mut self,
        id: DoctorReviewRowId,
        selected: bool,
    ) -> Result<(), DoctorReviewError> {
        let row = self
            .rows
            .iter_mut()
            .find(|row| row.id == id)
            .ok_or(DoctorReviewError::UnknownRow)?;
        if selected && row.state != DoctorReviewRowState::Ready {
            return Err(DoctorReviewError::RowNotReady);
        }
        row.selected = selected;
        if matches!(row.origin, DoctorReviewRowOrigin::ManualGroup(_)) {
            self.tie_selection.insert(id, selected);
        }
        Ok(())
    }

    pub fn all_safe(&mut self) {
        for templates in self.tie_templates.values() {
            for template in templates {
                self.tie_selection.insert(template.id, false);
            }
        }
        for row in &mut self.rows {
            row.selected = row.state == DoctorReviewRowState::Ready
                && self.local_safe.get(&row.id).copied().unwrap_or(false);
        }
    }

    pub fn none(&mut self) {
        for templates in self.tie_templates.values() {
            for template in templates {
                self.tie_selection.insert(template.id, false);
            }
        }
        for row in &mut self.rows {
            row.selected = false;
        }
    }

    pub fn choose_candidate(
        &mut self,
        group_id: DoctorReviewGroupId,
        candidate: &DoctorValue,
    ) -> Result<(), DoctorReviewError> {
        let group = self
            .groups
            .iter()
            .find(|group| group.id == group_id)
            .ok_or(DoctorReviewError::UnknownGroup)?;
        let chosen = group
            .candidates
            .iter()
            .find(|existing| &existing.value == candidate)
            .cloned()
            .ok_or(DoctorReviewError::InvalidCandidate)?;
        let confidence = chosen
            .evidence
            .iter()
            .map(|evidence| evidence.confidence)
            .min()
            .unwrap_or(100);
        let source = match chosen.evidence.first().map(|evidence| evidence.source) {
            Some(super::remote::RemoteEvidenceSource::AcoustId) => ProposalSource::AcoustId,
            Some(super::remote::RemoteEvidenceSource::MusicBrainz) => ProposalSource::MusicBrainz,
            None => ProposalSource::Local,
        };
        let templates = self
            .tie_templates
            .get(&group_id)
            .cloned()
            .ok_or(DoctorReviewError::UnknownGroup)?;
        self.groups
            .iter_mut()
            .find(|group| group.id == group_id)
            .expect("validated group must remain present")
            .chosen = Some(candidate.clone());
        self.rows.retain(
            |row| !matches!(row.origin, DoctorReviewRowOrigin::ManualGroup(id) if id == group_id),
        );
        for template in templates {
            if &template.current == candidate {
                continue;
            }
            let state = template.state;
            let selected = state == DoctorReviewRowState::Ready
                && self
                    .tie_selection
                    .get(&template.id)
                    .copied()
                    .unwrap_or(true);
            self.tie_selection.entry(template.id).or_insert(selected);
            self.rows.push(DoctorReviewRow {
                id: template.id,
                track_id: template.track_id,
                field: template.field,
                current: template.current,
                proposed: candidate.clone(),
                source,
                confidence,
                evidence: chosen.evidence.clone(),
                problem_class: tie_problem_class(template.field),
                state,
                selected,
                origin: DoctorReviewRowOrigin::ManualGroup(group_id),
            });
        }
        self.sort_rows();
        Ok(())
    }

    pub fn mark_state(
        &mut self,
        id: DoctorReviewRowId,
        state: DoctorReviewRowState,
    ) -> Result<(), DoctorReviewError> {
        let row = self
            .rows
            .iter_mut()
            .find(|row| row.id == id)
            .ok_or(DoctorReviewError::UnknownRow)?;
        row.state = state;
        if state != DoctorReviewRowState::Ready {
            row.selected = false;
            if matches!(row.origin, DoctorReviewRowOrigin::ManualGroup(_)) {
                self.tie_selection.insert(id, false);
            }
        }
        if let DoctorReviewRowOrigin::ManualGroup(group_id) = row.origin {
            if let Some(template) = self
                .tie_templates
                .get_mut(&group_id)
                .and_then(|templates| templates.iter_mut().find(|template| template.id == id))
            {
                template.state = state;
            }
        }
        Ok(())
    }

    pub fn summary(&self) -> DoctorReviewSummary {
        let plan = self.freeze_plan();
        DoctorReviewSummary {
            track_count: plan.track_count(),
            file_count: plan.file_count(),
            tag_change_count: plan.tag_change_count(),
        }
    }

    pub fn freeze_plan(&self) -> DoctorApplyPlan {
        let changes = self
            .rows
            .iter()
            .filter(|row| row.selected && row.state == DoctorReviewRowState::Ready)
            .filter_map(|row| {
                self.tracks
                    .get(&row.track_id)
                    .cloned()
                    .map(|track| DoctorApplyChange {
                        row_id: row.id,
                        track,
                        field: row.field,
                        expected: row.current.clone(),
                        proposed: row.proposed.clone(),
                        source: row.source,
                    })
            })
            .collect::<Vec<_>>();
        let track_count = changes
            .iter()
            .map(|change| change.track.track_id)
            .collect::<HashSet<_>>()
            .len();
        let file_count = changes
            .iter()
            .map(|change| change.track.path.clone())
            .collect::<HashSet<_>>()
            .len();
        let tag_change_count = changes.len();
        DoctorApplyPlan {
            scan_id: self.scan_id,
            changes,
            track_count,
            file_count,
            tag_change_count,
        }
    }

    fn sort_rows(&mut self) {
        self.rows.sort_by_key(|row| self.sort_keys[&row.id]);
    }
}

const fn tie_problem_class(field: DoctorField) -> ProblemClass {
    if matches!(field, DoctorField::Genre) {
        ProblemClass::GenreVariant
    } else {
        ProblemClass::CasingWhitespace
    }
}

fn proposal_category(proposal: &DoctorProposal, state: DoctorReviewRowState) -> u8 {
    if state != DoctorReviewRowState::Ready {
        return 5;
    }
    match proposal.source {
        ProposalSource::Local => 0,
        ProposalSource::MusicBrainz | ProposalSource::AcoustId if proposal.confidence >= 85 => 2,
        ProposalSource::MusicBrainz | ProposalSource::AcoustId if proposal.confidence >= 50 => 3,
        ProposalSource::MusicBrainz | ProposalSource::AcoustId => 4,
    }
}

const fn field_position(field: DoctorField) -> u8 {
    match field {
        DoctorField::Title => 0,
        DoctorField::Artist => 1,
        DoctorField::Album => 2,
        DoctorField::AlbumArtist => 3,
        DoctorField::Year => 4,
        DoctorField::Genre => 5,
        DoctorField::RecordingMbid => 6,
    }
}
