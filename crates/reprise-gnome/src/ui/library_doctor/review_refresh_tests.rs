use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library_doctor::{DoctorReviewFilter, DoctorReviewSession};

use super::super::review_model::{grouped_rows_for, ReviewCategory, ReviewRowModel};
use super::super::review_row::contract_tests::{album_change_scan, conflict_scan};
use super::super::review_snapshot::ReviewSnapshot;
use super::compare_rows;

fn sorted_count_for(
    session: Rc<RefCell<DoctorReviewSession>>,
    snapshot: &ReviewSnapshot,
    panel_present: bool,
) -> u32 {
    let store = gio::ListStore::new::<glib::Object>();
    let objects = snapshot
        .rows
        .iter()
        .cloned()
        .map(|row| glib::BoxedAnyObject::new(row).upcast::<glib::Object>())
        .collect::<Vec<_>>();
    store.splice(0, 0, &objects);
    if panel_present {
        store.append(&gtk4::StringObject::new("conflicts-panel"));
    }
    let filter = gtk4::CustomFilter::new(move |object| {
        let Some(boxed) = object.downcast_ref::<glib::BoxedAnyObject>() else {
            return true;
        };
        let model = boxed.borrow::<ReviewRowModel>();
        session
            .borrow()
            .category_filter_matches(model.row.problem_class)
    });
    let filtered = gtk4::FilterListModel::new(Some(store), Some(filter));
    let sorter = gtk4::CustomSorter::new(|left, right| compare_rows(left, right, false));
    let sorted = gtk4::SortListModel::new(Some(filtered), Some(sorter));
    sorted.n_items()
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9b_the_snapshot_is_the_visible_row_set() {
    gtk4::init().unwrap();
    for (category, panel_present) in [(None, false), (Some(ReviewCategory::Year), true)] {
        let scan = if panel_present {
            conflict_scan()
        } else {
            album_change_scan()
        };
        let mut session =
            DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
        session.set_category_filter(category.map(ReviewCategory::problem_classes));
        let rows = grouped_rows_for(&scan, &session, &HashMap::new());
        let snapshot = ReviewSnapshot::from_rows(rows);
        let expected = u32::try_from(snapshot.rows.len()).unwrap() + u32::from(panel_present);
        let actual = sorted_count_for(Rc::new(RefCell::new(session)), &snapshot, panel_present);

        assert_eq!(
            actual, expected,
            "the sorted model diverged from the cached visible rows for {category:?}"
        );
    }
}
