use std::path::PathBuf;

use reprise_core::library::tag_edit::EditableTags;
use reprise_core::library_doctor::{
    DoctorCandidate, DoctorField, DoctorGroupMember, DoctorProposal, DoctorScanOptions,
    DoctorTrackRef, DoctorTrackSnapshot, DoctorUnresolvedGroup, DoctorValue, ProblemClass,
    ProposalSource,
};

use super::*;

fn scan() -> DoctorScan {
    DoctorScan {
        id: 1,
        scope_kind: "whole_library".into(),
        created_at: 2,
        options: DoctorScanOptions::local_only(),
        checked_tracks: 1,
        skipped_tracks: 0,
        track_ids: vec![7],
        tracks: vec![DoctorTrackSnapshot {
            reference: DoctorTrackRef {
                track_id: 7,
                path: PathBuf::from("/tmp/doctor-review.flac"),
                file_mtime: 1,
                file_size: 2,
                device: None,
                inode: None,
            },
            tags: Some(EditableTags {
                title: "Review track".into(),
                artist: "Artist".into(),
                album: "Album".into(),
                album_artist: "Artist".into(),
                year: Some(2020),
                track_no: Some(1),
                genre: "Rock".into(),
            }),
            stale: false,
        }],
        proposals: vec![DoctorProposal {
            track_id: 7,
            field: DoctorField::Genre,
            current: DoctorValue::Text("Rock".into()),
            proposed: DoctorValue::Text("Alternative".into()),
            source: ProposalSource::MusicBrainz,
            confidence: 90,
            preselected: false,
            never_preselect: false,
            problem_class: ProblemClass::GenreVariant,
            resolved_release_mbid: None,
            evidence: Vec::new(),
            local_fallback: None,
        }],
        unresolved_groups: Vec::new(),
    }
}

fn three_album_scan() -> DoctorScan {
    let mut scan = scan();
    scan.track_ids = vec![7, 8, 9];
    for (track_id, album) in [(8, "Second"), (9, "Third")] {
        let mut track = scan.tracks[0].clone();
        track.reference.track_id = track_id;
        track.reference.path = PathBuf::from(format!("/tmp/doctor-review-{track_id}.flac"));
        track.tags.as_mut().unwrap().album = album.into();
        track.tags.as_mut().unwrap().title = format!("Track {track_id}");
        scan.tracks.push(track);
        let mut proposal = scan.proposals[0].clone();
        proposal.track_id = track_id;
        scan.proposals.push(proposal);
    }
    scan.checked_tracks = 3;
    scan
}

fn album_change_scan() -> DoctorScan {
    let template = scan();
    let mut scan = template.clone();
    scan.track_ids.clear();
    scan.tracks.clear();
    scan.proposals.clear();
    for track_id in 1..=11 {
        let mut track = template.tracks[0].clone();
        track.reference.track_id = track_id;
        track.reference.path = PathBuf::from(format!("/tmp/album-{track_id}.flac"));
        let tags = track.tags.as_mut().unwrap();
        tags.title = format!("Track {track_id}");
        tags.album = "One album".into();
        tags.album_artist = "Artists".into();
        scan.track_ids.push(track_id);
        scan.tracks.push(track);
        scan.proposals.push(DoctorProposal {
            track_id,
            field: DoctorField::AlbumArtist,
            current: DoctorValue::Text("Artists".into()),
            proposed: DoctorValue::Text("Artist".into()),
            source: ProposalSource::MusicBrainz,
            confidence: 90,
            preselected: false,
            never_preselect: false,
            problem_class: ProblemClass::MissingAlbumArtist,
            resolved_release_mbid: None,
            evidence: Vec::new(),
            local_fallback: None,
        });
    }
    for (track_id, field, current, proposed, problem_class) in [
        (
            1,
            DoctorField::Title,
            DoctorValue::Text("Track 1".into()),
            DoctorValue::Text("First track".into()),
            ProblemClass::CasingWhitespace,
        ),
        (
            2,
            DoctorField::Genre,
            DoctorValue::Text("Rock".into()),
            DoctorValue::Text("Alternative".into()),
            ProblemClass::GenreVariant,
        ),
        (
            3,
            DoctorField::Year,
            DoctorValue::Year(2020),
            DoctorValue::Year(2021),
            ProblemClass::MissingWrongYear,
        ),
    ] {
        scan.proposals.push(DoctorProposal {
            track_id,
            field,
            current,
            proposed,
            source: ProposalSource::MusicBrainz,
            confidence: 90,
            preselected: false,
            never_preselect: false,
            problem_class,
            resolved_release_mbid: None,
            evidence: Vec::new(),
            local_fallback: None,
        });
    }
    scan.checked_tracks = 11;
    scan
}

fn conflict_scan() -> DoctorScan {
    let mut scan = scan();
    scan.proposals.clear();
    scan.unresolved_groups = vec![DoctorUnresolvedGroup {
        field: DoctorField::Genre,
        group_key: "genre".into(),
        candidates: vec![
            DoctorCandidate {
                value: DoctorValue::Text("Rock".into()),
                count: 1,
                evidence: Vec::new(),
            },
            DoctorCandidate {
                value: DoctorValue::Text("rock".into()),
                count: 1,
                evidence: Vec::new(),
            },
        ],
        members: vec![DoctorGroupMember {
            track_id: 7,
            current: DoctorValue::Text("ROCK".into()),
        }],
        local_fallback: None,
    }];
    scan
}

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
    assert_eq!(strings::doctor_apply_changes(1), "Apply 1 change");
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
