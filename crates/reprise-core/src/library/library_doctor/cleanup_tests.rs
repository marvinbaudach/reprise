use std::path::{Path, PathBuf};

use lofty::prelude::*;

use super::*;
use crate::library::tag_edit::{read_editable_tags, TagPatch};
use crate::library::tag_mutation::prepare_tag_mutation;
use crate::library::tag_write_job::{prepare_tag_write_job, TagWriteJobSpec};

fn fixture(dir: &Path, name: &str, artist: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let path = dir.join(name);
    std::fs::copy(source, &path).unwrap();
    let mut tagged = lofty::read_from_path(&path).unwrap();
    let tag = tagged.primary_tag_mut().unwrap();
    tag.set_title(name.to_owned());
    tag.set_artist(artist.to_owned());
    tag.set_album("Album".into());
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
    path
}

fn seed(db: &crate::db::Db, paths: &[PathBuf]) -> Vec<i64> {
    for path in paths {
        crate::library::scanner::scan_folder(db, path).unwrap();
    }
    paths
        .iter()
        .map(|path| {
            db.conn()
                .query_row(
                    "SELECT id FROM tracks WHERE path=?1",
                    [path.to_string_lossy().as_ref()],
                    |row| row.get(0),
                )
                .unwrap()
        })
        .collect()
}

fn plans_for_one_scan(db: &crate::db::Db, ids: &[i64]) -> (DoctorApplyPlan, DoctorApplyPlan) {
    let outcome = LibraryDoctor::new(db)
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
    let mut review = DoctorReviewSession::from_scan(scan, DoctorReviewFilter::AutoApply);
    let choices = review
        .rows()
        .iter()
        .map(|row| (row.id, row.track_id))
        .collect::<Vec<_>>();
    for (row_id, track_id) in &choices {
        review.set_selected(*row_id, *track_id == ids[0]).unwrap();
    }
    let first = review.freeze_plan();
    for (row_id, track_id) in choices {
        review.set_selected(row_id, track_id == ids[1]).unwrap();
    }
    (first, review.freeze_plan())
}

fn apply_pair(
    db: &crate::db::Db,
    paths: &[PathBuf],
) -> (Vec<i64>, DoctorWriteReport, DoctorWriteReport) {
    let ids = seed(db, paths);
    let (quiet, reviewed) = plans_for_one_scan(db, &ids);
    let quiet_report = LibraryDoctor::new(db)
        .apply_review_plan(&quiet, |_| DoctorWriteControl::Continue)
        .unwrap();
    let reviewed_report = LibraryDoctor::new(db)
        .apply_review_plan(&reviewed, |_| DoctorWriteControl::Continue)
        .unwrap();
    (ids, quiet_report, reviewed_report)
}

fn auto_plan(db: &crate::db::Db, track_id: i64) -> DoctorApplyPlan {
    let outcome = LibraryDoctor::new(db)
        .scan_local(
            &LocalScanRequest {
                scope: DoctorScopeRequest::Selection {
                    track_ids: vec![track_id],
                },
            },
            |_| ScanControl::Continue,
        )
        .unwrap();
    let DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("scan must complete")
    };
    DoctorReviewSession::from_scan(scan, DoctorReviewFilter::AutoApply).freeze_plan()
}

fn prepared_tag_editor_job(db: &crate::db::Db, track_id: i64, path: &Path) -> i64 {
    let mutation = prepare_tag_mutation(
        db.conn(),
        track_id,
        path,
        &TagPatch {
            title: Some("Edited title".into()),
            ..TagPatch::default()
        },
    )
    .unwrap()
    .unwrap();
    prepare_tag_write_job(db.conn(), TagWriteJobSpec::tag_editor(), &[(0, mutation)])
        .unwrap()
        .id
}

#[test]
fn doc_10a_undo_reverts_the_quiet_and_the_reviewed_job_of_one_scan() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![
        fixture(dir.path(), "quiet.flac", " Quiet "),
        fixture(dir.path(), "reviewed.flac", " Reviewed "),
    ];
    let db = crate::db::Db::open_in_memory().unwrap();
    let (_, quiet, reviewed) = apply_pair(&db, &paths);

    let cleanup = LibraryDoctor::new(&db)
        .revert_last_cleanup(|_| DoctorWriteControl::Continue)
        .unwrap()
        .unwrap();

    assert_eq!(cleanup.reverted_tracks, 2);
    assert_eq!(cleanup.reports.len(), 2);
    assert_eq!(cleanup.reports[0].source_job_id, Some(reviewed.job_id));
    assert_eq!(cleanup.reports[1].source_job_id, Some(quiet.job_id));
    assert_eq!(read_editable_tags(&paths[0]).unwrap().artist, " Quiet ");
    assert_eq!(read_editable_tags(&paths[1]).unwrap().artist, " Reviewed ");
}

