use std::path::{Path, PathBuf};

use lofty::prelude::*;

use super::*;

fn fixture(dir: &Path) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let path = dir.join("snapshot-refresh.flac");
    std::fs::copy(source, &path).unwrap();
    let mut tagged = lofty::read_from_path(&path).unwrap();
    let tag = tagged.primary_tag_mut().unwrap();
    tag.set_title(" Title ".to_owned());
    tag.set_artist(" Artist ".to_owned());
    tag.set_album("Album".to_owned());
    tag.remove_key(lofty::tag::ItemKey::AlbumArtist);
    tag.set_genre("Rock".to_owned());
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
    path
}

fn scan_track(db: &crate::db::Db, path: &Path) -> DoctorScan {
    crate::library::scanner::scan_folder(db, path).unwrap();
    let track_id = db
        .conn()
        .query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    match LibraryDoctor::new(db)
        .scan_local(
            &LocalScanRequest {
                scope: DoctorScopeRequest::Selection {
                    track_ids: vec![track_id],
                },
            },
            |_| ScanControl::Continue,
        )
        .unwrap()
    {
        DoctorScanOutcome::Completed(scan) => scan,
        outcome => panic!("expected a completed scan, got {outcome:?}"),
    }
}

#[test]
fn doctor_apply_refreshes_snapshot_before_remaining_rows_are_classified() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path());
    let db = crate::db::Db::open_in_memory().unwrap();
    let scan = scan_track(&db, &path);
    let track_id = scan.track_ids[0];
    let mut review = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::AutoApply);
    assert_eq!(
        review.rows().len(),
        3,
        "fixture must create three proposals"
    );
    let choices = review
        .rows()
        .iter()
        .map(|row| (row.id, row.field == DoctorField::Title))
        .collect::<Vec<_>>();
    for (row_id, selected) in choices {
        review.set_selected(row_id, selected).unwrap();
    }
    assert_eq!(review.freeze_plan().tag_change_count(), 1);

    LibraryDoctor::new(&db)
        .apply_review_plan(&review.freeze_plan(), |_| DoctorWriteControl::Continue)
        .unwrap();

    let stored = LibraryDoctor::new(&db)
        .last_complete_scan()
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.proposals.len(),
        2,
        "only the written row leaves the scan"
    );
    let stale = stale_flags(db.conn(), scan.id).unwrap()[&track_id];
    let further_stale_rows = usize::from(stale) * stored.proposals.len();
    assert_eq!(
        further_stale_rows, 0,
        "one apply staled {further_stale_rows} further rows on the same track"
    );

    let remaining = DoctorReviewSession::from_scan(stored, DoctorReviewFilter::AutoApply);
    assert_eq!(remaining.rows().len(), 2);
    assert!(remaining
        .rows()
        .iter()
        .all(|row| row.state == DoctorReviewRowState::Ready));
}

#[test]
fn badge_and_review_share_one_fingerprint_comparison() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path());
    let db = crate::db::Db::open_in_memory().unwrap();
    let scan = scan_track(&db, &path);
    let track_id = scan.track_ids[0];
    db.conn()
        .execute(
            "UPDATE tracks SET file_mtime = file_mtime + 1 WHERE id=?1",
            [track_id],
        )
        .unwrap();

    let badge_stale = stale_flags(db.conn(), scan.id)
        .unwrap()
        .into_iter()
        .filter_map(|(track_id, stale)| stale.then_some(track_id))
        .collect::<std::collections::HashSet<_>>();
    let review_stale = LibraryDoctor::new(&db)
        .last_complete_scan()
        .unwrap()
        .unwrap()
        .stale_track_ids()
        .into_iter()
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(badge_stale, review_stale);
    assert_eq!(badge_stale, std::collections::HashSet::from([track_id]));

    let source = include_str!("store.rs");
    assert_eq!(
        source.matches(".is_none_or(|current| current !=").count(),
        1,
        "the badge and session must not implement fingerprint comparison separately"
    );
}

#[test]
fn fingerprint_stale_tracks_are_absent_from_session_and_badge() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path());
    let db = crate::db::Db::open_in_memory().unwrap();
    let scan = scan_track(&db, &path);
    let track_id = scan.track_ids[0];
    db.conn()
        .execute(
            "UPDATE tracks SET file_mtime = file_mtime + 1 WHERE id=?1",
            [track_id],
        )
        .unwrap();

    let stored = LibraryDoctor::new(&db)
        .last_complete_scan()
        .unwrap()
        .unwrap();
    assert_eq!(stored.stale_track_ids(), vec![track_id]);

    let review = DoctorReviewSession::from_scan(stored, DoctorReviewFilter::NeedsReview);
    let badge = crate::queries::count_doctor_findings(&db).unwrap();

    assert!(review.rows().is_empty());
    assert_eq!(badge.ready, 0);
    assert_eq!(badge.stale, 0);
}
