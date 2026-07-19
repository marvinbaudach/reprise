use super::*;

fn track(track_id: i64) -> DoctorTrackSnapshot {
    DoctorTrackSnapshot {
        reference: DoctorTrackRef {
            track_id,
            path: format!("/fixture/{track_id}.flac").into(),
            file_mtime: track_id,
            file_size: track_id * 10,
            device: Some(1),
            inode: Some(track_id),
        },
        tags: None,
        stale: false,
    }
}

fn proposal(track_id: i64, field: DoctorField, source: ProposalSource) -> DoctorProposal {
    DoctorProposal {
        track_id,
        field,
        current: DoctorValue::Text(format!("old-{track_id}-{field:?}")),
        proposed: DoctorValue::Text(format!("new-{track_id}-{field:?}")),
        source,
        confidence: if source == ProposalSource::Local {
            100
        } else {
            91
        },
        preselected: source == ProposalSource::Local,
        problem_class: ProblemClass::CasingWhitespace,
    }
}

fn scan(proposals: Vec<DoctorProposal>) -> DoctorScan {
    let mut track_ids = proposals
        .iter()
        .map(|proposal| proposal.track_id)
        .collect::<Vec<_>>();
    track_ids.sort_unstable();
    track_ids.dedup();
    DoctorScan {
        id: 17,
        scope_kind: "selection".into(),
        created_at: 1,
        options: DoctorScanOptions::local_only(),
        checked_tracks: track_ids.len(),
        skipped_tracks: 0,
        tracks: track_ids.iter().copied().map(track).collect(),
        track_ids,
        proposals,
        unresolved_groups: Vec::new(),
    }
}

fn unresolved_group(
    field: DoctorField,
    candidates: &[(&str, usize)],
    members: &[(i64, &str)],
) -> DoctorUnresolvedGroup {
    DoctorUnresolvedGroup {
        field,
        group_key: "same-key".into(),
        candidates: candidates
            .iter()
            .map(|(value, count)| DoctorCandidate {
                value: DoctorValue::Text((*value).into()),
                count: *count,
            })
            .collect(),
        members: members
            .iter()
            .map(|(track_id, current)| DoctorGroupMember {
                track_id: *track_id,
                current: DoctorValue::Text((*current).into()),
            })
            .collect(),
    }
}

fn scan_with_group(group: DoctorUnresolvedGroup) -> DoctorScan {
    let track_ids = group
        .members
        .iter()
        .map(|member| member.track_id)
        .collect::<Vec<_>>();
    DoctorScan {
        id: 23,
        scope_kind: "selection".into(),
        created_at: 1,
        options: DoctorScanOptions::local_only(),
        checked_tracks: track_ids.len(),
        skipped_tracks: 0,
        tracks: track_ids.iter().copied().map(track).collect(),
        track_ids,
        proposals: Vec::new(),
        unresolved_groups: vec![group],
    }
}

#[test]
fn doc_3a_review_selects_each_track_field_independently() {
    let scan = scan(vec![
        proposal(1, DoctorField::Artist, ProposalSource::Local),
        proposal(1, DoctorField::Album, ProposalSource::Local),
    ]);
    let mut review = DoctorReviewSession::from_scan(scan, DoctorReviewFilter::AllChanges);
    let rows = review.rows();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.selected));
    let album_id = rows
        .iter()
        .find(|row| row.field == DoctorField::Album)
        .unwrap()
        .id;

    review.set_selected(album_id, false).unwrap();

    assert!(
        review
            .rows()
            .iter()
            .find(|row| row.field == DoctorField::Artist)
            .unwrap()
            .selected
    );
    assert!(
        !review
            .rows()
            .iter()
            .find(|row| row.field == DoctorField::Album)
            .unwrap()
            .selected
    );
}

