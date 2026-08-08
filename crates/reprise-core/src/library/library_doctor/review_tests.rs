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
        never_preselect: false,
        problem_class: ProblemClass::CasingWhitespace,
        resolved_release_mbid: None,
        evidence: Vec::new(),
        local_fallback: None,
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
                evidence: Vec::new(),
            })
            .collect(),
        local_fallback: None,
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
fn doc_8b_auto_applied_tier_is_local_preselected_plus_every_recording_mbid() {
    let mut remote_mbid = proposal(3, DoctorField::RecordingMbid, ProposalSource::MusicBrainz);
    remote_mbid.preselected = false;
    let review = DoctorReviewSession::from_scan(
        scan(vec![
            proposal(1, DoctorField::Artist, ProposalSource::Local),
            proposal(2, DoctorField::Album, ProposalSource::MusicBrainz),
            remote_mbid,
        ]),
        DoctorReviewFilter::AutoApply,
    );

    assert_eq!(review.rows().len(), 2);
    assert!(review.rows().iter().all(|row| row.selected));
    assert!(review.rows().iter().any(|row| row.track_id == 1));
    assert!(review.rows().iter().any(|row| row.track_id == 3));
}

#[test]
fn doc_8b_stale_rows_are_never_auto_applied() {
    let mut source = scan(vec![proposal(
        1,
        DoctorField::Artist,
        ProposalSource::Local,
    )]);
    source.tracks[0].stale = true;

    let review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::AutoApply);

    assert!(review.rows().is_empty());
}

#[test]
fn doc_8b_review_tier_preselects_every_ready_row() {
    let mut capped = proposal(3, DoctorField::Title, ProposalSource::MusicBrainz);
    capped.confidence = 49;
    capped.never_preselect = true;
    let review = DoctorReviewSession::from_scan(
        scan(vec![
            proposal(1, DoctorField::Artist, ProposalSource::MusicBrainz),
            proposal(2, DoctorField::Genre, ProposalSource::AcoustId),
            capped,
        ]),
        DoctorReviewFilter::NeedsReview,
    );

    assert_eq!(review.rows().len(), 3);
    assert!(review
        .rows()
        .iter()
        .filter(|row| !row.never_preselect)
        .all(|row| row.selected));
    assert!(
        !review
            .rows()
            .iter()
            .find(|row| row.never_preselect)
            .unwrap()
            .selected
    );
}

#[test]
fn doc_4c_a_capped_proposal_does_not_start_selected() {
    let mut capped = proposal(1, DoctorField::Title, ProposalSource::MusicBrainz);
    capped.confidence = 49;
    capped.never_preselect = true;

    let review =
        DoctorReviewSession::from_scan(scan(vec![capped]), DoctorReviewFilter::NeedsReview);

    assert_eq!(review.rows()[0].state, DoctorReviewRowState::Ready);
    assert!(!review.rows()[0].selected);
}

#[test]
fn doc_4c_a_row_below_fifty_percent_does_not_start_selected() {
    let mut low_confidence = proposal(1, DoctorField::Title, ProposalSource::AcoustId);
    low_confidence.confidence = 49;

    let review =
        DoctorReviewSession::from_scan(scan(vec![low_confidence]), DoctorReviewFilter::NeedsReview);

    assert_eq!(review.rows()[0].state, DoctorReviewRowState::Ready);
    assert!(!review.rows()[0].selected);
}

#[test]
fn doc_8b_one_predicate_decides_the_initial_selection() {
    let source = include_str!("review.rs");
    let normalized = source.split_whitespace().collect::<Vec<_>>().join(" ");

    assert_eq!(source.matches("fn starts_selected(").count(), 1);
    assert_eq!(source.matches("starts_selected(").count(), 4);
    assert_eq!(normalized.matches("selected: starts_selected(").count(), 1);
    assert_eq!(
        normalized
            .matches("row.selected = starts_selected(")
            .count(),
        1
    );
    assert_eq!(
        normalized
            .matches("let selected = starts_selected(")
            .count(),
        1
    );
    assert!(!source.contains("selected: state == DoctorReviewRowState::Ready"));
}

#[test]
fn doc_8b_recording_mbid_never_reaches_the_review_tier() {
    let review = DoctorReviewSession::from_scan(
        scan(vec![proposal(
            1,
            DoctorField::RecordingMbid,
            ProposalSource::AcoustId,
        )]),
        DoctorReviewFilter::NeedsReview,
    );

    assert!(review.rows().is_empty());
}

