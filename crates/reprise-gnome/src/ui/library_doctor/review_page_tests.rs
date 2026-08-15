use std::path::PathBuf;

use reprise_core::library_doctor::{DoctorField, DoctorValue, LibraryDoctor, ProblemClass};

use super::super::review_row::contract_tests::{
    album_change_scan, conflict_scan, ready_and_stale_scan, scan,
    seed_ready_and_stale_badge_fixture, stale_album_scan, three_album_scan,
};
use super::*;

#[test]
fn doc_9b_one_column_header_serves_the_whole_page() {
    let header = include_str!("review_header.rs");
    assert!(header.contains("DOCTOR_TRACK"));
    assert!(header.contains("DOCTOR_FIELD"));
    assert!(header.contains("DOCTOR_CURRENT"));
    assert!(header.contains("DOCTOR_PROPOSED"));
    assert!(header.contains("DOCTOR_SOURCE"));
}

#[test]
fn doc_7c_the_review_page_carries_no_provider_toggle() {
    let source = include_str!("review_page.rs");

    assert!(!source.contains("remote_suggestions_row_for"));
    assert!(!source.contains("PreferencesGroup"));
    assert!(!source.contains("options_clamp"));
}

#[test]
fn review_widget_handlers_hold_review_state_weakly() {
    // Whitespace-insensitive on purpose: an assertion that pins exact indentation
    // breaks on the next rustfmt reflow while proving nothing about behaviour.
    // What matters is that neither handler captures the state strongly.
    let source = include_str!("review_page.rs").replace([' ', '\n'], "");

    assert!(source.contains("select_all.connect_toggled(glib::clone!(#[weak]state,"));
    assert!(
        source.contains("apply.connect_clicked(glib::clone!(#[weak(rename_to=state)]self.state,")
    );
    // `callback_state` on its own is fine — the filter bar uses it legitimately.
    assert!(!source.contains("letcallback_state=state.clone();lethandler=header.select_all"));
    assert!(!source.contains("letstate=self.state.clone();self.state.apply.connect_clicked"));
}

#[test]
fn doc_9b_every_reviewable_row_starts_selected() {
    let mut source = scan();
    let mut capped = source.proposals[0].clone();
    capped.field = DoctorField::Title;
    capped.confidence = 49;
    capped.never_preselect = true;
    source.proposals.push(capped);

    let session = DoctorReviewSession::from_scan(source, DoctorReviewFilter::NeedsReview);

    assert!(session
        .rows()
        .iter()
        .filter(|row| !row.never_preselect)
        .all(|row| row.selected));
    assert!(
        !session
            .rows()
            .iter()
            .find(|row| row.never_preselect)
            .unwrap()
            .selected
    );
}

#[test]
fn doc_9b_footer_counts_the_changes_that_will_be_written() {
    let session = DoctorReviewSession::from_scan(scan(), DoctorReviewFilter::NeedsReview);
    assert_eq!(session.summary().tag_change_count, 1);
    assert_eq!(strings::doctor_apply_changes(1), "Apply 1 fix");
}

#[test]
fn doc_9d_the_footer_states_the_scope_of_the_filter() {
    let summary = reprise_core::library_doctor::DoctorReviewSummary {
        track_count: 20,
        file_count: 20,
        tag_change_count: 27,
        total_tag_change_count: 390,
    };

    assert_eq!(
        review_footer_summary(summary, Some(ReviewCategory::Year), 433),
        "27 of 390 · filtered by Year"
    );
}

#[test]
fn doc_9b_the_unfiltered_footer_names_selection_and_ready_inventory() {
    let summary = reprise_core::library_doctor::DoctorReviewSummary {
        track_count: 304,
        file_count: 304,
        tag_change_count: 419,
        total_tag_change_count: 419,
    };

    assert_eq!(
        review_footer_summary(summary, None, 433),
        "419 of 433 selected · 304 files · undo available after"
    );
}

