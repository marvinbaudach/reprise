//! What the result page actually renders, measured on a presented window.
//!
//! The model tests in `summary_model.rs` decide *what* should be on the page.
//! These prove the widgets agree: three cards, the action inline on the right,
//! the column flush left and capped, and a running scan that never shows any of
//! it.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AdwApplicationWindowExt;
use reprise_core::library::tag_edit::EditableTags;
use reprise_core::library_doctor::{
    DoctorField, DoctorProposal, DoctorScan, DoctorScanOptions, DoctorScanSummary, DoctorTrackRef,
    DoctorTrackSnapshot, DoctorUnresolvedGroup, DoctorValue, DoctorWriteReport, DoctorWriteRow,
    DoctorWriteRowState, ProblemClass, ProposalSource,
};

use super::progress_card::DoctorJobKind;
use super::summary_page::LibraryDoctorPage;

const WINDOW_WIDTH: i32 = 1240;
const WINDOW_HEIGHT: i32 = 780;
/// The mockup's content column: `max-width: 700px` inside `padding: … 64px`.
const COLUMN_CAP: f32 = 700.0;
const LEFT_EDGE_TOLERANCE: f32 = 120.0;

fn track(track_id: i64, artist: &str) -> DoctorTrackSnapshot {
    DoctorTrackSnapshot {
        reference: DoctorTrackRef {
            track_id,
            path: format!("/tmp/doctor-summary-{track_id}.flac").into(),
            file_mtime: 1,
            file_size: 2,
            device: None,
            inode: None,
        },
        tags: Some(EditableTags {
            title: format!("Track {track_id}"),
            artist: artist.into(),
            album: "Album".into(),
            album_artist: artist.into(),
            year: Some(2020),
            track_no: Some(1),
            genre: "Rock".into(),
        }),
        stale: false,
    }
}

/// `preselected` is what puts a local finding in the quiet tier; an unselected
/// local one is a review finding that survives a scan whose remote lookups are
/// not being shown.
fn proposal(
    track_id: i64,
    source: ProposalSource,
    class: ProblemClass,
    preselected: bool,
) -> DoctorProposal {
    DoctorProposal {
        track_id,
        field: DoctorField::Artist,
        current: DoctorValue::Text("old".into()),
        proposed: DoctorValue::Text("new".into()),
        source,
        confidence: 92,
        preselected,
        never_preselect: false,
        problem_class: class,
        evidence: Vec::new(),
        local_fallback: None,
    }
}

/// A scan with all three blocks populated: something quietly applied, something
/// to review, and one spelling conflict.
fn scan_with_everything() -> DoctorScan {
    DoctorScan {
        id: 1,
        scope_kind: "whole_library".into(),
        created_at: 2,
        options: DoctorScanOptions::local_only(),
        checked_tracks: 27,
        skipped_tracks: 0,
        track_ids: vec![1, 2, 3],
        tracks: vec![
            track(1, "Artist"),
            track(2, "Artist"),
            track(3, "Artist Two"),
        ],
        proposals: vec![
            proposal(
                1,
                ProposalSource::Local,
                ProblemClass::CasingWhitespace,
                true,
            ),
            proposal(
                2,
                ProposalSource::Local,
                ProblemClass::MissingWrongYear,
                false,
            ),
        ],
        unresolved_groups: vec![DoctorUnresolvedGroup {
            field: DoctorField::Artist,
            group_key: "artist".into(),
            candidates: Vec::new(),
            members: Vec::new(),
            local_fallback: None,
        }],
    }
}

fn quiet_report() -> DoctorWriteReport {
    DoctorWriteReport {
        job_id: 4,
        source_job_id: None,
        updated_tracks: 1,
        cancelled_tracks: 0,
        failed_tracks: 0,
        conflict_tracks: 0,
        unavailable_tracks: 0,
        rows: vec![DoctorWriteRow {
            row_id: None,
            track_id: 1,
            path: "/tmp/doctor-summary-1.flac".into(),
            field: DoctorField::Artist,
            expected: DoctorValue::Text("old".into()),
            proposed: DoctorValue::Text("new".into()),
            state: DoctorWriteRowState::Applied,
            file_written: true,
            error_kind: None,
            error: None,
        }],
    }
}

/// Measuring an allocation means the state has to be settled *before* the first
/// present: switching the stack afterwards leaves the previous page's size in
/// place until another frame is drawn, and a main-context spin does not draw
/// one.
fn presented_page_showing(
    setup: impl FnOnce(&LibraryDoctorPage),
) -> (adw::ApplicationWindow, Rc<LibraryDoctorPage>) {
    let db = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder().build();
    let page = LibraryDoctorPage::new(&db, &parent, false, Rc::new(|_| {}));
    setup(&page);
    let navigation = adw::NavigationView::new();
    navigation.add(page.navigation_page());
    let window = adw::ApplicationWindow::builder()
        .default_width(WINDOW_WIDTH)
        .default_height(WINDOW_HEIGHT)
        .build();
    window.set_size_request(WINDOW_WIDTH, WINDOW_HEIGHT);
    window.set_content(Some(&navigation));
    window.present();
    pump();
    (window, page)
}

