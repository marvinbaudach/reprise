use std::path::{Path, PathBuf};

use lofty::prelude::*;

use super::*;
use crate::library::tag_edit::{
    apply_track_writes, read_editable_tags, TagPatch, TrackEditPatch, TrackWrite,
};

fn fixture(dir: &Path, name: &str, artist: &str, album: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let path = dir.join(name);
    std::fs::copy(source, &path).unwrap();
    let mut tagged = lofty::read_from_path(&path).unwrap();
    let tag = tagged.primary_tag_mut().unwrap();
    tag.set_title(name.to_owned());
    tag.set_artist(artist.to_owned());
    tag.set_album(album.to_owned());
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
    path
}

fn seed(conn: &mut rusqlite::Connection, paths: &[PathBuf]) -> Vec<i64> {
    for path in paths {
        crate::library::scanner::scan_folder(conn, path).unwrap();
    }
    paths
        .iter()
        .map(|path| {
            conn.query_row(
                "SELECT id FROM tracks WHERE path=?1",
                [path.to_string_lossy().as_ref()],
                |row| row.get(0),
            )
            .unwrap()
        })
        .collect()
}

fn plan_for(
    conn: &mut rusqlite::Connection,
    ids: &[i64],
    select: impl Fn(&DoctorReviewRow) -> bool,
) -> DoctorApplyPlan {
    let outcome = LibraryDoctor::new(conn)
        .scan_local(
            &LocalScanRequest {
                scope: DoctorScopeRequest::Selection {
                    track_ids: ids.to_vec(),
                },
            },
            |_| ScanControl::Continue,
        )
        .unwrap();
    let DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("scan must complete")
    };
    let mut review = DoctorReviewSession::from_scan(scan, DoctorReviewFilter::AllChanges);
    let choices = review
        .rows()
        .iter()
        .map(|row| (row.id, select(row)))
        .collect::<Vec<_>>();
    for (row, selected) in choices {
        review.set_selected(row, selected).unwrap();
    }
    review.freeze_plan()
}

fn continue_job(_: DoctorWriteProgress) -> DoctorWriteControl {
    DoctorWriteControl::Continue
}

struct PaddedRemoteResolver;

impl super::remote::RemoteResolver for PaddedRemoteResolver {
    fn resolve_track(
        &mut self,
        metadata: &super::remote::RemoteTrackMetadata,
        _: &Path,
        _: Option<&dyn crate::fingerprint::FingerprintBackend>,
        _: &mut dyn FnMut() -> ScanControl,
    ) -> Result<super::remote::RemoteResolution, super::remote::RemoteProviderError> {
        Ok(super::remote::arbitrate(
            metadata,
            &[super::remote::RemoteIdentity {
                source: super::remote::RemoteEvidenceSource::MusicBrainz,
                confidence: 100,
                recording_mbid: None,
                release_mbid: None,
                release_group_mbid: None,
                artist_mbid: None,
                release_artist_mbid: None,
                title: None,
                artist: Some("Canonical artist".into()),
                album: None,
                album_artist: None,
                release_year: None,
                original_release_year: None,
                duration_ms: None,
            }],
        ))
    }
}

#[test]
fn doc_apply_writes_only_checked() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "checked.flac", " Artist ", " Album ");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |row| row.field == DoctorField::Artist);

    let report = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();

    let tags = read_editable_tags(&path).unwrap();
    assert_eq!(tags.artist, "Artist");
    assert_eq!(tags.album, " Album ");
    assert_eq!(report.updated_tracks, 1);
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].state, DoctorWriteRowState::Applied);
}

#[test]
fn remote_review_preserves_raw_current_tag_for_guarded_apply() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "padded.flac", " Old artist ", "Album");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let mut resolver = PaddedRemoteResolver;
    let outcome = LibraryDoctor::new(&mut conn)
        .scan_with_resolver(
            &DoctorScanRequest {
                scope: DoctorScopeRequest::Selection {
                    track_ids: ids.clone(),
                },
                options: DoctorScanOptions {
                    remote_enabled: true,
                },
            },
            None,
            &mut resolver,
            &mut |_| ScanControl::Continue,
        )
        .unwrap();
    let DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("scan must complete")
    };
    let mut review = DoctorReviewSession::from_scan(scan, DoctorReviewFilter::AllChanges);
    let row = review
        .rows()
        .iter()
        .find(|row| row.field == DoctorField::Artist)
        .unwrap();
    assert_eq!(row.source, ProposalSource::MusicBrainz);
    assert_eq!(row.current, DoctorValue::Text(" Old artist ".into()));
    let row_id = row.id;
    review.set_selected(row_id, true).unwrap();

    let report = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&review.freeze_plan(), continue_job)
        .unwrap();

    assert_eq!(
        read_editable_tags(&path).unwrap().artist,
        "Canonical artist"
    );
    assert_eq!(report.updated_tracks, 1);
    assert_eq!(report.conflict_tracks, 0);
}

