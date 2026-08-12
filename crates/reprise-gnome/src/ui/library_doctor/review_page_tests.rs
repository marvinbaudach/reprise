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

fn ready_and_stale_scan() -> DoctorScan {
    let mut scan = scan();
    let mut stale_track = scan.tracks[0].clone();
    stale_track.reference.track_id = 8;
    stale_track.reference.path = PathBuf::from("/tmp/doctor-review-stale.flac");
    stale_track.stale = true;
    scan.track_ids.push(8);
    scan.tracks.push(stale_track);
    let mut stale_proposal = scan.proposals[0].clone();
    stale_proposal.track_id = 8;
    scan.proposals.push(stale_proposal);
    scan.checked_tracks = 2;
    scan
}

fn seed_ready_and_stale_badge_fixture(db: &Db) {
    let conn = crate::test_db::connection(db);
    conn.execute(
        "INSERT INTO library_doctor_scans \
             (id, scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
             VALUES (1, 'whole_library', 2, 0, 2, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE library_doctor_state SET last_complete_scan_id=1 WHERE singleton=1",
        [],
    )
    .unwrap();
    for (position, track_id, path, mtime) in [
        (0, 7, "/tmp/doctor-review.flac", 1),
        (1, 8, "/tmp/doctor-review-stale.flac", 2),
    ] {
        conn.execute(
            "INSERT INTO tracks (id, path, title, added_at, file_mtime, file_size) \
                 VALUES (?1, ?2, 'Review track', 0, ?3, 2)",
            rusqlite::params![track_id, path, mtime],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_doctor_scan_tracks \
                 (scan_id, position, track_id, path, file_mtime, file_size, read_ok, \
                  title, artist, album, album_artist, year, track_no, genre) \
                 VALUES (1, ?1, ?2, ?3, 1, 2, 1, 'Review track', 'Artist', 'Album', \
                         'Artist', 2020, 1, 'Rock')",
            rusqlite::params![position, track_id, path],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_doctor_proposals \
                 (scan_id, position, track_id, field, current_value, proposed_value, source, \
                  confidence, preselected, problem_class, evidence_json, local_fallback_json) \
                 VALUES (1, ?1, ?2, 'genre', 'Rock', 'Alternative', 'musicbrainz', \
                         90, 0, 'genre_variant', '[]', 'null')",
            rusqlite::params![position, track_id],
        )
        .unwrap();
    }
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