#[test]
fn doc_3a_review_selects_each_track_field_independently() {
    let scan = scan(vec![
        proposal(1, DoctorField::Artist, ProposalSource::MusicBrainz),
        proposal(1, DoctorField::Album, ProposalSource::MusicBrainz),
    ]);
    let mut review = DoctorReviewSession::from_scan(scan, DoctorReviewFilter::NeedsReview);
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
fn doc_8b_all_preset_selects_every_ready_row_and_none_clears_them() {
    let mut capped = proposal(3, DoctorField::Title, ProposalSource::MusicBrainz);
    capped.confidence = 49;
    capped.never_preselect = true;
    let source = scan(vec![
        proposal(1, DoctorField::Artist, ProposalSource::AcoustId),
        proposal(2, DoctorField::Album, ProposalSource::MusicBrainz),
        capped,
    ]);
    let mut review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::NeedsReview);
    let ready = review.rows()[0].id;
    let stale = review.rows()[1].id;
    review
        .mark_state(stale, DoctorReviewRowState::Stale)
        .unwrap();
    review.set_selected(ready, false).unwrap();

    review.all();

    assert!(review
        .rows()
        .iter()
        .filter(|row| row.state == DoctorReviewRowState::Ready)
        .all(|row| row.selected));
    assert!(
        !review
            .rows()
            .iter()
            .find(|row| row.id == stale)
            .unwrap()
            .selected
    );

    review.none();

    assert!(review.rows().iter().all(|row| !row.selected));
}

#[test]
fn doc_3a_none_clears_every_row() {
    let mut review = DoctorReviewSession::from_scan(
        scan(vec![
            proposal(1, DoctorField::Artist, ProposalSource::AcoustId),
            proposal(2, DoctorField::Album, ProposalSource::MusicBrainz),
        ]),
        DoctorReviewFilter::NeedsReview,
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

fn review_with_year_filter() -> DoctorReviewSession {
    let casing = proposal(1, DoctorField::Artist, ProposalSource::MusicBrainz);
    let mut year = proposal(2, DoctorField::Year, ProposalSource::MusicBrainz);
    year.problem_class = ProblemClass::MissingWrongYear;
    let mut review =
        DoctorReviewSession::from_scan(scan(vec![casing, year]), DoctorReviewFilter::NeedsReview);
    review.set_category_filter(Some(std::collections::HashSet::from([
        ProblemClass::MissingWrongYear,
    ])));
    review
}

#[test]
fn doc_9d_a_filtered_apply_writes_only_the_filtered_set() {
    let review = review_with_year_filter();
    let plan = review.freeze_plan();
    assert_eq!(plan.tag_change_count(), 1);
    assert_eq!(plan.changes()[0].field, DoctorField::Year);
}

#[test]
fn doc_9d_all_and_none_operate_on_the_filtered_set() {
    let mut review = review_with_year_filter();

    review.none();
    assert_eq!(review.rows().iter().filter(|row| row.selected).count(), 1);
    assert!(review
        .rows()
        .iter()
        .any(|row| row.selected && row.field == DoctorField::Artist));
    review.all();
    assert!(review.rows().iter().all(|row| row.selected));
}

#[test]
fn doc_9d_every_number_recomputes_from_one_selection_state() {
    let mut review = review_with_year_filter();

    review.none();
    let summary = review.summary();
    assert_eq!(summary.tag_change_count, 0);
    assert_eq!(summary.total_tag_change_count, 1);
    review.all();
    assert_eq!(review.summary().tag_change_count, 1);
    assert_eq!(review.summary().total_tag_change_count, 2);
}

#[test]
fn doc_7c_review_remote_toggle_removes_selection_and_restores_local_result() {
    let mut remote = proposal(1, DoctorField::Title, ProposalSource::MusicBrainz);
    remote.local_fallback = Some(DoctorLocalFallback::Proposal {
        proposed: DoctorValue::Text("local-title".into()),
        confidence: 100,
        problem_class: ProblemClass::CasingWhitespace,
    });
    let mut source = scan(vec![remote]);
    source.options.remote_enabled = true;
    let mut review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::NeedsReview);
    let remote_id = review.rows()[0].id;
    review.set_selected(remote_id, true).unwrap();

    review.set_remote_visible(false);

    assert!(!review.remote_visible());
    assert!(review.rows().is_empty());

    review.set_remote_visible(true);

    assert!(review.remote_visible());
    assert_eq!(review.rows().len(), 1);
    assert_eq!(review.rows()[0].source, ProposalSource::MusicBrainz);
    assert!(review.rows()[0].selected);
}

#[test]
fn doc_3a_tie_choice_materializes_only_real_diffs() {
    let group = unresolved_group(
        DoctorField::Artist,
        &[("AC/DC", 1), ("ac/dc", 2)],
        &[(1, "AC/DC"), (2, "ac/dc"), (3, "ac/dc")],
    );
    let mut review =
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::NeedsReview);
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
fn doc_4c_a_tie_choice_that_reduces_specificity_is_capped_and_never_preselected() {
    let group = unresolved_group(
        DoctorField::Title,
        &[("An Ocean", 2), ("an ocean", 1)],
        &[(1, "An Ocean Between Us"), (2, "An Ocean")],
    );
    let mut review =
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::NeedsReview);
    let group_id = review.groups()[0].id;

    review
        .choose_candidate(group_id, &DoctorValue::Text("An Ocean".into()))
        .unwrap();

    let truncated = review
        .rows()
        .iter()
        .find(|row| row.track_id == 1)
        .expect("the shortened title stays reviewable");
    assert_eq!(truncated.state, DoctorReviewRowState::Ready);
    assert!(truncated.never_preselect);
    assert!(truncated.confidence <= 49);
    assert!(!truncated.selected);
}