#[test]
fn doc_undo_restores_previous_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "undo.flac", " Artist ", "Album");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |_| true);
    let apply = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();
    assert_eq!(read_editable_tags(&path).unwrap().artist, "Artist");

    let revert = LibraryDoctor::new(&mut conn)
        .revert_last_cleanup(continue_job)
        .unwrap()
        .unwrap();

    assert_eq!(revert.source_job_id, Some(apply.job_id));
    assert_eq!(read_editable_tags(&path).unwrap().artist, " Artist ");
    assert!(LibraryDoctor::new(&mut conn)
        .last_cleanup()
        .unwrap()
        .is_none());
}

#[test]
fn doc_5a_sibling_field_conflict_does_not_block_valid_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "siblings.flac", " Artist ", " Album ");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |_| true);
    crate::library::tag_edit::apply_patch_to_file(
        &path,
        &TagPatch {
            artist: Some("External".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();

    let report = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();

    let tags = read_editable_tags(&path).unwrap();
    assert_eq!(tags.artist, "External");
    assert_eq!(tags.album, "Album");
    assert_eq!(report.updated_tracks, 1);
    assert_eq!(report.conflict_tracks, 1);
    assert_eq!(
        report
            .rows
            .iter()
            .find(|row| row.field == DoctorField::Artist)
            .unwrap()
            .state,
        DoctorWriteRowState::Conflict
    );
}

#[test]
fn doc_5a_moved_file_is_unavailable_not_failed() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "missing.flac", " Artist ", "Album");
    let moved = dir.path().join("moved.flac");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |_| true);
    std::fs::rename(&path, &moved).unwrap();

    let report = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();

    assert_eq!(report.unavailable_tracks, 1);
    assert_eq!(report.failed_tracks, 0);
    assert_eq!(report.rows[0].state, DoctorWriteRowState::Unavailable);
}

#[test]
fn doc_5b_cancel_stops_between_files_and_preserves_completed_write() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![
        fixture(dir.path(), "one.flac", " One ", "Album"),
        fixture(dir.path(), "two.flac", " Two ", "Album"),
    ];
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, &paths);
    let plan = plan_for(&mut conn, &ids, |_| true);

    let report = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, |progress| {
            if progress.completed_tracks == 1 {
                DoctorWriteControl::Cancel
            } else {
                DoctorWriteControl::Continue
            }
        })
        .unwrap();

    assert_eq!(read_editable_tags(&paths[0]).unwrap().artist, "One");
    assert_eq!(read_editable_tags(&paths[1]).unwrap().artist, " Two ");
    assert_eq!(report.updated_tracks, 1);
    assert_eq!(report.cancelled_tracks, 1);
    assert!(report
        .rows
        .iter()
        .filter(|row| row.track_id == ids[0])
        .all(|row| { row.state == DoctorWriteRowState::Applied }));
    assert!(report
        .rows
        .iter()
        .filter(|row| row.track_id == ids[1])
        .all(|row| { row.state == DoctorWriteRowState::Cancelled }));
    assert_eq!(
        LibraryDoctor::new(&mut conn)
            .last_cleanup()
            .unwrap()
            .unwrap()
            .job_id,
        report.job_id
    );
}

#[test]
fn tag_editor_job_never_replaces_doctor_cleanup_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "pointer.flac", " Artist ", "Album");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |_| true);
    let apply = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();

    let _ = apply_track_writes(
        &mut conn,
        &[TrackWrite {
            id: ids[0],
            path,
            patch: TrackEditPatch {
                tags: TagPatch {
                    album: Some("Changed later".into()),
                    ..TagPatch::default()
                },
                rating: None,
            },
        }],
        &mut |_, _| {},
    );

    assert_eq!(
        LibraryDoctor::new(&mut conn)
            .last_cleanup()
            .unwrap()
            .unwrap()
            .job_id,
        apply.job_id
    );
}