#[test]
fn doc_3a_all_safe_is_an_exact_reset() {
    let scan = scan(vec![
        proposal(1, DoctorField::Artist, ProposalSource::Local),
        proposal(2, DoctorField::Album, ProposalSource::MusicBrainz),
    ]);
    let mut review = DoctorReviewSession::from_scan(scan, DoctorReviewFilter::AllChanges);
    let local = review
        .rows()
        .iter()
        .find(|row| row.source == ProposalSource::Local)
        .unwrap()
        .id;
    let remote = review
        .rows()
        .iter()
        .find(|row| row.source == ProposalSource::MusicBrainz)
        .unwrap()
        .id;
    review.set_selected(local, false).unwrap();
    review.set_selected(remote, true).unwrap();

    review.all_safe();

    assert!(
        review
            .rows()
            .iter()
            .find(|row| row.id == local)
            .unwrap()
            .selected
    );
    assert!(
        !review
            .rows()
            .iter()
            .find(|row| row.id == remote)
            .unwrap()
            .selected
    );
}

#[test]
fn doc_3a_none_clears_every_row() {
    let mut review = DoctorReviewSession::from_scan(
        scan(vec![
            proposal(1, DoctorField::Artist, ProposalSource::Local),
            proposal(2, DoctorField::Album, ProposalSource::MusicBrainz),
        ]),
        DoctorReviewFilter::AllChanges,
    );
    let remote = review
        .rows()
        .iter()
        .find(|row| row.source == ProposalSource::MusicBrainz)
        .unwrap()
        .id;
    review.set_selected(remote, true).unwrap();

    review.none();

    assert!(review.rows().iter().all(|row| !row.selected));
}

#[test]
fn doc_3a_tie_choice_materializes_only_real_diffs() {
    let group = unresolved_group(
        DoctorField::Artist,
        &[("AC/DC", 1), ("ac/dc", 2)],
        &[(1, "AC/DC"), (2, "ac/dc"), (3, "ac/dc")],
    );
    let mut review =
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::AllChanges);
    let group_id = review.groups()[0].id;
    assert!(review.rows().is_empty());

    review
        .choose_candidate(group_id, &DoctorValue::Text("AC/DC".into()))
        .unwrap();

    assert_eq!(
        review
            .rows()
            .iter()
            .map(|row| row.track_id)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert!(review.rows().iter().all(|row| {
        row.proposed == DoctorValue::Text("AC/DC".into())
            && row.origin == DoctorReviewRowOrigin::ManualGroup(group_id)
            && row.selected
    }));
}

#[test]
fn candidate_switch_preserves_manual_deselections() {
    let group = unresolved_group(
        DoctorField::Artist,
        &[("A", 1), ("B", 1), ("C", 1)],
        &[(1, "A"), (2, "B"), (3, "C")],
    );
    let mut review =
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::AllChanges);
    let group_id = review.groups()[0].id;
    review
        .choose_candidate(group_id, &DoctorValue::Text("A".into()))
        .unwrap();
    let surviving = review
        .rows()
        .iter()
        .find(|row| row.track_id == 3)
        .unwrap()
        .id;
    review.set_selected(surviving, false).unwrap();

    review
        .choose_candidate(group_id, &DoctorValue::Text("B".into()))
        .unwrap();

    assert!(
        !review
            .rows()
            .iter()
            .find(|row| row.track_id == 3)
            .unwrap()
            .selected
    );
    assert!(
        review
            .rows()
            .iter()
            .find(|row| row.track_id == 1)
            .unwrap()
            .selected
    );
}

#[test]
fn stale_and_conflict_rows_cannot_be_selected() {
    let mut source = scan(vec![
        proposal(1, DoctorField::Artist, ProposalSource::Local),
        proposal(2, DoctorField::Artist, ProposalSource::Local),
    ]);
    source.tracks[0].stale = true;
    let mut review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::AllChanges);
    let stale = review
        .rows()
        .iter()
        .find(|row| row.track_id == 1)
        .unwrap()
        .id;
    let conflict = review
        .rows()
        .iter()
        .find(|row| row.track_id == 2)
        .unwrap()
        .id;
    assert_eq!(
        review.set_selected(stale, true),
        Err(DoctorReviewError::RowNotReady)
    );

    review
        .mark_state(conflict, DoctorReviewRowState::Conflict)
        .unwrap();

    assert_eq!(
        review.set_selected(conflict, true),
        Err(DoctorReviewError::RowNotReady)
    );
    assert!(review.rows().iter().all(|row| !row.selected));
}