/// The header describes the page, the footer describes the decision. With
/// everything selected the two agree; unchecking a row moves the footer and
/// the Apply button down and leaves the header where it is, because the row
/// is still on screen.
#[test]
fn doc_9d_the_header_counts_the_inventory_while_the_footer_counts_the_selection() {
    let scan = album_change_scan();
    let mut session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);

    let rows = grouped_rows_for(&scan, &session, &HashMap::new());
    assert_eq!(review_header_counts(&rows), (14, 1));
    assert_eq!(session.summary().tag_change_count, 14);

    session.none();
    let rows = grouped_rows_for(&scan, &session, &HashMap::new());
    assert_eq!(
        review_header_counts(&rows),
        (14, 1),
        "unchecking rows removes nothing from the page"
    );
    assert_eq!(session.summary().tag_change_count, 0);
}

#[test]
fn doc_8a_the_badge_and_unfiltered_review_header_count_the_same_ready_fixes() {
    let db = crate::test_db::open().unwrap();
    seed_ready_and_stale_badge_fixture(&db);
    let scan = ready_and_stale_scan();
    let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
    let rows = grouped_rows_for(&scan, &session, &HashMap::new());

    let badge = reprise_core::queries::count_pending_doctor_findings(&db).unwrap();
    let (header, _) = review_header_counts(&rows);

    assert_eq!(usize::try_from(badge).unwrap(), header);
    assert_eq!(header, 1);
}

#[test]
fn doc_9b_stale_notice_follows_category_filter_and_is_hidden_at_zero() {
    let mut fixture = ready_and_stale_scan();
    fixture.proposals[0].problem_class = ProblemClass::MissingWrongYear;
    let mut session = DoctorReviewSession::from_scan(fixture, DoctorReviewFilter::NeedsReview);

    assert_eq!(
        review_stale_notice(&session),
        Some("1 fix is out of date — this file changed after the scan.".to_owned())
    );
    session.set_category_filter(Some(ReviewCategory::Year.problem_classes()));
    assert_eq!(
        review_stale_notice(&session),
        None,
        "a stale Genre fix is outside the active Year category"
    );
    assert_eq!(
        review_stale_notice(&DoctorReviewSession::from_scan(
            scan(),
            DoctorReviewFilter::NeedsReview
        )),
        None
    );

    let source = include_str!("review_page.rs");
    let filter = source
        .find("page_content.append(&state.filter_bar.root)")
        .unwrap();
    let notice = source
        .find("page_content.append(&state.stale_notice)")
        .unwrap();
    let header = source.find("page_content.append(&header.root)").unwrap();
    assert!(filter < notice && notice < header);
    assert!(source.contains("self.state.rescan.set_sensitive(!running)"));
}

/// A filter does change what is on screen, so the header follows it — and
/// with everything inside the filter selected, header, footer and button all
/// name the same number.
#[test]
fn doc_9d_a_filtered_header_counts_only_the_filtered_rows() {
    let scan = album_change_scan();
    let mut session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
    session.set_category_filter(Some(ReviewCategory::Year.problem_classes()));

    let rows = grouped_rows_for(&scan, &session, &HashMap::new())
        .into_iter()
        .filter(|row| session.category_filter_matches(row.row.problem_class))
        .collect::<Vec<_>>();

    assert_eq!(review_header_counts(&rows), (1, 1));
    assert_eq!(session.summary().tag_change_count, 1);
    assert_eq!(strings::doctor_apply_changes(1), "Apply 1 fix");
}

#[test]
fn doc_9b_the_album_pill_counts_written_changes_not_display_rows() {
    let scan = album_change_scan();
    let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
    let rows = grouped_rows_for(&scan, &session, &HashMap::new());
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter()
            .map(|row| row.selected_change_count)
            .sum::<usize>(),
        14
    );
}

#[test]
fn doc_9b_conflicts_sit_at_the_end_and_skip_all_clears_them() {
    let source = include_str!("review_page.rs");
    assert!(source.contains("store.append(&panel.root)"));
    let mut session =
        DoctorReviewSession::from_scan(conflict_scan(), DoctorReviewFilter::NeedsReview);
    let group = session.groups()[0].clone();
    session
        .choose_candidate(group.id, &group.candidates[0].value)
        .unwrap();
    assert!(!session.rows().is_empty());
    session.clear_group_choices();
    assert!(session.rows().is_empty());
    assert!(session.groups().iter().all(|group| group.chosen.is_none()));
}