#[test]
fn partial_revert_keeps_pointer_and_full_revert_reveals_previous_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![
        fixture(dir.path(), "first.flac", " First ", "Album"),
        fixture(dir.path(), "second.flac", " Second ", "Album"),
    ];
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, &paths);

    let first_plan = plan_for(&mut conn, &ids[..1], |_| true);
    let first_apply = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&first_plan, continue_job)
        .unwrap();
    let second_plan = plan_for(&mut conn, &ids[1..], |_| true);
    let second_apply = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&second_plan, continue_job)
        .unwrap();
    assert_eq!(
        LibraryDoctor::new(&mut conn)
            .last_cleanup()
            .unwrap()
            .unwrap()
            .job_id,
        second_apply.job_id
    );

    crate::library::tag_edit::apply_patch_to_file(
        &paths[1],
        &TagPatch {
            artist: Some("External".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();
    let partial = LibraryDoctor::new(&mut conn)
        .revert_last_cleanup(continue_job)
        .unwrap()
        .unwrap();
    assert_eq!(partial.conflict_tracks, 1);
    assert_eq!(
        LibraryDoctor::new(&mut conn)
            .last_cleanup()
            .unwrap()
            .unwrap()
            .job_id,
        second_apply.job_id
    );

    crate::library::tag_edit::apply_patch_to_file(
        &paths[1],
        &TagPatch {
            artist: Some("Second".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();
    let complete = LibraryDoctor::new(&mut conn)
        .revert_last_cleanup(continue_job)
        .unwrap()
        .unwrap();
    assert_eq!(complete.updated_tracks, 1);
    assert_eq!(
        LibraryDoctor::new(&mut conn)
            .last_cleanup()
            .unwrap()
            .unwrap()
            .job_id,
        first_apply.job_id
    );
}

#[test]
fn doc_5b_revert_cancel_preserves_completed_and_unstarted_rows() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![
        fixture(dir.path(), "revert-one.flac", " One ", "Album"),
        fixture(dir.path(), "revert-two.flac", " Two ", "Album"),
    ];
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, &paths);
    let plan = plan_for(&mut conn, &ids, |row| row.field == DoctorField::Artist);
    let apply = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();

    let revert = LibraryDoctor::new(&mut conn)
        .revert_last_cleanup(|progress| {
            if progress.completed_tracks == 1 {
                DoctorWriteControl::Cancel
            } else {
                DoctorWriteControl::Continue
            }
        })
        .unwrap()
        .unwrap();

    assert_eq!(revert.updated_tracks, 1);
    assert_eq!(revert.cancelled_tracks, 1);
    assert_eq!(read_editable_tags(&paths[0]).unwrap().artist, " One ");
    assert_eq!(read_editable_tags(&paths[1]).unwrap().artist, "Two");
    assert_eq!(
        LibraryDoctor::new(&mut conn)
            .last_cleanup()
            .unwrap()
            .unwrap()
            .job_id,
        apply.job_id
    );

    let final_revert = LibraryDoctor::new(&mut conn)
        .revert_last_cleanup(continue_job)
        .unwrap()
        .unwrap();
    assert_eq!(final_revert.updated_tracks, 1);
    assert_eq!(read_editable_tags(&paths[1]).unwrap().artist, " Two ");
    assert!(LibraryDoctor::new(&mut conn)
        .last_cleanup()
        .unwrap()
        .is_none());
}

#[test]
fn doc_5b_post_write_failure_reports_file_truth_and_remains_revertible() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "reconcile.flac", " Artist ", "Album");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |row| row.field == DoctorField::Artist);
    conn.execute_batch(
        "CREATE TRIGGER reject_doctor_reconcile BEFORE UPDATE OF file_mtime ON tracks \
         BEGIN SELECT RAISE(ABORT, 'reconcile blocked'); END;",
    )
    .unwrap();

    let report = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();

    assert_eq!(read_editable_tags(&path).unwrap().artist, "Artist");
    assert_eq!(report.updated_tracks, 1);
    assert_eq!(report.failed_tracks, 1);
    assert!(report.rows[0].file_written);
    assert_eq!(report.rows[0].state, DoctorWriteRowState::Applied);
    assert_eq!(
        LibraryDoctor::new(&mut conn)
            .last_cleanup()
            .unwrap()
            .unwrap()
            .job_id,
        report.job_id
    );
}

