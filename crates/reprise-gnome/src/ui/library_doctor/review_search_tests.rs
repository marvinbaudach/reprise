use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library_doctor::{DoctorReviewFilter, DoctorReviewSession, DoctorReviewSummary};

use super::super::review_model::grouped_rows_for;
use super::super::review_row::contract_tests::{
    album_change_scan, conflict_scan, three_album_scan,
};
use super::super::review_snapshot::ReviewSnapshot;
use super::super::review_summary::review_footer_summary;
use super::LibraryDoctorReviewPage;

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

#[test]
fn doc_12a_the_review_search_matches_track_album_and_artist() {
    let scan = album_change_scan();
    let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
    let rows = grouped_rows_for(&scan, &session, &HashMap::new());
    let mut snapshot = ReviewSnapshot::from_rows(rows, "");
    let all_changes = snapshot.unfiltered_changes;

    for (query, expected) in [
        ("Track 1", 1),
        ("One album", all_changes),
        ("Artists", all_changes),
    ] {
        snapshot.apply_query(query);
        assert_eq!(snapshot.totals.changes, expected, "{query}");
    }
}

#[test]
fn doc_12a_the_review_search_ignores_the_normalized_album_key() {
    let (_, mut snapshot) = snapshot("");
    snapshot.rows[0].album_key = "normalized-only-key".into();

    snapshot.apply_query("normalized-only-key");

    assert_eq!(snapshot.totals.changes, 0);
}

#[test]
fn doc_12a_an_empty_query_hides_nothing() {
    let (_, snapshot) = snapshot("   ");

    assert_eq!(snapshot.totals.changes, snapshot.unfiltered_changes);
}

fn page_for(scan: &reprise_core::library_doctor::DoctorScan) -> Rc<LibraryDoctorReviewPage> {
    let conn = Rc::new(Db::open_in_memory().unwrap());
    let parent = adw::ApplicationWindow::builder().build();
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    LibraryDoctorReviewPage::new(
        &conn,
        &parent,
        scan,
        Rc::new(|_| {}),
        Rc::new(|| {}),
        &on_edit,
    )
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_12a_the_conflicts_panel_survives_an_active_query() {
    let _guard = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let page = page_for(&conflict_scan());

    page.state.set_query("matches no review row");

    assert_eq!(page.state.sorted.n_items(), 1);
    assert_eq!(
        page.state.content.visible_child_name().as_deref(),
        Some("rows")
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_12a_a_query_with_no_matches_shows_its_own_state() {
    let _guard = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let page = page_for(&three_album_scan());

    page.state.set_query("matches no review row");

    assert_eq!(page.state.sorted.n_items(), 0);
    assert_eq!(
        page.state.content.visible_child_name().as_deref(),
        Some("no-match")
    );
    let no_match = page
        .state
        .content
        .child_by_name("no-match")
        .and_downcast::<adw::StatusPage>()
        .unwrap();
    assert_eq!(no_match.title(), "No matches for “matches no review row”");
    assert!(no_match.description().unwrap().contains("3 fixes"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_12a_clearing_the_query_restores_every_row() {
    let _guard = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let page = page_for(&three_album_scan());
    let row_count = page.state.sorted.n_items();

    page.state.set_query("second");
    assert!(page.state.sorted.n_items() < row_count);
    page.state.set_query("  ");

    assert_eq!(page.state.sorted.n_items(), row_count);
    assert_eq!(page.state.query.borrow().as_str(), "");
}

#[test]
fn doc_9d_the_footer_states_the_scope_of_the_search() {
    let summary = DoctorReviewSummary {
        track_count: 20,
        file_count: 20,
        tag_change_count: 27,
        total_tag_change_count: 390,
    };

    assert_eq!(
        review_footer_summary(summary, None, "beatles", 433),
        "27 of 390 · filtered by search “beatles”"
    );
}

#[test]
fn doc_9d_the_footer_names_both_search_and_category_when_both_are_active() {
    let summary = DoctorReviewSummary {
        track_count: 20,
        file_count: 20,
        tag_change_count: 27,
        total_tag_change_count: 390,
    };

    assert_eq!(
        review_footer_summary(
            summary,
            Some(super::super::review_model::ReviewCategory::Year),
            "beatles",
            433,
        ),
        "27 of 390 · filtered by Year and search “beatles”"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_12a_a_committed_query_renders_exactly_one_chip() {
    let _guard = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let page = page_for(&three_album_scan());

    page.state.filter_bar.set_committed_query("first");
    page.state.filter_bar.set_committed_query("second");

    let search_slot = page
        .state
        .filter_bar
        .root
        .first_child()
        .unwrap()
        .next_sibling()
        .unwrap();
    let chip = search_slot.first_child().unwrap();
    assert!(chip.next_sibling().is_none());
    assert!(chip
        .downcast::<gtk4::Button>()
        .unwrap()
        .label()
        .unwrap()
        .contains("second"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_12a_apply_writes_only_the_searched_set() {
    let _guard = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let page = page_for(&three_album_scan());

    page.state.set_query("second");

    assert_eq!(
        page.state.session.borrow().freeze_plan().tag_change_count(),
        page.state.snapshot.borrow().totals.selectable,
    );
    assert_eq!(page.state.snapshot.borrow().totals.selectable, 1);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_12a_select_all_under_a_query_marks_only_the_matching_rows() {
    let _guard = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let page = page_for(&three_album_scan());
    page.state.session.borrow_mut().none();

    page.state.set_query("second");
    page.state.session.borrow_mut().all();

    let snapshot = page.state.snapshot.borrow();
    let session = page.state.session.borrow();
    for row in session.rows() {
        assert_eq!(row.selected, snapshot.is_visible(Some(&row.id)));
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9d_a_row_hidden_by_the_query_keeps_its_selection_and_stays_out_of_the_plan() {
    let _guard = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let page = page_for(&three_album_scan());

    page.state.set_query("second");

    let hidden = page
        .state
        .session
        .borrow()
        .rows()
        .iter()
        .find(|row| !page.state.snapshot.borrow().is_visible(Some(&row.id)))
        .unwrap()
        .clone();
    assert!(hidden.selected);
    assert!(page
        .state
        .session
        .borrow()
        .freeze_plan()
        .changes()
        .iter()
        .all(|change| change.row_id != hidden.id));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_12a_search_and_category_compose_as_an_intersection() {
    let _guard = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let page = page_for(&album_change_scan());

    page.state.set_query("Track 1");
    page.state
        .set_category(Some(super::super::review_model::ReviewCategory::Year));

    assert_eq!(page.state.sorted.n_items(), 0);
    assert_eq!(
        page.state.session.borrow().freeze_plan().tag_change_count(),
        0
    );
}
