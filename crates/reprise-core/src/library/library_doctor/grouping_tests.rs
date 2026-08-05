use super::*;
use crate::library::tag_edit::EditableTags;

fn track(track_id: i64, album: &str, artist: &str) -> DoctorTrackSnapshot {
    DoctorTrackSnapshot {
        reference: DoctorTrackRef {
            track_id,
            path: format!("/fixture/{track_id}.flac").into(),
            file_mtime: 1,
            file_size: 2,
            device: Some(3),
            inode: Some(track_id),
        },
        tags: Some(EditableTags {
            title: format!("Track {track_id}"),
            artist: artist.into(),
            album: album.into(),
            album_artist: artist.into(),
            ..EditableTags::default()
        }),
        stale: false,
    }
}

fn proposal(track_id: i64, field: DoctorField) -> DoctorProposal {
    DoctorProposal {
        track_id,
        field,
        current: DoctorValue::Text("old".into()),
        proposed: DoctorValue::Text("new".into()),
        source: ProposalSource::MusicBrainz,
        confidence: 90,
        preselected: false,
        problem_class: ProblemClass::CasingWhitespace,
        evidence: Vec::new(),
        local_fallback: None,
    }
}

fn scan(tracks: Vec<DoctorTrackSnapshot>, proposals: Vec<DoctorProposal>) -> DoctorScan {
    DoctorScan {
        id: 1,
        scope_kind: "selection".into(),
        created_at: 2,
        options: DoctorScanOptions {
            remote_enabled: true,
        },
        checked_tracks: tracks.len(),
        skipped_tracks: 0,
        track_ids: tracks
            .iter()
            .map(|track| track.reference.track_id)
            .collect(),
        tracks,
        proposals,
        unresolved_groups: Vec::new(),
    }
}

#[test]
fn doc_9b_rows_group_by_album_in_scope_order() {
    let scan = scan(
        vec![
            track(2, "Second", "Artist B"),
            track(1, "First", "Artist A"),
            track(3, "Second", "Artist B"),
        ],
        vec![
            proposal(1, DoctorField::Title),
            proposal(2, DoctorField::Title),
            proposal(3, DoctorField::Artist),
        ],
    );
    let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);

    let albums = grouping::group_review_rows(&scan, &session);

    assert_eq!(
        albums
            .iter()
            .map(|album| album.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Second", "First"]
    );
    assert_eq!(albums[0].track_count, 2);
    assert_eq!(albums[1].track_count, 1);
}

#[test]
fn doc_9b_album_level_change_collapses_into_one_row_over_all_tracks() {
    let scan = scan(
        vec![track(1, "Album", "Artist"), track(2, "Album", "Artist")],
        vec![
            proposal(1, DoctorField::AlbumArtist),
            proposal(2, DoctorField::AlbumArtist),
        ],
    );
    let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);

    let albums = group_review_rows(&scan, &session);

    assert_eq!(albums.len(), 1);
    assert_eq!(albums[0].rows.len(), 1);
    assert!(matches!(
        &albums[0].rows[0],
        DoctorReviewDisplayRow::AllTracks {
            row_ids,
            track_count: 2,
        } if row_ids.len() == 2
    ));
}

#[test]
fn doc_9b_tracks_without_an_album_form_one_trailing_group() {
    let scan = scan(
        vec![
            track(1, "", "Artist A"),
            track(2, "Album", "Artist B"),
            track(3, "", "Artist C"),
        ],
        vec![
            proposal(1, DoctorField::Title),
            proposal(2, DoctorField::Title),
            proposal(3, DoctorField::Artist),
        ],
    );
    let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);

    let albums = group_review_rows(&scan, &session);

    assert_eq!(albums.len(), 2);
    assert_eq!(albums[0].title, "Album");
    assert_eq!(albums[1].key, "");
    assert_eq!(albums[1].title, "");
    assert_eq!(albums[1].track_count, 2);
    assert_eq!(albums[1].rows.len(), 2);
}

#[test]
fn doc_9b_group_counts_report_written_changes_not_display_rows() {
    let tracks = (1..=11)
        .map(|track_id| track(track_id, "Album", "Artist"))
        .collect::<Vec<_>>();
    let mut proposals = (1..=11)
        .map(|track_id| proposal(track_id, DoctorField::AlbumArtist))
        .collect::<Vec<_>>();
    proposals.extend((1..=3).map(|track_id| proposal(track_id, DoctorField::Title)));
    let scan = scan(tracks, proposals);
    let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);

    let albums = group_review_rows(&scan, &session);

    assert_eq!(albums.len(), 1);
    assert_eq!(albums[0].change_count, 14);
    assert_eq!(albums[0].rows.len(), 4);
}