fn presented_page() -> (adw::ApplicationWindow, Rc<LibraryDoctorPage>) {
    let db = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder().build();
    let page = LibraryDoctorPage::new(&db, &parent, false, Rc::new(|_| {}));
    let navigation = adw::NavigationView::new();
    navigation.add(page.navigation_page());
    let window = adw::ApplicationWindow::builder()
        .default_width(WINDOW_WIDTH)
        .default_height(WINDOW_HEIGHT)
        .build();
    window.set_size_request(WINDOW_WIDTH, WINDOW_HEIGHT);
    window.set_content(Some(&navigation));
    window.present();
    pump();
    (window, page)
}

fn pump() {
    while gtk4::glib::MainContext::default().iteration(false) {}
}

fn card_children(page: &LibraryDoctorPage) -> Vec<gtk4::Widget> {
    let mut cards = Vec::new();
    let mut child = page.result_cards().first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        cards.push(widget);
    }
    cards
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9a_the_result_page_shows_three_cards_with_the_conflicts_card_quietest() {
    gtk4::init().unwrap();
    let (window, page) = presented_page();
    page.set_scan(Some(scan_with_everything()), true);
    page.complete_auto_apply(Some(quiet_report()));
    pump();

    assert_eq!(page.visible_screen().as_deref(), Some("summary"));
    let cards = card_children(&page);
    assert_eq!(cards.len(), 3, "applied, review, conflicts");

    assert!(cards[0].has_css_class("card"));
    assert!(!cards[0].has_css_class("doctor-card-accent"));
    assert!(
        cards[1].has_css_class("doctor-card-accent"),
        "the review card is the one that carries emphasis"
    );
    assert!(
        !cards[2].has_css_class("card"),
        "the conflicts card has no fill at all"
    );
    assert!(cards[2].has_css_class("doctor-card-dashed"));

    // The two filled cards keep their action inline at the trailing edge; the
    // conflicts card has no action at all.
    assert_eq!(
        page.undo_button().parent().and_then(|row| row.parent()),
        Some(cards[0].clone())
    );
    assert_eq!(
        page.review_button().parent().and_then(|row| row.parent()),
        Some(cards[1].clone())
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9a_the_result_column_is_flush_left_and_capped() {
    gtk4::init().unwrap();
    let (window, page) = presented_page_showing(|page| {
        page.set_scan(Some(scan_with_everything()), true);
        page.complete_auto_apply(Some(quiet_report()));
    });

    assert_eq!(page.visible_screen().as_deref(), Some("summary"));
    let bounds = page
        .result_cards()
        .compute_bounds(&window)
        .expect("the cards must be laid out");
    assert!(
        bounds.x() < LEFT_EDGE_TOLERANCE,
        "the column starts at the content edge, not in the middle: x={}",
        bounds.x()
    );
    assert!(
        bounds.width() <= COLUMN_CAP + 1.0,
        "the column stays capped: width={}",
        bounds.width()
    );
    assert!(
        bounds.width() > COLUMN_CAP / 2.0,
        "…but it is not squeezed to its content either: width={}, window={}x{}",
        bounds.width(),
        window.width(),
        window.height()
    );
    // Top-weighted: the cards start in the upper part of an 780px window rather
    // than floating in its vertical centre.
    assert!(
        bounds.y() < f32::from(u16::try_from(WINDOW_HEIGHT / 3).unwrap()),
        "the page is top-weighted: y={}",
        bounds.y()
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_2c_a_running_scan_shows_progress_and_no_result_vocabulary() {
    gtk4::init().unwrap();
    let (window, page) = presented_page();
    // A previous result exists — it must not leak into the running screen.
    page.set_scan(Some(scan_with_everything()), true);
    page.complete_auto_apply(Some(quiet_report()));
    pump();
    assert_eq!(page.visible_screen().as_deref(), Some("summary"));

    page.begin_job(DoctorJobKind::Scan, 1648);
    let mut live = DoctorScanSummary::default();
    live.auto_applied_changes = 511;
    live.review_changes = 39;
    page.set_live_summary(live);
    page.update_job(DoctorJobKind::Scan, 742, 1648);
    pump();

    assert_eq!(
        page.visible_screen().as_deref(),
        Some("running"),
        "in-progress and final are two different pages"
    );
    assert!(!page.scan_again_button().is_mapped());
    assert!(!page.review_button().is_mapped());
    assert!(!page.undo_button().is_mapped());

    page.complete_auto_apply(Some(quiet_report()));
    pump();
    assert_eq!(page.visible_screen().as_deref(), Some("summary"));
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9a_a_clean_library_gets_the_empty_state_not_three_empty_blocks() {
    gtk4::init().unwrap();
    let (window, page) = presented_page();
    let mut clean = scan_with_everything();
    clean.proposals.clear();
    clean.unresolved_groups.clear();
    page.set_scan(Some(clean), false);
    pump();

    assert_eq!(page.visible_screen().as_deref(), Some("result"));
    assert_eq!(card_children(&page).len(), 0);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9a_undo_is_dead_until_there_is_something_to_undo() {
    gtk4::init().unwrap();
    let (window, page) = presented_page();
    page.set_scan(Some(scan_with_everything()), false);
    page.complete_auto_apply(None);
    pump();
    assert!(
        !page.undo_button().is_sensitive(),
        "no cleanup was recorded, so Undo has nothing to revert"
    );

    page.complete_auto_apply(Some(quiet_report()));
    pump();
    assert!(
        page.undo_button().is_sensitive(),
        "the quiet job wrote a row, so Undo is live"
    );

    page.mark_reverted();
    pump();
    assert_eq!(
        card_children(&page).len(),
        2,
        "after a revert the applied card is gone; review and conflicts stay"
    );
    window.close();
}