#[test]
fn doc_10a_undo_works_when_only_the_quiet_job_exists() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "quiet-only.flac", " Quiet ");
    let db = crate::db::Db::open_in_memory().unwrap();
    let ids = seed(&db, std::slice::from_ref(&path));
    let outcome = LibraryDoctor::new(&db)
        .scan_local(
            &LocalScanRequest {
                scope: DoctorScopeRequest::Selection { track_ids: ids },
            },
            |_| ScanControl::Continue,
        )
        .unwrap();
    let DoctorScanOutcome::Completed(scan) = outcome else {
        panic!("scan must complete")
    };
    let plan = DoctorReviewSession::from_scan(scan, DoctorReviewFilter::AutoApply).freeze_plan();
    LibraryDoctor::new(&db)
        .apply_review_plan(&plan, |_| DoctorWriteControl::Continue)
        .unwrap();

    let cleanup = LibraryDoctor::new(&db)
        .revert_last_cleanup(|_| DoctorWriteControl::Continue)
        .unwrap()
        .unwrap();

    assert_eq!(cleanup.reports.len(), 1);
    assert_eq!(cleanup.reverted_tracks, 1);
    assert_eq!(read_editable_tags(&path).unwrap().artist, " Quiet ");
}

#[test]
fn doc_10a_partial_revert_leaves_the_cleanup_available_for_a_second_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![
        fixture(dir.path(), "quiet-partial.flac", " Quiet "),
        fixture(dir.path(), "reviewed-partial.flac", " Reviewed "),
    ];
    let db = crate::db::Db::open_in_memory().unwrap();
    let (_, quiet, _) = apply_pair(&db, &paths);
    crate::library::tag_edit::apply_patch_to_file(
        &paths[0],
        &TagPatch {
            artist: Some("External".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();
    assert_eq!(read_editable_tags(&paths[0]).unwrap().artist, "External");

    let partial = LibraryDoctor::new(&db)
        .revert_last_cleanup(|_| DoctorWriteControl::Continue)
        .unwrap()
        .unwrap();

    assert_eq!(partial.reverted_tracks, 2);
    assert_eq!(partial.conflict_tracks, 1);
    assert_eq!(
        LibraryDoctor::new(&db)
            .last_cleanup()
            .unwrap()
            .unwrap()
            .job_ids,
        vec![quiet.job_id]
    );
    crate::library::tag_edit::apply_patch_to_file(
        &paths[0],
        &TagPatch {
            artist: Some("Quiet".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();
    let retry = LibraryDoctor::new(&db)
        .revert_last_cleanup(|_| DoctorWriteControl::Continue)
        .unwrap()
        .unwrap();
    assert_eq!(retry.reverted_tracks, 1);
    assert!(LibraryDoctor::new(&db).last_cleanup().unwrap().is_none());
}

#[test]
fn doc_10a_cancel_between_jobs_does_not_start_the_remaining_job() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![
        fixture(dir.path(), "quiet-cancel.flac", " Quiet "),
        fixture(dir.path(), "reviewed-cancel.flac", " Reviewed "),
    ];
    let db = crate::db::Db::open_in_memory().unwrap();
    let (_, quiet, _) = apply_pair(&db, &paths);

    let cleanup = LibraryDoctor::new(&db)
        .revert_last_cleanup(|progress| {
            if progress.completed_tracks == 1 {
                DoctorWriteControl::Cancel
            } else {
                DoctorWriteControl::Continue
            }
        })
        .unwrap()
        .unwrap();

    assert!(cleanup.cancelled);
    assert_eq!(cleanup.reports.len(), 1);
    assert_eq!(read_editable_tags(&paths[0]).unwrap().artist, "Quiet");
    assert_eq!(read_editable_tags(&paths[1]).unwrap().artist, " Reviewed ");
    assert_eq!(
        db.conn()
            .query_row(
                "SELECT COUNT(*) FROM tag_write_jobs WHERE kind='doctor_revert'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
    assert_eq!(
        LibraryDoctor::new(&db)
            .last_cleanup()
            .unwrap()
            .unwrap()
            .job_ids,
        vec![quiet.job_id]
    );
}

#[test]
fn doc_10a_a_fully_reverted_scan_is_no_longer_offered() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![
        fixture(dir.path(), "quiet-full.flac", " Quiet "),
        fixture(dir.path(), "reviewed-full.flac", " Reviewed "),
    ];
    let db = crate::db::Db::open_in_memory().unwrap();
    apply_pair(&db, &paths);

    LibraryDoctor::new(&db)
        .revert_last_cleanup(|_| DoctorWriteControl::Continue)
        .unwrap()
        .unwrap();

    assert!(LibraryDoctor::new(&db).last_cleanup().unwrap().is_none());
}

#[test]
fn doc_10b_a_second_tag_write_job_is_refused_while_one_is_prepared_or_running() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "busy-states.flac", "Artist");
    let db = crate::db::Db::open_in_memory().unwrap();
    let track_id = seed(&db, std::slice::from_ref(&path))[0];
    let job_id = prepared_tag_editor_job(&db, track_id, &path);
    let mutation = prepare_tag_mutation(
        db.conn(),
        track_id,
        &path,
        &TagPatch {
            title: Some("Another title".into()),
            ..TagPatch::default()
        },
    )
    .unwrap()
    .unwrap();

    let prepared_error = prepare_tag_write_job(
        db.conn(),
        TagWriteJobSpec::tag_editor(),
        &[(0, mutation.clone())],
    )
    .unwrap_err();
    assert_eq!(
        prepared_error.to_string(),
        "another tag-writing job is already running"
    );
    db.conn()
        .execute(
            "UPDATE tag_write_jobs SET state='running' WHERE id=?1",
            [job_id],
        )
        .unwrap();
    let running_error =
        prepare_tag_write_job(db.conn(), TagWriteJobSpec::tag_editor(), &[(0, mutation)])
            .unwrap_err();
    assert_eq!(
        running_error.to_string(),
        "another tag-writing job is already running"
    );
}

#[test]
fn doc_10b_a_finalized_interrupted_job_does_not_hold_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "interrupted.flac", " Artist ");
    let db = crate::db::Db::open_in_memory().unwrap();
    let track_id = seed(&db, std::slice::from_ref(&path))[0];
    let plan = auto_plan(&db, track_id);
    let change = &plan.changes()[0];
    let input = super::write::InputChange {
        row_id: Some(change.row_id),
        track: change.track.clone(),
        field: change.field,
        expected: change.expected.clone(),
        proposed: change.proposed.clone(),
    };
    super::write::prepare_job(
        db.conn(),
        "doctor_apply",
        None,
        Some(plan.scan_id()),
        &[input],
    )
    .unwrap();

    let recovered = LibraryDoctor::new(&db)
        .finalize_incomplete_writes()
        .unwrap();
    assert_eq!(recovered.len(), 1);
    let report = LibraryDoctor::new(&db)
        .apply_review_plan(&plan, |_| DoctorWriteControl::Continue)
        .unwrap();
    assert_eq!(report.updated_tracks, 1);
}

#[test]
fn doc_10b_tag_editor_and_doctor_share_one_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "shared-lock.flac", " Artist ");
    let db = crate::db::Db::open_in_memory().unwrap();
    let track_id = seed(&db, std::slice::from_ref(&path))[0];
    let plan = auto_plan(&db, track_id);
    prepared_tag_editor_job(&db, track_id, &path);

    let error = LibraryDoctor::new(&db)
        .apply_review_plan(&plan, |_| DoctorWriteControl::Continue)
        .unwrap_err();

    assert!(matches!(error, DoctorError::TagWriteBusy(_)));
}

#[test]
fn doc_10b_gui_sees_the_same_refusal_while_an_mcp_job_runs() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture(dir.path(), "foreign-lock.flac", " Artist ");
    let db = crate::db::Db::open_in_memory().unwrap();
    let track_id = seed(&db, std::slice::from_ref(&path))[0];
    let plan = auto_plan(&db, track_id);
    db.conn()
        .execute(
            "INSERT INTO tag_write_jobs \
             (kind, source_job_id, scan_id, state, created_at, finished_at, total_tracks) \
             VALUES ('doctor_apply', NULL, ?1, 'prepared', 1, NULL, 0)",
            [plan.scan_id()],
        )
        .unwrap();

    let error = LibraryDoctor::new(&db)
        .apply_review_plan(&plan, |_| DoctorWriteControl::Continue)
        .unwrap_err();

    assert!(matches!(error, DoctorError::TagWriteBusy(_)));
}