#[test]
fn doc_8b_the_tie_path_runs_the_same_selection_predicate() {
    let group = unresolved_group(
        DoctorField::Title,
        &[("An Ocean", 2), ("an ocean", 1)],
        &[(1, "An Ocean Between Us"), (2, "an ocean"), (3, "AN OCEAN")],
    );
    let mut source = scan_with_group(group);
    source
        .tracks
        .iter_mut()
        .find(|track| track.reference.track_id == 3)
        .expect("fixture track")
        .stale = true;
    let mut review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::NeedsReview);
    let group_id = review.groups()[0].id;

    review
        .choose_candidate(group_id, &DoctorValue::Text("An Ocean".into()))
        .unwrap();

    let mut selected = review
        .rows()
        .iter()
        .map(|row| (row.track_id, row.selected))
        .collect::<Vec<_>>();
    selected.sort_unstable();
    // 1 reduces specificity, 3 is stale, 2 is the plain casing fix.
    assert_eq!(selected, vec![(1, false), (2, true), (3, false)]);
}

#[test]
fn candidate_switch_preserves_manual_deselections() {
    let group = unresolved_group(
        DoctorField::Artist,
        &[("A", 1), ("B", 1), ("C", 1)],
        &[(1, "A"), (2, "B"), (3, "C")],
    );
    let mut review =
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::NeedsReview);
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
        proposal(1, DoctorField::Artist, ProposalSource::MusicBrainz),
        proposal(2, DoctorField::Artist, ProposalSource::MusicBrainz),
    ]);
    source.tracks[0].stale = true;
    let mut review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::NeedsReview);
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
            ProposalSource::MusicBrainz,
        )]),
        DoctorReviewFilter::NeedsReview,
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

    let review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::AutoApply);

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
        DoctorReviewFilter::AutoApply,
    );

    assert_eq!(
        review.summary(),
        DoctorReviewSummary {
            track_count: 2,
            file_count: 2,
            tag_change_count: 3,
            total_tag_change_count: 3,
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
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::NeedsReview);
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
        proposal(2, DoctorField::Album, ProposalSource::MusicBrainz),
        medium,
        high,
        proposal(1, DoctorField::Genre, ProposalSource::MusicBrainz),
    ]);
    source.unresolved_groups.push(unresolved_group(
        DoctorField::AlbumArtist,
        &[("A", 1), ("B", 1)],
        &[(1, "A"), (2, "B")],
    ));
    let mut review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::NeedsReview);
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
            (2, DoctorField::AlbumArtist),
            (1, DoctorField::Genre),
            (2, DoctorField::Title),
            (2, DoctorField::Album),
            (1, DoctorField::Artist),
            (1, DoctorField::Title),
        ]
    );
}

#[test]
fn all_preset_selects_hidden_tie_selection_memory() {
    let group = unresolved_group(
        DoctorField::Artist,
        &[("A", 1), ("B", 1), ("C", 1)],
        &[(1, "A"), (2, "B"), (3, "C")],
    );
    let mut review =
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::NeedsReview);
    let group_id = review.groups()[0].id;
    review
        .choose_candidate(group_id, &DoctorValue::Text("A".into()))
        .unwrap();

    review.all();
    review
        .choose_candidate(group_id, &DoctorValue::Text("B".into()))
        .unwrap();

    assert!(review.rows().iter().all(|row| row.selected));
}

#[test]
fn marking_a_row_stale_does_not_reorder_the_session() {
    let mut review = DoctorReviewSession::from_scan(
        scan(vec![
            proposal(1, DoctorField::Artist, ProposalSource::Local),
            proposal(2, DoctorField::Artist, ProposalSource::Local),
        ]),
        DoctorReviewFilter::AutoApply,
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
        DoctorReviewSession::from_scan(scan_with_group(group), DoctorReviewFilter::NeedsReview);
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