#[test]
fn doc_9b_the_conflicts_panel_is_the_last_row_of_the_scrolled_list() {
    let source = include_str!("review_page.rs");

    assert!(source.contains("store.append(&panel.root)"));
    assert!(!source.contains("page_content.append(&state.conflicts)"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9b_the_conflicts_panel_covers_no_row() {
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder()
        .default_width(900)
        .default_height(700)
        .build();
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    let page = LibraryDoctorReviewPage::new(
        &conn,
        &parent,
        &conflict_scan(),
        Rc::new(|_| {}),
        Rc::new(|| {}),
        &on_edit,
    );
    let group = page.state.session.borrow().groups()[0].clone();
    page.state
        .session
        .borrow_mut()
        .choose_candidate(group.id, &group.candidates[0].value)
        .unwrap();
    page.state.refresh();
    parent.set_content(Some(page.navigation_page()));
    parent.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let panel = descendant_with_css_class(&page.rows.clone().upcast(), "doctor-conflicts-dashed")
        .expect("conflicts panel must be realized inside the ListView");
    let panel_bounds = panel
        .compute_bounds(&page.rows)
        .expect("conflicts panel must share the list coordinate space");
    let panel_top = panel_bounds.y();
    let row_bottom = descendants_with_css_class(&page.rows.clone().upcast(), "doctor-review-row")
        .into_iter()
        .filter_map(|widget| widget.compute_bounds(&page.rows))
        .map(|bounds| bounds.y() + bounds.height())
        .fold(0.0_f32, f32::max);

    assert!(
        row_bottom <= panel_top,
        "conflicts panel starts at {panel_top}px but a review row reaches {row_bottom}px"
    );
}

fn descendant_with_css_class(root: &gtk4::Widget, class: &str) -> Option<gtk4::Widget> {
    descendants_with_css_class(root, class).into_iter().next()
}

fn descendants_with_css_class(root: &gtk4::Widget, class: &str) -> Vec<gtk4::Widget> {
    let mut matches = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if widget.has_css_class(class) {
            matches.push(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    matches
}

#[test]
fn doc_8a_skip_all_marks_the_scan_reviewed() {
    let db = crate::test_db::open().unwrap();
    let outcome = LibraryDoctor::new(&db)
        .scan_local(
            &reprise_core::library_doctor::LocalScanRequest {
                scope: reprise_core::library_doctor::DoctorScopeRequest::WholeLibrary,
            },
            |_| reprise_core::library_doctor::ScanControl::Continue,
        )
        .unwrap();
    let reprise_core::library_doctor::DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("empty scan must complete")
    };
    acknowledge_skipped_scan(&db, scan.id).unwrap();
    assert_eq!(
        LibraryDoctor::new(&db).reviewed_scan_id().unwrap(),
        Some(scan.id)
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9b_review_groups_render_one_header_per_album() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder().build();
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    let page = LibraryDoctorReviewPage::new(
        &conn,
        &parent,
        &three_album_scan(),
        Rc::new(|_| {}),
        Rc::new(|| {}),
        &on_edit,
    );
    assert_eq!(page.state.sorted.n_items(), 3);
    assert_eq!(page.state.sorted.section(0), (0, 1));
    assert_eq!(page.state.sorted.section(1), (1, 2));
    assert_eq!(page.state.sorted.section(2), (2, 3));
    assert!(page.rows.header_factory().is_some());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9b_the_first_row_carries_its_album_header() {
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder()
        .default_width(900)
        .default_height(700)
        .build();
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    let page = LibraryDoctorReviewPage::new(
        &conn,
        &parent,
        &three_album_scan(),
        Rc::new(|_| {}),
        Rc::new(|| {}),
        &on_edit,
    );
    parent.set_content(Some(page.navigation_page()));
    parent.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let header =
        descendant_with_css_class(&page.rows.clone().upcast(), "doctor-album-header-first")
            .expect("the first realized review row must be preceded by its album header");
    assert!(
        descendant_label_text(&header)
            .iter()
            .any(|label| label == "Album"),
        "the first header must name the first album"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_3c_an_album_with_nothing_selectable_binds_an_insensitive_header_check() {
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder()
        .default_width(900)
        .default_height(700)
        .build();
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    let page = LibraryDoctorReviewPage::new(
        &conn,
        &parent,
        &stale_album_scan(),
        Rc::new(|_| {}),
        Rc::new(|| {}),
        &on_edit,
    );
    parent.set_content(Some(page.navigation_page()));
    parent.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let headers = ["doctor-album-header-first", "doctor-album-header-later"]
        .into_iter()
        .flat_map(|class| descendants_with_css_class(&page.rows.clone().upcast(), class))
        .collect::<Vec<_>>();
    let header = |album: &str| {
        headers
            .iter()
            .find(|header| {
                descendant_label_text(header)
                    .iter()
                    .any(|label| label == album)
            })
            .unwrap_or_else(|| panic!("missing realized header for {album}"))
    };
    let checkbox = |root: &gtk4::Widget| {
        root.first_child()
            .and_downcast::<gtk4::CheckButton>()
            .expect("album header begins with its checkbox")
    };
    let ready = header("Ready album");
    let stale = header("Stale album");

    assert!(checkbox(ready).is_sensitive());
    assert!(
        !checkbox(stale).is_sensitive(),
        "an album with no selectable changes must not expose an active checkbox"
    );
    let stale_labels = descendant_label_text(stale);
    assert!(stale_labels
        .iter()
        .any(|label| label == "1 change · out of date"));
    assert!(!stale_labels.iter().any(|label| label == "0 changes"));
    assert_eq!(
        stale.tooltip_text().as_deref(),
        Some("This file changed after the scan — scan again to include this fix.")
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9b_activating_an_unselectable_row_selects_nothing() {
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder().build();
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    let page = LibraryDoctorReviewPage::new(
        &conn,
        &parent,
        &stale_album_scan(),
        Rc::new(|_| {}),
        Rc::new(|| {}),
        &on_edit,
    );
    let position_for = |state| {
        (0..page.state.selection.n_items())
            .find(|position| {
                let object = page
                    .state
                    .selection
                    .item(*position)
                    .and_downcast::<glib::BoxedAnyObject>()
                    .unwrap();
                let row_state = object.borrow::<ReviewRowModel>().row.state;
                row_state == state
            })
            .unwrap_or_else(|| panic!("missing {state:?} row"))
    };
    let selections = || {
        page.state
            .session
            .borrow()
            .rows()
            .iter()
            .map(|row| (row.id, row.selected))
            .collect::<Vec<_>>()
    };
    let churn = Rc::new(Cell::new(0_u32));
    page.state.store.connect_items_changed({
        let churn = churn.clone();
        move |_, _, removed, added| churn.set(churn.get() + removed + added)
    });
    let before = selections();

    page.state
        .toggle_position(position_for(DoctorReviewRowState::Stale));

    assert_eq!(
        churn.get(),
        0,
        "an activation the page refuses must not rebuild its store"
    );
    assert_eq!(selections(), before);

    let ready_position = position_for(DoctorReviewRowState::Ready);
    let ready_id = page
        .state
        .selection
        .item(ready_position)
        .and_downcast::<glib::BoxedAnyObject>()
        .map(|object| object.borrow::<ReviewRowModel>().row.id)
        .unwrap();
    let ready_before = page
        .state
        .session
        .borrow()
        .rows()
        .iter()
        .find(|row| row.id == ready_id)
        .unwrap()
        .selected;

    page.state.toggle_position(ready_position);

    assert_eq!(
        page.state
            .session
            .borrow()
            .rows()
            .iter()
            .find(|row| row.id == ready_id)
            .unwrap()
            .selected,
        !ready_before,
        "a selectable row activation must still flip its selection"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9b_every_section_boundary_binds_a_non_empty_header() {
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder()
        .default_width(900)
        .default_height(700)
        .build();
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    let mut source = three_album_scan();
    source.proposals[1].field = DoctorField::Year;
    source.proposals[1].current = DoctorValue::Year(2020);
    source.proposals[1].proposed = DoctorValue::Year(2021);
    source.proposals[1].problem_class = ProblemClass::MissingWrongYear;
    source.proposals[2].field = DoctorField::Title;
    source.proposals[2].current = DoctorValue::Text("Track 9".into());
    source.proposals[2].proposed = DoctorValue::Text("Track Nine".into());
    source.proposals[2].problem_class = ProblemClass::CasingWhitespace;
    let page = LibraryDoctorReviewPage::new(
        &conn,
        &parent,
        &source,
        Rc::new(|_| {}),
        Rc::new(|| {}),
        &on_edit,
    );
    let empty_bindings = Rc::new(RefCell::new(Vec::new()));
    let observed_bindings = Rc::new(Cell::new(0_u32));
    let headers = Rc::new(RefCell::new(Vec::<gtk4::ListHeader>::new()));
    let factory = page
        .rows
        .header_factory()
        .unwrap()
        .downcast::<gtk4::SignalListItemFactory>()
        .unwrap();
    factory.connect_bind({
        let empty_bindings = empty_bindings.clone();
        let observed_bindings = observed_bindings.clone();
        let headers = headers.clone();
        move |_, object| {
            let header = object.downcast_ref::<gtk4::ListHeader>().unwrap();
            observed_bindings.set(observed_bindings.get() + 1);
            headers.borrow_mut().push(header.clone());
            if header.child().is_none() {
                tracing::warn!(
                    start = header.start(),
                    end = header.end(),
                    "DOC-9b section boundary bound while its model row was unavailable"
                );
                empty_bindings
                    .borrow_mut()
                    .push((header.start(), header.end()));
            }
        }
    });
    parent.set_content(Some(page.navigation_page()));
    parent.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_current_headers(&page, &headers.borrow());

    page.state.set_category(Some(ReviewCategory::Year));
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_current_headers(&page, &headers.borrow());
    page.state.refresh();
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_current_headers(&page, &headers.borrow());
    page.state.set_category(None);
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_current_headers(&page, &headers.borrow());

    assert!(
        observed_bindings.get() >= 3,
        "expected one bound header for each of the three album sections, observed {}",
        observed_bindings.get()
    );
    assert!(
        empty_bindings.borrow().is_empty(),
        "section boundaries bound empty at {:?}",
        empty_bindings.borrow()
    );
}

fn assert_current_headers(page: &LibraryDoctorReviewPage, headers: &[gtk4::ListHeader]) {
    let mut position = 0;
    while position < page.state.sorted.n_items() {
        let section = page.state.sorted.section(position);
        let row = row_at(&page.state.sorted, section.0).unwrap();
        let header = headers
            .iter()
            .rev()
            .find(|header| header.start() == section.0 && header.end() == section.1)
            .unwrap_or_else(|| {
                panic!(
                    "section boundary {}..{} has no bound ListHeader",
                    section.0, section.1
                )
            });
        let child = header.child().unwrap_or_else(|| {
            tracing::warn!(
                start = section.0,
                end = section.1,
                album = row.album_title,
                "DOC-9b current section boundary has an empty header"
            );
            panic!(
                "section boundary {}..{} for {:?} has no child",
                section.0, section.1, row.album_title
            )
        });
        let labels = descendant_label_text(&child);
        assert!(
            labels.iter().any(|label| label == &row.album_title),
            "section boundary {}..{} expected album {:?}, found labels {:?}",
            section.0,
            section.1,
            row.album_title,
            labels
        );
        position = section.1;
    }
}

fn descendant_label_text(root: &gtk4::Widget) -> Vec<String> {
    let mut labels = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            labels.push(label.text().to_string());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    labels
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_3b_review_page_virtualizes_rows_without_horizontal_scroll() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder().build();
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    let page = LibraryDoctorReviewPage::new(
        &conn,
        &parent,
        &scan(),
        Rc::new(|_| {}),
        Rc::new(|| {}),
        &on_edit,
    );
    let scrolled = page
        .state
        .content
        .child_by_name("rows")
        .and_downcast::<gtk4::ScrolledWindow>()
        .unwrap();
    assert_eq!(scrolled.hscrollbar_policy(), gtk4::PolicyType::Never);
    assert!(scrolled.child().unwrap().is::<gtk4::ListView>());
    page.mark_paths_stale(&[PathBuf::from("/tmp/doctor-review.flac")]);
    assert_eq!(
        page.state.session.borrow().rows()[0].state,
        DoctorReviewRowState::Stale
    );
}

#[test]
fn doc_5d_write_outcomes_preserve_honest_review_state() {
    assert_eq!(
        outcome_transition(DoctorWriteRowState::Applied).selected,
        Some(false)
    );
    assert_eq!(
        outcome_transition(DoctorWriteRowState::Conflict).review_state,
        Some(DoctorReviewRowState::Conflict)
    );
    assert_eq!(
        outcome_transition(DoctorWriteRowState::Unavailable).review_state,
        Some(DoctorReviewRowState::Stale)
    );
}