#[test]
fn doc_3a_apply_plan_is_an_owned_selection_snapshot() {
    let mut review = DoctorReviewSession::from_scan(
        scan(vec![proposal(
            1,
            DoctorField::Artist,
            ProposalSource::Local,
        )]),
        DoctorReviewFilter::AllChanges,
    );

    let plan = review.freeze_plan();
    review.none();

    assert_eq!(plan.scan_id(), 17);
    assert_eq!(plan.track_count(), 1);
    assert_eq!(plan.file_count(), 1);
    assert_eq!(plan.tag_change_count(), 1);
    assert_eq!(plan.changes()[0].track.track_id, 1);
    assert_eq!(plan.changes()[0].field, DoctorField::Artist);
    assert_eq!(
        plan.changes()[0].expected,
        DoctorValue::Text("old-1-Artist".into())
    );
    assert_eq!(
        plan.changes()[0].proposed,
        DoctorValue::Text("new-1-Artist".into())
    );
    assert_eq!(review.freeze_plan().tag_change_count(), 0);
}

#[test]
fn local_safe_review_filter_excludes_remote_and_manual_groups() {
    let mut source = scan(vec![
        proposal(1, DoctorField::Artist, ProposalSource::Local),
        proposal(2, DoctorField::Album, ProposalSource::MusicBrainz),
    ]);
    source.tracks.push(track(3));
    source.track_ids.push(3);
    source.unresolved_groups.push(unresolved_group(
        DoctorField::Genre,
        &[("Rock", 1), ("rock", 1)],
        &[(1, "Rock"), (3, "rock")],
    ));

    let review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::LocalSafeOnly);

    assert_eq!(review.rows().len(), 1);
    assert_eq!(review.rows()[0].source, ProposalSource::Local);
    assert!(review.groups().is_empty());
}

#[test]
fn review_summary_counts_tracks_files_and_tag_changes_once() {
    let review = DoctorReviewSession::from_scan(
        scan(vec![
            proposal(1, DoctorField::Artist, ProposalSource::Local),
            proposal(1, DoctorField::Album, ProposalSource::Local),
            proposal(2, DoctorField::Genre, ProposalSource::Local),
        ]),
        DoctorReviewFilter::AllChanges,
    );

    assert_eq!(
        review.summary(),
        DoctorReviewSummary {
            track_count: 2,
            file_count: 2,
            tag_change_count: 3,
        }
    );
}

#[test]
fn invalid_tie_candidate_is_rejected_without_mutation() {
    let group = unresolved_group(
        DoctorField::Artist,
        &[("A", 1), ("B", 1)],
        &[(1, "A"), (2, "B")],
    );
    let mut review =
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::AllChanges);
    let group_id = review.groups()[0].id;

    let result = review.choose_candidate(group_id, &DoctorValue::Text("Invented".into()));

    assert_eq!(result, Err(DoctorReviewError::InvalidCandidate));
    assert!(review.rows().is_empty());
    assert_eq!(review.groups()[0].chosen, None);
}