#[test]
fn doc_5b_post_write_revert_failure_consumes_the_fields_that_were_restored() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "revert-reconcile.flac", " Artist ", "Album");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |row| row.field == DoctorField::Artist);
    LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();
    conn.execute_batch(
        "CREATE TRIGGER reject_revert_reconcile BEFORE UPDATE OF file_mtime ON tracks \
         BEGIN SELECT RAISE(ABORT, 'reconcile blocked'); END;",
    )
    .unwrap();

    let report = LibraryDoctor::new(&mut conn)
        .revert_last_cleanup(continue_job)
        .unwrap()
        .unwrap();

    assert_eq!(read_editable_tags(&path).unwrap().artist, " Artist ");
    assert_eq!(report.updated_tracks, 1);
    assert_eq!(report.failed_tracks, 1);
    assert!(report.rows[0].file_written);
    assert!(LibraryDoctor::new(&mut conn)
        .last_cleanup()
        .unwrap()
        .is_none());
}

#[test]
fn doc_5a_post_write_failure_keeps_sibling_conflict_exact() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "mixed-failure.flac", " Artist ", " Album ");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |_| true);
    crate::library::tag_edit::apply_patch_to_file(
        &path,
        &TagPatch {
            artist: Some("External".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();
    conn.execute_batch(
        "CREATE TRIGGER reject_mixed_reconcile BEFORE UPDATE OF file_mtime ON tracks \
         BEGIN SELECT RAISE(ABORT, 'reconcile blocked'); END;",
    )
    .unwrap();

    let report = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();

    assert_eq!(read_editable_tags(&path).unwrap().artist, "External");
    assert_eq!(read_editable_tags(&path).unwrap().album, "Album");
    assert_eq!(report.updated_tracks, 1);
    assert_eq!(report.conflict_tracks, 1);
    assert_eq!(report.failed_tracks, 1);
    assert_eq!(
        report
            .rows
            .iter()
            .find(|row| row.field == DoctorField::Artist)
            .unwrap()
            .state,
        DoctorWriteRowState::Conflict
    );
}

#[test]
fn doc_5a_removed_track_is_unavailable_without_a_file_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "removed.flac", " Artist ", "Album");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |_| true);
    conn.execute("UPDATE tracks SET removed_at=1 WHERE id=?1", [ids[0]])
        .unwrap();

    let report = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();

    assert_eq!(report.unavailable_tracks, 1);
    assert_eq!(report.updated_tracks, 0);
    assert!(report.rows.iter().all(|row| !row.file_written));
    assert_eq!(read_editable_tags(&path).unwrap().artist, " Artist ");
}

#[test]
fn doc_5a_recording_mbid_uses_the_guarded_review_write_path() {
    use lofty::tag::ItemKey;

    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "mbid.flac", "Artist", "Album");
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let outcome = LibraryDoctor::new(&mut conn)
        .scan_local(
            &LocalScanRequest {
                scope: DoctorScopeRequest::Selection {
                    track_ids: ids.clone(),
                },
            },
            |_| ScanControl::Continue,
        )
        .unwrap();
    let DoctorScanOutcome::Completed(mut scan) = outcome else {
        panic!("scan must complete")
    };
    scan.proposals.push(DoctorProposal {
        track_id: ids[0],
        field: DoctorField::RecordingMbid,
        current: DoctorValue::Empty,
        proposed: DoctorValue::Text("recording-id".into()),
        source: ProposalSource::MusicBrainz,
        confidence: 100,
        preselected: false,
        problem_class: ProblemClass::MissingRecordingMbid,
        evidence: Vec::new(),
        local_fallback: None,
    });
    let mut review = DoctorReviewSession::from_scan(scan, DoctorReviewFilter::AllChanges);
    let row = review
        .rows()
        .iter()
        .find(|row| row.field == DoctorField::RecordingMbid)
        .unwrap()
        .id;
    review.set_selected(row, true).unwrap();

    let report = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&review.freeze_plan(), continue_job)
        .unwrap();

    let tagged = lofty::read_from_path(&path).unwrap();
    assert_eq!(
        tagged
            .primary_tag()
            .unwrap()
            .get_string(ItemKey::MusicBrainzRecordingId),
        Some("recording-id")
    );
    assert_eq!(report.updated_tracks, 1);
}

