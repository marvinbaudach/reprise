use std::collections::HashMap;

use reprise_core::library_doctor::{DoctorReviewFilter, DoctorReviewSession};

use super::super::review_model::grouped_rows_for;
use super::super::review_row::contract_tests::three_album_scan;
use super::super::review_snapshot::ReviewSnapshot;

fn snapshot(query: &str) -> (DoctorReviewSession, ReviewSnapshot) {
    let scan = three_album_scan();
    let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
    let rows = grouped_rows_for(&scan, &session, &HashMap::new());
    let snapshot = ReviewSnapshot::from_rows(rows, query);
    (session, snapshot)
}

#[test]
fn doc_9d_a_searched_header_counts_only_the_matching_rows() {
    let (_, snapshot) = snapshot("second");

    assert_eq!(snapshot.totals.changes, 1);
    assert_eq!(snapshot.totals.albums, 1);
    assert_eq!(snapshot.unfiltered_changes, 3);
}

#[test]
fn doc_3c_the_master_check_covers_only_the_searched_rows() {
    let (mut session, snapshot) = snapshot("second");
    let hidden = snapshot.rows[0].row_ids[0];
    session.set_selected(hidden, false).unwrap();

    let updated = snapshot
        .clone()
        .with_selection(&snapshot.selection_diff(&session));

    assert_eq!(updated.totals.selected, 1);
    assert_eq!(updated.totals.selectable, 1);
}

#[test]
fn review_snapshot_apply_query_preserves_absolute_row_positions() {
    let (mut session, mut snapshot) = snapshot("");
    let row_ids = snapshot
        .rows
        .iter()
        .map(|row| row.row_ids[0])
        .collect::<Vec<_>>();

    snapshot.apply_query("second");
    session.none();
    let changed = snapshot.selection_diff(&session);

    assert_eq!(
        snapshot
            .rows
            .iter()
            .map(|row| row.row_ids[0])
            .collect::<Vec<_>>(),
        row_ids
    );
    assert_eq!(
        changed
            .iter()
            .map(|(position, _)| *position)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn review_snapshot_toggling_a_hidden_row_does_not_move_the_totals() {
    let (mut session, snapshot) = snapshot("second");
    let hidden = snapshot.rows[0].row_ids[0];
    assert!(!snapshot.is_visible(Some(&hidden)));
    let totals = snapshot.totals;
    session.set_selected(hidden, false).unwrap();

    let changed = snapshot.selection_diff(&session);
    let updated = snapshot.with_selection(&changed);

    assert_eq!(updated.totals, totals);
    assert!(!updated.rows[0].row.selected);
}
