use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library_doctor::{DoctorReviewFilter, DoctorReviewSession};

use super::super::review_model::{grouped_rows_for, ReviewCategory, ReviewRowModel};
use super::super::review_row::contract_tests::{album_change_scan, conflict_scan};
use super::super::review_snapshot::ReviewSnapshot;
use super::{compare_rows, LibraryDoctorReviewPage};

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

#[test]
fn review_snapshot_selection_diff_changes_only_selection_facts() {
    let scan = album_change_scan();
    let mut session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
    let snapshot = ReviewSnapshot::from_rows(grouped_rows_for(&scan, &session, &HashMap::new()));
    let selected_before = snapshot.totals.selected;
    assert!(
        snapshot.selection_diff(&session).is_empty(),
        "an unchanged session must not allocate replacement rows"
    );

    session.none();
    let changed = snapshot.selection_diff(&session);
    assert_eq!(changed.len(), snapshot.rows.len());
    assert!(changed
        .iter()
        .all(|(_, row)| row.selected_change_count == 0 && !row.row.selected));

    let updated = snapshot.clone().with_selection(&changed);
    assert_eq!(
        snapshot.totals.selected, selected_before,
        "building a replacement snapshot must not mutate the cached value"
    );
    assert_eq!(updated.totals.selected, 0);
    assert!(updated.albums.values().all(|album| album.selected == 0));
    assert_eq!(updated.totals.changes, snapshot.totals.changes);
    assert_eq!(updated.totals.albums, snapshot.totals.albums);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9b_a_rebound_album_header_does_not_emit_a_selection_change() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder()
        .default_width(900)
        .default_height(700)
        .build();
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    let page = LibraryDoctorReviewPage::new(
        &conn,
        &parent,
        &album_change_scan(),
        Rc::new(|_| {}),
        Rc::new(|| {}),
        &on_edit,
    );
    parent.set_content(Some(page.navigation_page()));
    parent.present();
    drain_main_context();

    let checkbox = realized_album_checkbox(&page.rows);
    assert!(checkbox.is_active(), "the control album starts selected");
    let original = checkbox.as_ptr();

    page.state.refresh();
    drain_main_context();
    let rebound = realized_album_checkbox(&page.rows);
    assert_eq!(
        rebound.as_ptr(),
        original,
        "rebinding must reuse the header widgets built during setup"
    );
    assert_eq!(page.state.selection_requests.get(), 0);

    let pushes_before = page.state.album_headers.push_count();
    page.state.session.borrow_mut().none();
    page.state.apply_selection(true);
    drain_main_context();
    let pushed = realized_album_checkbox(&page.rows);
    assert_eq!(pushed.as_ptr(), original);
    assert!(
        !pushed.is_active(),
        "apply_selection must push the new state"
    );
    assert!(
        page.state.album_headers.push_count() > pushes_before,
        "apply_selection must explicitly push the affected album state"
    );
    assert_eq!(
        page.state.selection_requests.get(),
        0,
        "neither rebind nor push may look like a user selection"
    );
}

fn realized_album_checkbox(rows: &gtk4::ListView) -> gtk4::CheckButton {
    let mut pending = vec![rows.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = pending.pop() {
        if widget.has_css_class("doctor-album-check") {
            return widget
                .downcast::<gtk4::CheckButton>()
                .expect("album check css class belongs to a check button");
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    panic!("missing realized album checkbox")
}

fn drain_main_context() {
    while glib::MainContext::default().iteration(false) {}
}