#[test]
fn doctor_cleanup_pointer_survives_database_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "restart.flac", " Artist ", "Album");
    let database = dir.path().join("reprise.db");
    let job_id = {
        let mut conn = crate::db::open(Some(&database)).unwrap();
        crate::db::migrate(&conn).unwrap();
        let ids = seed(&mut conn, std::slice::from_ref(&path));
        let plan = plan_for(&mut conn, &ids, |_| true);
        LibraryDoctor::new(&mut conn)
            .apply_review_plan(&plan, continue_job)
            .unwrap()
            .job_id
    };

    let mut reopened = crate::db::open(Some(&database)).unwrap();
    crate::db::migrate(&reopened).unwrap();
    assert_eq!(
        LibraryDoctor::new(&mut reopened)
            .last_cleanup()
            .unwrap()
            .unwrap()
            .job_id,
        job_id
    );
}

#[test]
fn doctor_apply_crash_finalization_is_db_only_and_preserves_revert_reachability() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "apply-crash.flac", " Artist ", "Album");
    let database = dir.path().join("apply-crash.db");
    let mut conn = crate::db::open_migrated(Some(&database)).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |row| row.field == DoctorField::Artist);
    conn.execute_batch(
        "CREATE TRIGGER reject_doctor_apply_status BEFORE UPDATE OF state ON tag_write_job_files \
         WHEN NEW.state='complete' BEGIN SELECT RAISE(ABORT, 'status blocked'); END;",
    )
    .unwrap();

    assert!(LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .is_err());
    assert_eq!(read_editable_tags(&path).unwrap().artist, "Artist");
    conn.execute_batch("DROP TRIGGER reject_doctor_apply_status;")
        .unwrap();
    drop(conn);
    let before_finalize = std::fs::read(&path).unwrap();

    let mut reopened = crate::db::open_migrated(Some(&database)).unwrap();
    let reports = LibraryDoctor::new(&mut reopened)
        .finalize_incomplete_writes()
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), before_finalize);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].updated_tracks, 1);
    assert!(LibraryDoctor::new(&mut reopened)
        .last_cleanup()
        .unwrap()
        .is_some());
    LibraryDoctor::new(&mut reopened)
        .revert_last_cleanup(continue_job)
        .unwrap()
        .unwrap();
    assert_eq!(read_editable_tags(&path).unwrap().artist, " Artist ");
}

#[test]
fn doctor_revert_crash_finalization_consumes_source_without_rewriting_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "revert-crash.flac", " Artist ", "Album");
    let database = dir.path().join("revert-crash.db");
    let mut conn = crate::db::open_migrated(Some(&database)).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |row| row.field == DoctorField::Artist);
    LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();
    conn.execute_batch(
        "CREATE TRIGGER reject_doctor_revert_status BEFORE UPDATE OF state ON tag_write_job_files \
         WHEN NEW.state='complete' BEGIN SELECT RAISE(ABORT, 'status blocked'); END;",
    )
    .unwrap();

    assert!(LibraryDoctor::new(&mut conn)
        .revert_last_cleanup(continue_job)
        .is_err());
    assert_eq!(read_editable_tags(&path).unwrap().artist, " Artist ");
    conn.execute_batch("DROP TRIGGER reject_doctor_revert_status;")
        .unwrap();
    drop(conn);
    let before_finalize = std::fs::read(&path).unwrap();

    let mut reopened = crate::db::open_migrated(Some(&database)).unwrap();
    let reports = LibraryDoctor::new(&mut reopened)
        .finalize_incomplete_writes()
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), before_finalize);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].updated_tracks, 1);
    assert!(LibraryDoctor::new(&mut reopened)
        .last_cleanup()
        .unwrap()
        .is_none());
}

#[test]
fn doc_5a_unreadable_file_is_failed_not_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "unreadable-apply.flac", " Artist ", "Album");
    let mut conn = crate::db::open_migrated(None).unwrap();
    let ids = seed(&mut conn, std::slice::from_ref(&path));
    let plan = plan_for(&mut conn, &ids, |row| row.field == DoctorField::Artist);
    std::fs::write(&path, b"not an audio container").unwrap();

    let report = LibraryDoctor::new(&mut conn)
        .apply_review_plan(&plan, continue_job)
        .unwrap();

    assert_eq!(report.failed_tracks, 1);
    assert_eq!(report.unavailable_tracks, 0);
    assert_eq!(report.rows[0].state, DoctorWriteRowState::Failed);
    assert!(!report.rows[0].file_written);
    assert!(report.rows[0].error.is_some());
}