#[test]
fn review_order_is_stable_across_selection_and_candidate_changes() {
    let mut low = proposal(1, DoctorField::Title, ProposalSource::AcoustId);
    low.confidence = 41;
    let mut medium = proposal(1, DoctorField::Artist, ProposalSource::MusicBrainz);
    medium.confidence = 76;
    let mut high = proposal(2, DoctorField::Title, ProposalSource::MusicBrainz);
    high.confidence = 98;
    let mut source = scan(vec![
        low,
        proposal(2, DoctorField::Album, ProposalSource::Local),
        medium,
        high,
        proposal(1, DoctorField::Genre, ProposalSource::Local),
    ]);
    source.unresolved_groups.push(unresolved_group(
        DoctorField::AlbumArtist,
        &[("A", 1), ("B", 1)],
        &[(1, "A"), (2, "B")],
    ));
    let mut review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::AllChanges);
    let group_id = review.groups()[0].id;
    review
        .choose_candidate(group_id, &DoctorValue::Text("A".into()))
        .unwrap();
    let before = review
        .rows()
        .iter()
        .map(|row| (row.id, row.track_id, row.field))
        .collect::<Vec<_>>();

    let first = review.rows()[0].id;
    review.set_selected(first, false).unwrap();
    review
        .choose_candidate(group_id, &DoctorValue::Text("A".into()))
        .unwrap();

    let after = review
        .rows()
        .iter()
        .map(|row| (row.id, row.track_id, row.field))
        .collect::<Vec<_>>();
    assert_eq!(after, before);
    assert_eq!(
        after
            .iter()
            .map(|(_, track_id, field)| (*track_id, *field))
            .collect::<Vec<_>>(),
        vec![
            (1, DoctorField::Genre),
            (2, DoctorField::Album),
            (2, DoctorField::AlbumArtist),
            (2, DoctorField::Title),
            (1, DoctorField::Artist),
            (1, DoctorField::Title),
        ]
    );
}

#[test]
fn preset_reset_clears_hidden_tie_selection_memory() {
    let group = unresolved_group(
        DoctorField::Artist,
        &[("A", 1), ("B", 1), ("C", 1)],
        &[(1, "A"), (2, "B"), (3, "C")],
    );
    let mut review =
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::AllChanges);
    let group_id = review.groups()[0].id;
    review
        .choose_candidate(group_id, &DoctorValue::Text("A".into()))
        .unwrap();

    review.all_safe();
    review
        .choose_candidate(group_id, &DoctorValue::Text("B".into()))
        .unwrap();

    assert!(review.rows().iter().all(|row| !row.selected));
}

#[test]
fn marking_a_row_stale_does_not_reorder_the_session() {
    let mut review = DoctorReviewSession::from_scan(
        scan(vec![
            proposal(1, DoctorField::Artist, ProposalSource::Local),
            proposal(2, DoctorField::Artist, ProposalSource::Local),
        ]),
        DoctorReviewFilter::AllChanges,
    );
    let before = review.rows().iter().map(|row| row.id).collect::<Vec<_>>();

    review
        .mark_state(before[0], DoctorReviewRowState::Stale)
        .unwrap();

    assert_eq!(
        review.rows().iter().map(|row| row.id).collect::<Vec<_>>(),
        before
    );
}

#[test]
fn manual_stale_state_survives_candidate_switches() {
    let group = unresolved_group(
        DoctorField::Artist,
        &[("A", 1), ("B", 1), ("C", 1)],
        &[(1, "A"), (2, "B"), (3, "C")],
    );
    let mut review =
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::AllChanges);
    let group_id = review.groups()[0].id;
    review
        .choose_candidate(group_id, &DoctorValue::Text("A".into()))
        .unwrap();
    let row_id = review
        .rows()
        .iter()
        .find(|row| row.track_id == 2)
        .unwrap()
        .id;
    review
        .mark_state(row_id, DoctorReviewRowState::Stale)
        .unwrap();

    review
        .choose_candidate(group_id, &DoctorValue::Text("C".into()))
        .unwrap();

    let row = review.rows().iter().find(|row| row.id == row_id).unwrap();
    assert_eq!(row.state, DoctorReviewRowState::Stale);
    assert!(!row.selected);
    assert_eq!(
        review.set_selected(row_id, true),
        Err(DoctorReviewError::RowNotReady)
    );
}
