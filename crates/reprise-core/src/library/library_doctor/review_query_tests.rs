use std::collections::HashSet;

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
        confidence: 91,
        preselected: false,
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
        id: 29,
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

fn two_row_review() -> DoctorReviewSession {
    DoctorReviewSession::from_scan(
        scan(vec![
            proposal(1, DoctorField::Artist, ProposalSource::MusicBrainz),
            proposal(2, DoctorField::Year, ProposalSource::MusicBrainz),
        ]),
        DoctorReviewFilter::NeedsReview,
    )
}

#[test]
fn doc_12a_the_query_scope_limits_the_frozen_plan() {
    let mut review = two_row_review();
    let included = review.rows()[0].id;
    review.set_query_scope(Some(HashSet::from([included])));

    let plan = review.freeze_plan();

    assert_eq!(plan.tag_change_count(), 1);
    assert_eq!(plan.changes()[0].row_id, included);
}

#[test]
fn doc_12a_all_and_none_operate_only_on_the_query_scope() {
    let mut review = two_row_review();
    let included = review.rows()[0].id;
    let excluded = review.rows()[1].id;
    review.set_query_scope(Some(HashSet::from([included])));

    review.none();
    assert!(
        !review
            .rows()
            .iter()
            .find(|row| row.id == included)
            .unwrap()
            .selected
    );
    assert!(
        review
            .rows()
            .iter()
            .find(|row| row.id == excluded)
            .unwrap()
            .selected
    );

    review.set_selected(excluded, false).unwrap();
    review.all();
    assert!(
        review
            .rows()
            .iter()
            .find(|row| row.id == included)
            .unwrap()
            .selected
    );
    assert!(
        !review
            .rows()
            .iter()
            .find(|row| row.id == excluded)
            .unwrap()
            .selected
    );
}

#[test]
fn doc_12a_a_row_outside_the_query_scope_keeps_its_selection() {
    let mut review = two_row_review();
    let included = review.rows()[0].id;
    let excluded = review.rows()[1].id;
    review.set_query_scope(Some(HashSet::from([included])));

    review.none();

    assert!(
        review
            .rows()
            .iter()
            .find(|row| row.id == excluded)
            .unwrap()
            .selected
    );
}

#[test]
fn doc_12a_the_query_scope_survives_a_remote_visibility_rebuild() {
    let local = proposal(1, DoctorField::Artist, ProposalSource::Local);
    let remote = proposal(2, DoctorField::Year, ProposalSource::MusicBrainz);
    let mut source = scan(vec![local, remote]);
    source.options.remote_enabled = true;
    let mut review = DoctorReviewSession::from_scan(source, DoctorReviewFilter::NeedsReview);
    let included = review.rows()[0].id;
    let excluded = review.rows()[1].id;
    review.set_query_scope(Some(HashSet::from([included])));

    review.set_remote_visible(false);
    review.set_remote_visible(true);

    assert!(review.query_scope_matches(included));
    assert!(!review.query_scope_matches(excluded));
}

#[test]
fn doc_12a_the_query_scope_intersects_the_category_filter() {
    let mut review = two_row_review();
    let casing = review.rows()[0].id;
    review.set_query_scope(Some(HashSet::from([casing])));
    review.set_category_filter(Some(HashSet::from([ProblemClass::MissingWrongYear])));

    assert_eq!(review.freeze_plan().tag_change_count(), 0);
}
