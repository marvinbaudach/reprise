use std::path::PathBuf;

use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::library::tag_edit::EditableTags;
use reprise_core::library_doctor::{
    DoctorCandidate, DoctorField, DoctorGroupMember, DoctorProposal, DoctorReviewRow,
    DoctorReviewRowId, DoctorReviewRowOrigin, DoctorReviewRowState, DoctorScan, DoctorScanOptions,
    DoctorTrackRef, DoctorTrackSnapshot, DoctorUnresolvedGroup, DoctorValue, ProblemClass,
    ProposalSource,
};

use super::super::review_header::ReviewHeader;
use super::super::review_model::{ConfidencePresentation, ReviewRowModel};
use super::{
    apply_album_wide_style, bind, build_row, narrow_prefixed, strike_range, value_label,
    visible_edge_spaces, ConfidenceTone, ReviewLayout, ValueKind,
};

/// A window this wide is an ordinary maximised desktop window. Everything the
/// review row promises has to be readable inside it.
const DESKTOP_WIDTH: i32 = 1760;

pub(in crate::ui::library_doctor) fn scan() -> DoctorScan {
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

pub(in crate::ui::library_doctor) fn three_album_scan() -> DoctorScan {
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

pub(in crate::ui::library_doctor) fn album_change_scan() -> DoctorScan {
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

pub(in crate::ui::library_doctor) fn ready_and_stale_scan() -> DoctorScan {
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

pub(in crate::ui::library_doctor) fn stale_album_scan() -> DoctorScan {
    let mut scan = ready_and_stale_scan();
    scan.tracks[0].tags.as_mut().unwrap().album = "Ready album".into();
    scan.tracks[1].tags.as_mut().unwrap().album = "Stale album".into();
    scan.tracks[1].tags.as_mut().unwrap().title = "Stale track".into();
    scan
}

pub(in crate::ui::library_doctor) fn seed_ready_and_stale_badge_fixture(db: &Db) {
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

pub(in crate::ui::library_doctor) fn conflict_scan() -> DoctorScan {
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

fn row_model(state: DoctorReviewRowState) -> ReviewRowModel {
    let id = DoctorReviewRowId::from_raw(1);
    ReviewRowModel {
        row: DoctorReviewRow {
            id,
            track_id: 7,
            field: DoctorField::Genre,
            current: DoctorValue::Text("Rock".into()),
            proposed: DoctorValue::Text("Alternative".into()),
            source: ProposalSource::MusicBrainz,
            confidence: 90,
            evidence: Vec::new(),
            problem_class: ProblemClass::GenreVariant,
            state,
            never_preselect: false,
            selected: state == DoctorReviewRowState::Ready,
            origin: DoctorReviewRowOrigin::Proposal,
        },
        row_ids: vec![id],
        selectable_row_ids: (state == DoctorReviewRowState::Ready)
            .then_some(id)
            .into_iter()
            .collect(),
        track_ids: vec![7],
        album_position: 0,
        row_position: 0,
        album_key: "album".into(),
        album_title: "Album".into(),
        album_artist: "Artist".into(),
        album_track_count: 1,
        selected_change_count: usize::from(state == DoctorReviewRowState::Ready),
        is_album_wide: false,
        track: "Review track".into(),
        field: "Genre".into(),
        current: "Rock".into(),
        proposed: "Alternative".into(),
        confidence: ConfidencePresentation {
            label: "MusicBrainz · 90%".into(),
            tone: ConfidenceTone::Normal,
            warning: false,
        },
        outcome: None,
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9b_a_stale_row_names_its_reason_where_the_click_happens() {
    if gtk4::init().is_err() {
        return;
    }
    let header = ReviewHeader::new();
    let widgets = build_row(&header.groups);
    let stale = row_model(DoctorReviewRowState::Stale);

    bind(&widgets, &stale, ReviewLayout::Wide);

    assert!(widgets.source.text().contains("Stale"));
    assert_eq!(
        widgets.root.tooltip_text().as_deref(),
        Some("This file changed after the scan — scan again to include this fix.")
    );
    assert!(!widgets.selected.is_sensitive());
    assert!(stale
        .accessible_description()
        .contains("This file changed after the scan — scan again to include this fix."));

    bind(
        &widgets,
        &row_model(DoctorReviewRowState::Ready),
        ReviewLayout::Wide,
    );

    assert!(!widgets.source.text().contains("Stale"));
    assert_eq!(widgets.root.tooltip_text(), None);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_3b_the_column_header_and_rows_share_horizontal_alignment() {
    gtk4::init().unwrap();
    let header = ReviewHeader::new();
    let row = build_row(&header.groups);

    assert_eq!(header.root.margin_start(), 28);
    assert_eq!(header.root.margin_start(), row.root.margin_start());
    assert_eq!(header.root.margin_end(), row.root.margin_end());
}

/// A label that ellipsizes still asks for its whole text unless something caps
/// its natural width. Bound into a horizontal size group, that request becomes
/// the column's width for every row — and the columns to its right leave the
/// page, silently, because the list refuses to scroll sideways.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_3b_a_long_value_does_not_widen_its_column_without_bound() {
    gtk4::init().unwrap();
    let label = value_label(false, super::VALUE_MAX_CHARS);
    label.set_text(&"unreasonably descriptive track title ".repeat(8));

    let (_, natural, _, _) = label.measure(gtk4::Orientation::Horizontal, -1);

    assert!(
        natural < DESKTOP_WIDTH / 3,
        "one value wants {natural}px; three of those plus track, field and \
         source cannot fit a {DESKTOP_WIDTH}px window"
    );
}

/// The regression this pins: with long values in the rows, the shared header
/// grew past the window and Current, Proposed and Source were rendered outside
/// it. The user saw that a year would change but never to what.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_3b_every_column_still_fits_a_desktop_window_with_long_values() {
    gtk4::init().unwrap();
    let header = ReviewHeader::new();
    let widgets = build_row(&header.groups);
    let long = "unreasonably descriptive track title that never ends".repeat(3);
    widgets.track.set_text(&long);
    widgets.field.set_text(&long);
    widgets.current.set_text(&long);
    widgets.proposed.set_text(&long);
    widgets.source.set_text(&long);

    let (_, header_natural, _, _) = header.root.measure(gtk4::Orientation::Horizontal, -1);
    let (_, row_natural, _, _) = widgets.root.measure(gtk4::Orientation::Horizontal, -1);

    assert!(
        header_natural <= DESKTOP_WIDTH,
        "the shared header wants {header_natural}px in a {DESKTOP_WIDTH}px window"
    );
    assert!(
        row_natural <= DESKTOP_WIDTH,
        "a row wants {row_natural}px in a {DESKTOP_WIDTH}px window"
    );
}

#[test]
fn doc_9b_rows_carry_no_caption_labels() {
    let source = include_str!("review_row.rs");

    assert!(!source.contains("value_widgets("));
}

/// Wide rows are named by the shared header above them. Narrow rows have no
/// header — it is hidden below the breakpoint — so the value has to say which
/// column it came from, or the user reads three bare strings in a stack.
#[test]
fn doc_3b_narrow_rows_name_their_values_and_wide_rows_do_not() {
    let wide = narrow_prefixed(ReviewLayout::Wide, ValueKind::Current, "The beatles");
    let narrow = narrow_prefixed(ReviewLayout::Narrow, ValueKind::Current, "The beatles");

    assert_eq!(wide, "The beatles", "the header already names this column");
    assert!(narrow.contains("The beatles"), "the value must survive");
    assert!(
        narrow.len() > wide.len(),
        "the narrow layout adds a prefix: {narrow}"
    );
}

/// Each of the three values gets its own word — a stack of identically
/// prefixed lines would be no better than no prefix at all.
#[test]
fn doc_3b_each_narrow_value_carries_a_distinct_prefix() {
    let current = narrow_prefixed(ReviewLayout::Narrow, ValueKind::Current, "x");
    let proposed = narrow_prefixed(ReviewLayout::Narrow, ValueKind::Proposed, "x");
    let source = narrow_prefixed(ReviewLayout::Narrow, ValueKind::Source, "x");

    assert_ne!(current, proposed);
    assert_ne!(proposed, source);
    assert_ne!(current, source);
}

/// The prefix is a label, not a superseded value. Striking it through would
/// say "Now:" is what changed.
#[test]
fn doc_3b_the_strikethrough_covers_the_value_and_not_its_prefix() {
    let value = "The beatles";
    let rendered = narrow_prefixed(ReviewLayout::Narrow, ValueKind::Current, value);
    let (start, end) = strike_range(&rendered, value);

    assert!(
        start > 0,
        "a prefix precedes the value in the narrow layout"
    );
    assert_eq!(
        &rendered[start as usize..end as usize],
        value,
        "the struck range must be exactly the old value"
    );
}

/// In the wide layout the rendered text *is* the value, so the range covers
/// all of it — the same call site works for both layouts.
#[test]
fn doc_3b_the_strikethrough_covers_a_wide_value_whole() {
    let value = "The beatles";
    let rendered = narrow_prefixed(ReviewLayout::Wide, ValueKind::Current, value);

    assert_eq!(strike_range(&rendered, value), (0, value.len() as u32));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9b_an_album_wide_change_renders_all_n_tracks_italic_and_muted() {
    if gtk4::init().is_err() {
        return;
    }
    let label = gtk4::Label::new(Some("All 11 tracks"));

    apply_album_wide_style(&label, true);

    assert!(label.has_css_class("doctor-album-wide-track"));
    assert!(label
        .attributes()
        .unwrap()
        .iterator()
        .get(gtk4::pango::AttrType::Style)
        .is_some());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_9b_a_recycled_row_loses_the_italic_style_again() {
    if gtk4::init().is_err() {
        return;
    }
    let label = gtk4::Label::new(Some("All 11 tracks"));
    apply_album_wide_style(&label, true);

    apply_album_wide_style(&label, false);

    assert!(!label.has_css_class("doctor-album-wide-track"));
    assert!(label.attributes().is_none());
}

#[test]
fn doc_9b_edge_spaces_are_visible_without_replacing_internal_spaces() {
    assert_eq!(visible_edge_spaces(" Panic Attack "), "␣Panic Attack␣");
    assert_eq!(visible_edge_spaces("   "), "␣␣␣");
}
